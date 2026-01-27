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
    #[error("Email input missing")]
    EmailInputNotFound,
    #[error("Error connecting to email SMTP")]
    EmailConnectionError,
    #[error("Error sending email: {0}")]
    EmailSendError(Box<dyn std::error::Error>),
}

