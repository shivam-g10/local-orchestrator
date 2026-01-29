use super::logger;
use std::fmt::Display;
use thiserror::Error;
#[derive(Debug, Error)]
pub struct CommonError {
    message: String,
}

impl CommonError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}
impl Display for CommonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<String> for CommonError {
    fn from(msg: String) -> Self {
        logger::error(&msg);
        Self::new(&msg)
    }
}

impl From<&str> for CommonError {
    fn from(msg: &str) -> Self {
        logger::error(msg);
        Self::new(msg)
    }
}
