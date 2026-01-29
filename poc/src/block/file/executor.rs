use crate::{block::{ExecutionResult, ExecutionRunResult, executor_error::ExecutorError}, logger};
use super::{FileBlockBody, FileOperationType};

pub fn execute_file(
    input: Option<String>,
    body: FileBlockBody,
) -> ExecutionRunResult {
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
                    Ok(Some(ExecutionResult::Response(Some(content))))
                }
            }
        },
        FileOperationType::WATCH => {
            Err(ExecutorError::NotImplemented("File Watch not implemented".to_owned()))
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
            Ok(Some(ExecutionResult::Response(Some(content)))) => {
                assert!(content.contains("/target"));
            }
            Ok(None) => {
                panic!("No content to read");
            }
            Err(e) => {
                panic!("error {e}");
            },
            _ => panic!("unexpected response")
        }
    }
}
