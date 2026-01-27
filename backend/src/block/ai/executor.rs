use crate::block::executor_error::ExecutorError;

use super::AIBlockBody;
use super::open_ai::get_ai_response;


pub fn execute_ai(
    input: Option<String>,
    body: AIBlockBody,
) -> Result<Option<String>, ExecutorError> {
    let replace_input = input.unwrap_or(String::from(""));
    let final_prompt = body.prompt.replace("###INPUT", &replace_input);
    match get_ai_response(&body.api_key, &final_prompt) {
        Err(e) => {
            Err(ExecutorError::AiApiError(e))
        }
        Ok(result) => Ok(result),
    }
}