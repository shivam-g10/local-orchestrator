//! SendEmail block: Action that sends email using an injected mailer.
//! Input may be JSON with `to`/`email`, `name`, `subject`, and `body`, or a plain string as body (config supplies default `to` and `subject`).
//!
//! The mailer API matches the poc: `send_email(subject, to_name, to_email, body)`.
//! Pass your mailer when registering: `register_send_email(registry, Arc::new(your_mailer))`.
//! `default_registry()` registers `send_email` with [`EnvSmtpMailer`], which reads SMTP
//! settings from env only when the block executes.
//! Use `registry_with_mailer(mailer)` to override with your own implementation.

mod lettre_env;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

pub use lettre_env::EnvSmtpMailer;

/// Error from sending email.
#[derive(Debug, Clone)]
pub struct SendEmailError(pub String);

impl std::fmt::Display for SendEmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SendEmailError {}

/// Mailer abstraction: same API as poc `Mailer::send_email(subject, to_name, to_email, html_template)`.
/// Implement this and pass it when registering the send_email block.
pub trait SendEmail: Send + Sync {
    /// Send an email. `to_name` is the recipient display name, `to_email` the address, `body` the content (e.g. HTML).
    fn send_email(
        &self,
        subject: &str,
        to_name: &str,
        to_email: &str,
        body: String,
    ) -> Result<(), SendEmailError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendEmailConfig {
    pub to: String,
    pub subject: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
}

impl SendEmailConfig {
    pub fn new(to: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            subject: None,
            smtp_host: None,
            smtp_port: None,
        }
    }
}

pub struct SendEmailBlock {
    config: SendEmailConfig,
    mailer: Arc<dyn SendEmail>,
}

impl SendEmailBlock {
    pub fn new(config: SendEmailConfig, mailer: Arc<dyn SendEmail>) -> Self {
        Self { config, mailer }
    }
}

fn parse_input(
    input: &BlockInput,
    default_to: &str,
    default_subject: &str,
) -> Result<(String, String, String, String), BlockError> {
    match input {
        BlockInput::Json(v) => {
            let to_email = v
                .get("to")
                .or_else(|| v.get("email"))
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| default_to.to_string());
            let to_name = v
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_default();
            let subject = v
                .get("subject")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| default_subject.to_string());
            let body = v
                .get("body")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| v.to_string());
            Ok((to_email, to_name, subject, body))
        }
        BlockInput::String(s) => Ok((
            default_to.to_string(),
            String::new(),
            default_subject.to_string(),
            s.clone(),
        )),
        BlockInput::Text(s) => Ok((
            default_to.to_string(),
            String::new(),
            default_subject.to_string(),
            s.clone(),
        )),
        BlockInput::Empty => Ok((
            default_to.to_string(),
            String::new(),
            default_subject.to_string(),
            String::new(),
        )),
        BlockInput::List { items } => Ok((
            default_to.to_string(),
            String::new(),
            default_subject.to_string(),
            items.join("\n"),
        )),
        BlockInput::Multi { outputs } => {
            let body = outputs
                .iter()
                .filter_map(|o| Option::<String>::from(o.clone()))
                .collect::<Vec<_>>()
                .join("\n");
            Ok((
                default_to.to_string(),
                String::new(),
                default_subject.to_string(),
                body,
            ))
        }
        BlockInput::Error { .. } => unreachable!(),
    }
}

impl BlockExecutor for SendEmailBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }
        let default_subject = self.config.subject.as_deref().unwrap_or("");
        let (to_email, to_name, subject, body) =
            parse_input(&input, &self.config.to, default_subject)?;
        self.mailer
            .send_email(&subject, &to_name, &to_email, body)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::Json {
            value: serde_json::json!({ "sent": true, "to": to_email }),
        }))
    }
}

/// Register the send_email block with a mailer. The user passes their mailer when building the registry.
pub fn register_send_email(
    registry: &mut orchestrator_core::block::BlockRegistry,
    mailer: Arc<dyn SendEmail>,
) {
    let mailer = Arc::clone(&mailer);
    registry.register_custom("send_email", move |payload| {
        let config: SendEmailConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(SendEmailBlock::new(config, Arc::clone(&mailer))))
    });
}

/// Register send_email with the built-in env-based SMTP mailer.
pub fn register_send_email_env(registry: &mut orchestrator_core::block::BlockRegistry) {
    register_send_email(registry, Arc::new(EnvSmtpMailer));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No-op mailer for tests only.
    struct NoOpSendEmail;

    impl SendEmail for NoOpSendEmail {
        fn send_email(
            &self,
            _subject: &str,
            _to_name: &str,
            _to_email: &str,
            _body: String,
        ) -> Result<(), SendEmailError> {
            Ok(())
        }
    }

    #[test]
    fn send_email_executes_and_returns_sent_json() {
        let config = SendEmailConfig::new("user@example.com");
        let block = SendEmailBlock::new(config, Arc::new(NoOpSendEmail));
        let input = BlockInput::String("Hello body".into());
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                assert_eq!(value.get("sent"), Some(&serde_json::json!(true)));
                assert_eq!(
                    value.get("to"),
                    Some(&serde_json::json!("user@example.com"))
                );
            }
            _ => panic!("expected Once(Json)"),
        }
    }

    #[test]
    fn send_email_json_input_to_name_subject_body() {
        let mut config = SendEmailConfig::new("default@example.com");
        config.subject = Some("Default subject".into());
        let block = SendEmailBlock::new(config, Arc::new(NoOpSendEmail));
        let input = BlockInput::Json(serde_json::json!({
            "to": "recipient@example.com",
            "name": "Alice",
            "subject": "Hello",
            "body": "Email body text"
        }));
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                assert_eq!(value.get("sent"), Some(&serde_json::json!(true)));
                assert_eq!(
                    value.get("to"),
                    Some(&serde_json::json!("recipient@example.com"))
                );
            }
            _ => panic!("expected Once(Json)"),
        }
    }

    #[test]
    fn send_email_error_input_returns_error() {
        let config = SendEmailConfig::new("user@example.com");
        let block = SendEmailBlock::new(config, Arc::new(NoOpSendEmail));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
