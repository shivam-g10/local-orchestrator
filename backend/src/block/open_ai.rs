use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::logger;

pub fn get_ai_response(api_key: &str, prompt: &str) -> Result<Option<String>, reqwest::Error> {
    let client = reqwest::blocking::Client::new();
    let result = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&get_request_body(prompt))
        .timeout(Duration::from_secs(60 * 2)) // 2 min timeout
        .build();
    let request = match result {
        Ok(r) => r,
        Err(e) => {
            return Err(e);
        }
    };
    match client.execute(request) {
        Err(e) => {
            logger::error(&format!("Error sending request {:#?}", e));
            Err(e)
        }
        Ok(res) => {
            let result = match res.text() {
                Err(e) => {
                    return Err(e);
                }
                Ok(t) => {
                    logger::debug(&format!("Got AI Response: {t}"));
                    t
                }
            };
            match serde_json::from_str::<OpenAIResponse>(&result) {
                Err(e) => Ok(Some(e.to_string())),
                Ok(res) => {
                    let content = res.output.iter().find(|r| r.content.is_some());
                    if let Some(output) = content {
                        match &output.content {
                            Some(c) => match c.first() {
                                Some(nested_content) => {
                                    return Ok(Some(nested_content.text.clone()));
                                }
                                None => {
                                    return Ok(None);
                                }
                            },
                            None => {
                                return Ok(None);
                            }
                        }
                    }
                    Ok(None)
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Body {
    model: String,
    input: String,
    tools: Vec<OpenAITools>,
}

#[derive(Serialize, Deserialize)]
struct OpenAITools {
    #[serde(rename = "type")]
    tool_type: String,
}

#[derive(Serialize, Deserialize)]
struct OpenAIResponse {
    pub output: Vec<OpenAIOutput>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIOutput {
    pub id: String,
    #[serde(rename = "type")]
    pub output_type: String,
    pub content: Option<Vec<OpenAIContent>>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIContent {
    pub text: String,
}

fn get_request_body(prompt: &str) -> Body {
    Body {
        model: "gpt-5-nano".to_string(),
        input: prompt.to_string(),
        tools: vec![OpenAITools {
            tool_type: "web_search".to_string(),
        }],
    }
}

#[cfg(test)]
mod test {
    use crate::config;

    use super::*;

    #[test]
    fn test_request() {
        config::init();
        let api_key = config::get_env::<String>("OPENAI_API_KEY");
        let prompt = "Hi";
        let result = get_ai_response(&api_key, prompt);
        match result {
            Err(e) => {
               panic!("Error in sending request {e}");
            }
            Ok(None) => {
                panic!("Succeeded without response");
            }

            Ok(Some(_)) => {}
        }
    }
}
