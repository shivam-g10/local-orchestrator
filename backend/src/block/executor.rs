use thiserror::Error;

use crate::{
    block::{AIBlockBody, CronBlockBody, FileBlockBody, FileOperationType, open_ai::get_ai_response},
    logger,
};

#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("{0} is not implemented yet")]
    NotImplemented(String),
    #[error("{0}, {1} file error")]
    FileError(String, std::io::Error),
    #[error("{0} file location not found")]
    FileLocationNotFound(String),
    #[error("{0} file doesn't exist")]
    FileNotExist(String),
    #[error("AI API Error {0}")]
    AiApiError(reqwest::Error),
}

pub fn execute_cron(_: Option<String>, _: CronBlockBody) -> Result<Option<String>, ExecutorError> {
    Ok(None)
}

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

pub fn execute_file(
    input: Option<String>,
    body: FileBlockBody,
) -> Result<Option<String>, ExecutorError> {
    match std::fs::exists(&body.location) {
        Err(e) => {
            return Err(ExecutorError::FileError(body.location, e));
        }
        Ok(exists) => {
            if !exists {
                return Err(ExecutorError::FileLocationNotFound(body.location));
            }
        }
    }

    match &body.operation {
        FileOperationType::WRITE => {
            let path = body.location + "/" + &body.file_name;
            let contents = input.unwrap_or_default();
            match std::fs::write(&path, contents) {
                Err(e) => {
                    logger::error(&format!("Error writing to file {e}"));
                    Err(ExecutorError::FileError(path, e))
                }
                Ok(_) => {
                    logger::info("File write completed successfully");
                    Ok(None)
                }
            }
        }
        FileOperationType::READ => {
            let path = body.location + "/" + &body.file_name;
            match std::fs::read_to_string(&path) {
                Err(e) => {
                    Err(ExecutorError::FileError(path, e))
                }
                Ok(content) => {
                    logger::info("File read successful");
                    Ok(Some(content))
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::env;
    #[test]
    fn write_file() {
        let current_dir = match env::current_dir() {
            Ok(dir) => dir.to_str().unwrap().to_owned(),
            Err(e) => {
                panic!("Error getting cwd {e}");
            }
        };
        let body: FileBlockBody = FileBlockBody {
            file_name: "test.md".to_string(),
            location: current_dir,
            operation: FileOperationType::WRITE,
        };
        match execute_file(Some("test".to_string()), body.clone()) {
            Err(e) => {
                panic!("Error writing to file {e}");
            }
            Ok(_) => {
                let path = body.location + "/" + &body.file_name;
                let _ = std::fs::remove_file(path);
            }
        }
    }

    #[test]
    fn read_file() {
        let current_dir = match env::current_dir() {
            Ok(dir) => dir.to_str().unwrap().to_owned(),
            Err(e) => {
                panic!("Error getting cwd {e}");
            }
        };
        let body: FileBlockBody = FileBlockBody {
            file_name: ".gitignore".to_string(),
            location: current_dir,
            operation: FileOperationType::READ,
        };
        match execute_file(None, body.clone()) {
            Ok(Some(content)) => {
                assert!(content.contains("/target"));
            }
            Ok(None) => {
                panic!("No content to read");
            }
            Err(e) => {
                panic!("error {e}");
            }
        }
    }
}
