use crate::block::executor_error::ExecutorError;
use crate::block::{ExecutionResult, ExecutionRunResult};

use super::AIBlockBody;
use super::open_ai::{get_ai_response, get_ai_response_with_fs};

pub fn execute_ai(input: Option<String>, body: AIBlockBody) -> ExecutionRunResult {
    let replace_input = input.unwrap_or(String::from(""));
    let final_prompt = body.prompt.replace("###INPUT", &replace_input);
    if let Some(fs_tools) = body.fs_tools {
        match get_ai_response_with_fs(&body.api_key, &final_prompt, &fs_tools) {
            Err(e) => Err(ExecutorError::AiFsError(e)),
            Ok(result) => Ok(Some(ExecutionResult::Response(result))),
        }
    } else {
        match get_ai_response(&body.api_key, &final_prompt) {
            Err(e) => Err(ExecutorError::AiApiError(e)),
            Ok(result) => Ok(Some(ExecutionResult::Response(result))),
        }
    }
}
