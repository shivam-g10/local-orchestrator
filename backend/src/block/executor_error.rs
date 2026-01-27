use thiserror::Error;

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

