use std::str::FromStr;

use lettre::{
    Address, Message, SmtpTransport, Transport,
    message::{Mailbox, header::ContentType},
    transport::smtp::authentication::Credentials,
};

use super::{SendEmail, SendEmailError};

/// Built-in SMTP mailer for `default_registry()`.
///
/// Env contract (read at block execution time):
/// - host: `SMTP_HOST` (fallback `SMTP`)
/// - port: `SMTP_PORT` (default `587`)
/// - user: `SMTP_USERNAME` (fallback `SMTP_UNAME`) optional
/// - pass: `SMTP_PASSWORD` (fallback `SMTP_PASS`) optional
/// - secure: `SMTP_SECURE` optional (`true/false`, default `true`)
/// - sender email: `EMAIL_FROM` (fallback `DEFAULT_SENDER`) required
/// - sender name: `EMAIL_FROM_NAME` (fallback `DEFAULT_SENDER_NAME`) optional
#[derive(Default)]
pub struct EnvSmtpMailer;

#[derive(Debug, Clone)]
struct EnvSmtpConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    secure: bool,
    from_email: String,
    from_name: Option<String>,
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        std::env::var(k)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

impl EnvSmtpConfig {
    fn from_env() -> Result<Self, SendEmailError> {
        let host = env_first(&["SMTP_HOST", "SMTP"]).ok_or_else(|| {
            SendEmailError("missing SMTP host env var (SMTP_HOST or SMTP)".into())
        })?;
        let port = env_first(&["SMTP_PORT"])
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(587);
        let username = env_first(&["SMTP_USERNAME", "SMTP_UNAME"]);
        let password = env_first(&["SMTP_PASSWORD", "SMTP_PASS"]);
        if username.is_some() ^ password.is_some() {
            return Err(SendEmailError(
                "set both SMTP_USERNAME/SMTP_UNAME and SMTP_PASSWORD/SMTP_PASS".into(),
            ));
        }
        let secure = env_first(&["SMTP_SECURE"])
            .as_deref()
            .and_then(parse_bool)
            .unwrap_or(true);
        let from_email = env_first(&["EMAIL_FROM", "DEFAULT_SENDER"]).ok_or_else(|| {
            SendEmailError("missing sender env var (EMAIL_FROM or DEFAULT_SENDER)".into())
        })?;
        let from_name = env_first(&["EMAIL_FROM_NAME", "DEFAULT_SENDER_NAME"]);
        Ok(Self {
            host,
            port,
            username,
            password,
            secure,
            from_email,
            from_name,
        })
    }
}

impl SendEmail for EnvSmtpMailer {
    fn send_email(
        &self,
        subject: &str,
        to_name: &str,
        to_email: &str,
        body: String,
    ) -> Result<(), SendEmailError> {
        let cfg = EnvSmtpConfig::from_env()?;

        let from_address = Address::from_str(&cfg.from_email)
            .map_err(|e| SendEmailError(format!("invalid sender email: {}", e)))?;
        let from_mailbox = Mailbox::new(cfg.from_name.clone(), from_address);
        let to_address = Address::from_str(to_email)
            .map_err(|e| SendEmailError(format!("invalid recipient email: {}", e)))?;
        let to_mailbox = Mailbox::new(
            if to_name.trim().is_empty() {
                None
            } else {
                Some(to_name.to_string())
            },
            to_address,
        );

        let email = Message::builder()
            .to(to_mailbox)
            .reply_to(from_mailbox.clone())
            .from(from_mailbox)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body)
            .map_err(|e| SendEmailError(e.to_string()))?;

        let mut builder = if cfg.secure {
            SmtpTransport::relay(&cfg.host).map_err(|e| SendEmailError(e.to_string()))?
        } else {
            SmtpTransport::builder_dangerous(&cfg.host)
        };
        builder = builder.port(cfg.port);
        if let (Some(username), Some(password)) = (cfg.username, cfg.password) {
            builder = builder.credentials(Credentials::new(username, password));
        }
        builder
            .build()
            .send(&email)
            .map_err(|e| SendEmailError(e.to_string()))?;
        Ok(())
    }
}
