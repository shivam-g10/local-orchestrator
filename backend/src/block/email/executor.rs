use crate::{block::executor_error::ExecutorError, logger};

use super::EmailBlockBody;

pub fn execute_email(input: Option<String>, body: EmailBlockBody) -> Result<Option<String>, ExecutorError> {
    let email_content = match input {
        None => return Err(ExecutorError::EmailInputNotFound),
        Some(input) => input,
    };

    let mailer = super::mailer::Mailer::new();
    if !mailer.check_connection() {
        return Err(ExecutorError::EmailConnectionError)
    }

    match mailer.send_email(&body.subject, &body.name, &body.email, email_content) {
        Err(e) => Err(ExecutorError::EmailSendError(e)),
        Ok(_) => {
            logger::debug("Email Sent successfully");
            Ok(None)
        }
    }
}
