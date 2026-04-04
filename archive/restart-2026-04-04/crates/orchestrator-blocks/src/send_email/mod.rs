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
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::input_binding::{
    resolve_effective_input, validate_expected_input, validate_single_input_mode,
};
use orchestrator_core::RetryPolicy;
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind, ValueKindSet,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendEmailConfig {
    #[serde(default)]
    pub to: Option<String>,
    pub subject: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_retry_policy")]
    pub retry_policy: RetryPolicy,
}

fn default_timeout_ms() -> Option<u64> {
    Some(30_000)
}

fn default_retry_policy() -> RetryPolicy {
    RetryPolicy::exponential(3, 1_000, 2.0)
}

impl SendEmailConfig {
    pub fn new(to: impl Into<String>) -> Self {
        Self {
            to: Some(to.into()),
            subject: None,
            smtp_host: None,
            smtp_port: None,
            timeout_ms: default_timeout_ms(),
            retry_policy: default_retry_policy(),
        }
    }
}

pub struct SendEmailBlock {
    config: SendEmailConfig,
    mailer: Arc<dyn SendEmail>,
    input_from: Box<[uuid::Uuid]>,
}

impl SendEmailBlock {
    pub fn new(config: SendEmailConfig, mailer: Arc<dyn SendEmail>) -> Self {
        Self {
            config,
            mailer,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
    }
}

fn block_input_kind(input: &BlockInput) -> &'static str {
    match input {
        BlockInput::Empty => "empty",
        BlockInput::String(_) => "string",
        BlockInput::Text(_) => "text",
        BlockInput::Json(_) => "json",
        BlockInput::List { .. } => "list",
        BlockInput::Multi { .. } => "multi",
        BlockInput::Error { .. } => "error",
    }
}

fn email_domain(email: &str) -> Option<&str> {
    email
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim())
        .filter(|domain| !domain.is_empty())
}

fn parse_input(
    input: &BlockInput,
    default_to: Option<&str>,
    force_default_to: bool,
    default_subject: &str,
) -> Result<(String, String, String, String), BlockError> {
    match input {
        BlockInput::Json(v) => {
            let from_input = v
                .get("to")
                .or_else(|| v.get("email"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let to_email = if force_default_to {
                default_to.map(String::from)
            } else {
                from_input.or_else(|| default_to.map(String::from))
            }
            .ok_or_else(|| BlockError::Other("send_email recipient is required".into()))?;
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
            default_to
                .map(String::from)
                .ok_or_else(|| BlockError::Other("send_email recipient is required".into()))?,
            String::new(),
            default_subject.to_string(),
            s.clone(),
        )),
        BlockInput::Text(s) => Ok((
            default_to
                .map(String::from)
                .ok_or_else(|| BlockError::Other("send_email recipient is required".into()))?,
            String::new(),
            default_subject.to_string(),
            s.clone(),
        )),
        BlockInput::Empty => Ok((
            default_to
                .map(String::from)
                .ok_or_else(|| BlockError::Other("send_email recipient is required".into()))?,
            String::new(),
            default_subject.to_string(),
            String::new(),
        )),
        BlockInput::List { items } => Ok((
            default_to
                .map(String::from)
                .ok_or_else(|| BlockError::Other("send_email recipient is required".into()))?,
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
                default_to
                    .map(String::from)
                    .ok_or_else(|| BlockError::Other("send_email recipient is required".into()))?,
                String::new(),
                default_subject.to_string(),
                body,
            ))
        }
        BlockInput::Error { .. } => unreachable!(),
    }
}

impl BlockExecutor for SendEmailBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }
        let default_subject = self.config.subject.as_deref().unwrap_or("");
        let force_default_to = self.input_from.is_empty() && self.config.to.is_some();
        let default_to = if self.input_from.is_empty() {
            self.config.to.as_deref()
        } else {
            None
        };
        let (to_email, to_name, subject, body) =
            parse_input(&input, default_to, force_default_to, default_subject)?;
        debug!(
            event = "email.send_configured",
            domain = "email",
            block_type = "send_email",
            input_kind = block_input_kind(&input),
            to_domain = email_domain(&to_email).unwrap_or("unknown"),
            subject_len = subject.len() as u64,
            body_len = body.len() as u64,
            timeout_ms = self.config.timeout_ms.unwrap_or(30_000),
            max_retries = self.config.retry_policy.max_retries
        );
        let mut retries_done = 0u32;
        loop {
            let attempt = retries_done + 1;
            debug!(
                event = "email.send_attempt",
                domain = "email",
                block_type = "send_email",
                attempt = attempt,
                to_domain = email_domain(&to_email).unwrap_or("unknown"),
                subject_len = subject.len() as u64
            );
            match send_once_with_timeout(
                Arc::clone(&self.mailer),
                self.config.timeout_ms,
                subject.clone(),
                to_name.clone(),
                to_email.clone(),
                body.clone(),
            ) {
                Ok(()) => {
                    debug!(
                        event = "email.send_succeeded",
                        domain = "email",
                        block_type = "send_email",
                        attempt = attempt,
                        to_domain = email_domain(&to_email).unwrap_or("unknown")
                    );
                    break;
                }
                Err(err) => {
                    let (code, retryable) = classify_email_error(&err.0);
                    let can_retry = retryable && self.config.retry_policy.can_retry(retries_done);
                    debug!(
                        event = "email.send_failed",
                        domain = "email",
                        block_type = "send_email",
                        code = code,
                        attempt = attempt,
                        retryable = retryable,
                        can_retry = can_retry,
                        error = %err,
                        error_len = err.0.len() as u64
                    );
                    if can_retry {
                        let backoff = self.config.retry_policy.backoff_duration(retries_done);
                        info!(
                            event = "block.retry_scheduled",
                            domain = "email",
                            block_type = "send_email",
                            code = code,
                            attempt = retries_done + 1,
                            next_attempt = retries_done + 2,
                            backoff_ms = backoff.as_millis() as u64
                        );
                        std::thread::sleep(backoff);
                        retries_done += 1;
                        continue;
                    }
                    debug!(
                        event = "email.send_retry_exhausted",
                        domain = "email",
                        block_type = "send_email",
                        code = code,
                        attempt = attempt
                    );
                    return Err(BlockError::Other(error_payload_json(
                        "email",
                        code,
                        &err.0,
                        retries_done + 1,
                    )));
                }
            }
        }
        Ok(BlockExecutionResult::Once(BlockOutput::Json {
            value: serde_json::json!({ "sent": true, "to": to_email }),
        }))
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::from_kind(ValueKind::Json, OutputMode::Once)
    }

    fn validate_linkage(&self, ctx: &ValidateContext<'_>) -> Result<(), BlockError> {
        if !self.input_from.is_empty() {
            validate_single_input_mode(ctx)?;
            return validate_expected_input(ctx, ValueKindSet::singleton(ValueKind::Json));
        }
        if self.config.to.is_some() {
            return Ok(());
        }
        validate_single_input_mode(ctx)?;
        validate_expected_input(ctx, ValueKindSet::singleton(ValueKind::Json))
    }
}

fn send_once_with_timeout(
    mailer: Arc<dyn SendEmail>,
    timeout_ms: Option<u64>,
    subject: String,
    to_name: String,
    to_email: String,
    body: String,
) -> Result<(), SendEmailError> {
    match timeout_ms {
        None => mailer.send_email(&subject, &to_name, &to_email, body),
        Some(ms) => {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let result = mailer.send_email(&subject, &to_name, &to_email, body);
                let _ = tx.send(result);
            });
            match rx.recv_timeout(Duration::from_millis(ms.max(1))) {
                Ok(result) => result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    Err(SendEmailError(format!("send_email timeout after {}ms", ms)))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    Err(SendEmailError("send_email worker disconnected".into()))
                }
            }
        }
    }
}

fn classify_email_error(message: &str) -> (&'static str, bool) {
    let lower = message.to_ascii_lowercase();
    if lower.contains("auth")
        || lower.contains("invalid sender")
        || lower.contains("invalid recipient")
    {
        return ("email.smtp.auth_failed", false);
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return ("email.smtp.timeout", true);
    }
    if lower.contains("transient")
        || lower.contains("temporary")
        || lower.contains("server unavailable")
        || lower.contains("421")
        || lower.contains("450")
        || lower.contains("451")
        || lower.contains("452")
    {
        return ("email.smtp.transient", true);
    }
    ("email.smtp.permanent", false)
}

fn error_payload_json(domain: &str, code: &str, message: &str, attempt: u32) -> String {
    serde_json::json!({
        "origin": "block",
        "domain": domain,
        "code": code,
        "message": message,
        "provider_status": serde_json::Value::Null,
        "attempt": attempt,
        "retry_disposition": "never",
        "severity": "error"
    })
    .to_string()
}

/// Register the send_email block with a mailer. The user passes their mailer when building the registry.
pub fn register_send_email(
    registry: &mut orchestrator_core::block::BlockRegistry,
    mailer: Arc<dyn SendEmail>,
) {
    let mailer = Arc::clone(&mailer);
    registry.register_custom("send_email", move |payload, input_from| {
        let config: SendEmailConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            SendEmailBlock::new(config, Arc::clone(&mailer)).with_input_from(input_from),
        ))
    });
}

#[cfg(test)]
fn test_ctx(input: BlockInput) -> BlockExecutionContext {
    BlockExecutionContext {
        workflow_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
        block_id: uuid::Uuid::new_v4(),
        attempt: 1,
        prev: input,
        store: Default::default(),
    }
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
        let result = block.execute(test_ctx(input)).unwrap();
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
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                assert_eq!(value.get("sent"), Some(&serde_json::json!(true)));
                assert_eq!(
                    value.get("to"),
                    Some(&serde_json::json!("default@example.com"))
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
        let err = block.execute(test_ctx(input));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }

    #[test]
    fn send_email_precedence_forced_over_config() {
        let source_id = uuid::Uuid::new_v4();
        let ctx = test_ctx(BlockInput::empty());
        ctx.store.insert(
            source_id,
            orchestrator_core::block::StoredOutput::Once(Arc::new(BlockOutput::Json {
                value: serde_json::json!({
                    "to": "forced@example.com",
                    "subject": "From forced",
                    "body": "forced body"
                }),
            })),
        );

        let mut config = SendEmailConfig::new("config@example.com");
        config.subject = Some("From config".into());
        let block = SendEmailBlock::new(config, Arc::new(NoOpSendEmail))
            .with_input_from(vec![source_id].into_boxed_slice());
        let result = block.execute(ctx).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                assert_eq!(value.get("sent"), Some(&serde_json::json!(true)));
                assert_eq!(
                    value.get("to"),
                    Some(&serde_json::json!("forced@example.com"))
                );
            }
            _ => panic!("expected Once(Json)"),
        }
    }
}
