use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::logger;

// Import your in-process filesystem tools (Option 3).
// Adjust the path if your module layout differs.
use super::fs_tools::FsTools;

const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
const DEFAULT_MODEL: &str = "gpt-5-nano";

// ------------------------------- Public API --------------------------------

/// Backwards-compatible: plain prompt -> text (no custom tools).
pub fn get_ai_response(api_key: &str, prompt: &str) -> Result<Option<String>, reqwest::Error> {
    let client = reqwest::blocking::Client::new();

    let input = vec![json!({ "role": "user", "content": prompt })];
    let tools = vec![json!({ "type": "web_search" })];

    let body = ResponsesRequestBody {
        model: DEFAULT_MODEL.to_string(),
        input,
        tools,
        instructions: None,
        store: Some(false),
        parallel_tool_calls: Some(false),
    };

    let res_text = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&body)
        .timeout(Duration::from_secs(60 * 2))
        .send()?
        .text()?;

    logger::debug(&format!("Got AI Response: {res_text}"));

    match serde_json::from_str::<ResponsesResponse>(&res_text) {
        Ok(resp) => Ok(extract_output_text(&resp.output)),
        Err(e) => Ok(Some(e.to_string())),
    }
}

/// Tool-capable: prompt -> model may call local filesystem tools -> final text.
pub fn get_ai_response_with_fs(
    api_key: &str,
    prompt: &str,
    fs: &FsTools,
) -> Result<Option<String>, anyhow::Error> {
    let client = reqwest::blocking::Client::new();

    // Running input list (messages + returned reasoning items + function_call + function_call_output).
    let mut input: Vec<Value> = vec![json!({ "role": "user", "content": prompt })];

    // Built-in + custom function tools.
    let tools: Vec<Value> = build_tools_with_fs();

    // Helps the model choose tools correctly.
    let instructions = r#"
You can call local filesystem tools to navigate and read files under the app's allowed base directory.
When working with a folder/repo:
1) Start with fs_folder_digest(root) to understand structure.
2) Use fs_grep(root, query, ...) to locate relevant files.
3) Use fs_read_file_chunk(path, ...) to fetch specific sections.
4) Use fs_extract_repo_facts(root) when asked for extracted signals (ports, env vars, urls, docker, rust).
Avoid guessing. Use tools when file content is required.
"#;

    // Agent loop: execute tool calls until the model produces a final assistant message.
    // Keep this small to prevent runaway loops.
    for _step in 0..8 {
        let body = ResponsesRequestBody {
            model: DEFAULT_MODEL.to_string(),
            input: input.clone(),
            tools: tools.clone(),
            instructions: Some(instructions.to_string()),
            store: Some(true),
            parallel_tool_calls: Some(false),
        };
        logger::debug(&format!("Sending API Request: {:#?}", body));
        let res_text = client
            .post(OPENAI_RESPONSES_URL)
            .bearer_auth(api_key)
            .json(&body)
            .timeout(Duration::from_secs(60 * 2))
            .send()
            .map_err(|e| {
                logger::error(&format!("Error sending request {:#?}", e));
                e
            })?
            .text()?;

        logger::debug(&format!("Got AI Response:_{res_text}_"));

        let resp: ResponsesResponse = match serde_json::from_str(&res_text) {
            Ok(r) => r,
            Err(e) => {
                // If parsing fails, return the parse error as the response (keeps behavior similar to old code).
                logger::error(&format!("Parsing failed {:#?}", e));
                return Err(anyhow::Error::new(e));
            }
        };

        // Collect tool calls from this response.
        let mut calls: Vec<FunctionCallItem> = Vec::new();

        for item in &resp.output {
            let item_json = item.to_input_value()?;
            match item.item_type.as_str() {
                // Reasoning models may return reasoning items that must be passed back when tool calls are involved.
                "reasoning" => {
                    input.push(item_json);
                }
                "function_call" => {
                    if let Some(call) = FunctionCallItem::from_output_item(item) {
                        // Pass the function_call item back in.
                        input.push(item_json);
                        calls.push(call);
                    }
                }
                _ => {}
            }
        }

        // If no function calls, return any assistant message text.
        if calls.is_empty() {
            return Ok(extract_output_text(&resp.output));
        }

        // Execute each function call and append function_call_output items.
        for call in calls {
            let output_str = match execute_fs_tool_call(fs, &call.name, &call.arguments_json) {
                Ok(v) => serde_json::to_string(&v)
                    .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string()),
                Err(e) => json!({ "error": e.to_string() }).to_string(),
            };

            input.push(json!({
                "type": "function_call_output",
                "call_id": call.call_id,
                "output": output_str
            }));
        }
    }

    Ok(Some(
        "Tool loop limit reached without a final message.".to_string(),
    ))
}

// ------------------------------ Request/Response ----------------------------

#[derive(Debug, Serialize)]
struct ResponsesRequestBody {
    model: String,
    input: Vec<Value>,
    tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    output: Vec<OutputItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OutputItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<Value>>,
    // Keep the full raw object for re-inserting into input.
    #[serde(flatten)]
    pub raw: Value,
}

impl OutputItem {
    pub fn to_input_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

// Represents a function_call item.
struct FunctionCallItem {
    call_id: String,
    name: String,
    arguments_json: Value,
}

impl FunctionCallItem {
    fn from_output_item(item: &OutputItem) -> Option<Self> {
        let call_id = item.call_id.clone()?;
        let name = item.name.clone()?;
        let args_str = item.arguments.clone()?;

        let arguments_json: Value = serde_json::from_str(&args_str).unwrap_or_else(|_| json!({}));

        Some(Self {
            call_id,
            name,
            arguments_json,
        })
    }
}

// ----------------------------- Tool Definitions -----------------------------

fn build_tools_with_fs() -> Vec<Value> {
    let mut tools: Vec<Value> = vec![json!({ "type": "web_search" })];

    // fs_folder_digest(root)
    tools.push(json!({
        "type": "function",
        "name": "fs_folder_digest",
        "description": "Summarize a folder: tree + stats + curated samples (deterministic).",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {
                "root": { "type": "string", "description": "Folder path (relative to allowed base_dir or absolute under it)." }
            },
            "required": ["root"],
            "additionalProperties": false
        }
    }));

    // fs_read_file_chunk(path, offset, max_bytes)
    tools.push(json!({
        "type": "function",
        "name": "fs_read_file_chunk",
        "description": "Read a UTF-8 text chunk from a file with offset/max_bytes paging.",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path (relative to allowed base_dir or absolute under it)." },
                "offset": { "type": ["integer", "null"], "description": "Byte offset (default 0)." },
                "max_bytes": { "type": ["integer", "null"], "description": "Max bytes to read (default 262144, capped internally)." }
            },
            "required": ["path", "offset", "max_bytes"],
            "additionalProperties": false
        }
    }));

    // fs_grep(root, query, case_sensitive, max_matches)
    tools.push(json!({
        "type": "function",
        "name": "fs_grep",
        "description": "Search for a substring in text files under a folder. Returns (path,line,snippet).",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {
                "root": { "type": "string", "description": "Folder root to search under." },
                "query": { "type": "string", "description": "Substring to search for." },
                "case_sensitive": { "type": ["boolean", "null"], "description": "Case sensitive search (default false)." },
                "max_matches": { "type": ["integer", "null"], "description": "Max matches to return (default 200)." }
            },
            "required": ["root", "query", "case_sensitive", "max_matches"],
            "additionalProperties": false
        }
    }));

    // fs_extract_repo_facts(root)
    tools.push(json!({
        "type": "function",
        "name": "fs_extract_repo_facts",
        "description": "Extract lightweight repo facts (env var tokens, urls, ports, docker presence, rust signals).",
        "strict": true,
        "parameters": {
            "type": "object",
            "properties": {
                "root": { "type": "string", "description": "Folder path (relative to allowed base_dir or absolute under it)." }
            },
            "required": ["root"],
            "additionalProperties": false
        }
    }));

    tools
}

// ------------------------------ Tool Execution ------------------------------

fn execute_fs_tool_call(fs: &FsTools, name: &str, args: &Value) -> Result<Value, anyhow::Error> {
    match name {
        "fs_folder_digest" => {
            let root = args.get("root").and_then(|v| v.as_str()).unwrap_or(".");
            let digest = fs.folder_digest(root)?;
            Ok(serde_json::to_value(digest)?)
        }

        "fs_read_file_chunk" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'path'"))?;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
            let max_bytes = args
                .get("max_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(256 * 1024);

            let text = fs.read_file_chunk(path, offset, max_bytes)?;
            Ok(json!({
                "path": path,
                "offset": offset,
                "max_bytes": max_bytes,
                "text": text
            }))
        }

        "fs_grep" => {
            let root = args.get("root").and_then(|v| v.as_str()).unwrap_or(".");
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let max_matches = args
                .get("max_matches")
                .and_then(|v| v.as_u64())
                .unwrap_or(200) as usize;

            let result = fs.grep(root, query, case_sensitive, max_matches)?;
            Ok(serde_json::to_value(result)?)
        }

        "fs_extract_repo_facts" => {
            let root = args.get("root").and_then(|v| v.as_str()).unwrap_or(".");
            let facts = fs.extract_repo_facts(root)?;
            Ok(serde_json::to_value(facts)?)
        }

        other => Ok(json!({ "error": format!("unknown tool: {other}") })),
    }
}

// ------------------------------- Text Extraction ----------------------------

fn extract_output_text(output: &[OutputItem]) -> Option<String> {
    // Look for assistant message items; concatenate any content parts that include "text".
    let mut parts: Vec<String> = Vec::new();

    for item in output {
        if item.item_type != "message" {
            continue;
        }
        let role = item.role.clone().unwrap_or("".to_string());
        if role != "assistant" {
            continue;
        }

        if let Some(content) = &item.content {
            for c in content {
                if let Some(t) = c.get("text").and_then(|v| v.as_str()) {
                    parts.push(t.to_string());
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(""))
    }
}

// ---------------------------------- Tests ----------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use crate::config;

    // Adjust import paths if needed.
    use super::super::fs_tools::{FsPolicy, FsTools};

    #[test]
    fn test_request_plain() {
        config::init();
        let api_key = config::get_env::<String>("OPENAI_API_KEY");
        let prompt = "Hi";

        let result = get_ai_response(&api_key, prompt);
        match result {
            Err(e) => panic!("Error in sending request {e}"),
            Ok(None) => panic!("Succeeded without response"),
            Ok(Some(_)) => {}
        }
    }

    #[test]
    fn test_request_with_fs_smoke() {
        config::init();
        let _ = logger::init();
        let api_key = config::get_env::<String>("OPENAI_API_KEY");

        // Allow current working directory in tests.
        let policy = FsPolicy::new(".").expect("FsPolicy::new failed");
        let fs = FsTools::new(policy).expect("FsTools::new failed");

        let prompt = "Summarize this folder. Start by calling fs_folder_digest(root=\".\"). Then give a short summary.";
        // let prompt = "Extract all useful information from this current directory '.'. Make sure to keep it data dense.";
        let result = get_ai_response_with_fs(&api_key, prompt, &fs);

        match result {
            Err(e) => panic!("Error in sending request {e}"),
            Ok(None) => panic!("Succeeded without response"),
            Ok(Some(r)) => {
                println!("{}", r);
            }
        }
    }

    #[test]
    fn deserialization_test() {
        let json = r#"{
        "id": "resp_00a9ecd13f92a37001697af90c1050819480068cd123a4c07d",
        "object": "response",
        "created_at": 1769666828,
        "status": "completed",
        "background": false,
        "billing": {
            "payer": "developer"
        },
        "completed_at": 1769666830,
        "error": null,
        "frequency_penalty": 0.0,
        "incomplete_details": null,
        "instructions": "\nYou can call local filesystem tools to navigate and read files under the app's allowed base directory.\nWhen working with a folder/repo:\n1) Start with fs_folder_digest(root) to understand structure.\n2) Use fs_grep(root, query, ...) to locate relevant files.\n3) Use fs_read_file_chunk(path, ...) to fetch specific sections.\n4) Use fs_extract_repo_facts(root) when asked for extracted signals (ports, env vars, urls, docker, rust).\nAvoid guessing. Use tools when file content is required.\n",
        "max_output_tokens": null,
        "max_tool_calls": null,
        "model": "gpt-5-nano-2025-08-07",
        "output": [
            {
            "id": "rs_00a9ecd13f92a37001697af90ca1288194ba69afbfad530b66",
            "type": "reasoning",
            "summary": []
            },
            {
            "id": "fc_00a9ecd13f92a37001697af90eb4b881949e152942d3dff5f3",
            "type": "function_call",
            "status": "completed",
            "arguments": "{\"root\":\".\"}",
            "call_id": "call_6lL6zqZ4zW5536i9cNq3Iwf3",
            "name": "fs_folder_digest"
            }
        ],
        "parallel_tool_calls": false,
        "presence_penalty": 0.0,
        "previous_response_id": null,
        "prompt_cache_key": null,
        "prompt_cache_retention": null,
        "reasoning": {
            "effort": "medium",
            "summary": null
        },
        "safety_identifier": null,
        "service_tier": "default",
        "store": false,
        "temperature": 1.0,
        "text": {
            "format": {
            "type": "text"
            },
            "verbosity": "medium"
        },
        "tool_choice": "auto",
        "tools": [
            {
            "type": "function",
            "description": "Summarize a folder: tree + stats + curated samples (deterministic).",
            "name": "fs_folder_digest",
            "parameters": {
                "additionalProperties": false,
                "properties": {
                "root": {
                    "description": "Folder path (relative to allowed base_dir or absolute under it).",
                    "type": "string"
                }
                },
                "required": [
                "root"
                ],
                "type": "object"
            },
            "strict": true
            },
            {
            "type": "function",
            "description": "Read a UTF-8 text chunk from a file with offset/max_bytes paging.",
            "name": "fs_read_file_chunk",
            "parameters": {
                "additionalProperties": false,
                "properties": {
                "max_bytes": {
                    "description": "Max bytes to read (default 262144, capped internally).",
                    "type": [
                    "integer",
                    "null"
                    ]
                },
                "offset": {
                    "description": "Byte offset (default 0).",
                    "type": [
                    "integer",
                    "null"
                    ]
                },
                "path": {
                    "description": "File path (relative to allowed base_dir or absolute under it).",
                    "type": "string"
                }
                },
                "required": [
                "path",
                "offset",
                "max_bytes"
                ],
                "type": "object"
            },
            "strict": true
            },
            {
            "type": "function",
            "description": "Search for a substring in text files under a folder. Returns (path,line,snippet).",
            "name": "fs_grep",
            "parameters": {
                "additionalProperties": false,
                "properties": {
                "case_sensitive": {
                    "description": "Case sensitive search (default false).",
                    "type": [
                    "boolean",
                    "null"
                    ]
                },
                "max_matches": {
                    "description": "Max matches to return (default 200).",
                    "type": [
                    "integer",
                    "null"
                    ]
                },
                "query": {
                    "description": "Substring to search for.",
                    "type": "string"
                },
                "root": {
                    "description": "Folder root to search under.",
                    "type": "string"
                }
                },
                "required": [
                "root",
                "query",
                "case_sensitive",
                "max_matches"
                ],
                "type": "object"
            },
            "strict": true
            },
            {
            "type": "function",
            "description": "Extract lightweight repo facts (env var tokens, urls, ports, docker presence, rust signals).",
            "name": "fs_extract_repo_facts",
            "parameters": {
                "additionalProperties": false,
                "properties": {
                "root": {
                    "description": "Folder path (relative to allowed base_dir or absolute under it).",
                    "type": "string"
                }
                },
                "required": [
                "root"
                ],
                "type": "object"
            },
            "strict": true
            },
            {
            "type": "web_search",
            "filters": null,
            "search_context_size": "medium",
            "user_location": {
                "type": "approximate",
                "city": null,
                "country": "US",
                "region": null,
                "timezone": null
            }
            }
        ],
        "top_logprobs": 0,
        "top_p": 1.0,
        "truncation": "disabled",
        "usage": {
            "input_tokens": 5021,
            "input_tokens_details": {
            "cached_tokens": 0
            },
            "output_tokens": 304,
            "output_tokens_details": {
            "reasoning_tokens": 256
            },
            "total_tokens": 5325
        },
        "user": null,
        "metadata": {}
        }"#;
        let parsed: ResponsesResponse = match serde_json::from_str(json) {
            Ok(j) => j,
            Err(e) => panic!("{}" ,e),
        };

        println!("{:#?}",parsed);
    }
}
