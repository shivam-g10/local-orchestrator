use std::str::FromStr;

use lettre::{
    Address, Message, SmtpTransport, Transport,
    message::{Mailbox, header::ContentType},
    transport::smtp::PoolConfig,
};

use crate::logger;

use crate::config;

#[derive(Clone)]
pub struct Mailer {
    mailer: SmtpTransport,
    default_sender: Mailbox,
}

impl Mailer {
    pub fn new() -> Self {
        let smtp_server = config::get_env::<String>("SMTP");
        let smtp_port = config::get_env("SMTP_PORT");
        let smtp_user_name = config::get_env::<String>("SMTP_UNAME");
        let smtp_user_pass = config::get_env::<String>("SMTP_PASS");
        let default_sender = config::get_env::<String>("DEFAULT_SENDER");
        let default_sender_name = config::get_env("DEFAULT_SENDER_NAME");

        let mut split = default_sender.split("@");
        let user = split.next().unwrap();
        let domain = split.next().unwrap();
        logger::debug(&format!(
            "Creating mailer with SMTP server: {}, port: {}, user: {}, domain: {}",
            smtp_server, smtp_port, user, domain
        ));
        let sender = Address::new(user, domain).unwrap();
        let pool_config = PoolConfig::new().min_idle(1);

        let transport = if config::get_env::<String>("DEPLOY_ENV") == "production" {
            logger::debug("Using SMTPS transport");
            let smtps_url = format!(
                "smtps://{}:{}@{}:{}",
                &smtp_user_name, &smtp_user_pass, &smtp_server, &smtp_port
            );
            let result = SmtpTransport::from_url(&smtps_url);
            if result.is_err() {
                logger::error(&format!(
                    "Failed to create SMTP transport from URL: {}",
                    result.as_ref().err().unwrap()
                ));
            } else {
                logger::debug("SMTP transport created successfully");
            }
            result
                .expect("Failed to create SMTP transport")
                .pool_config(pool_config)
                .build()
        } else {
            SmtpTransport::builder_dangerous(&smtp_server)
                .pool_config(pool_config)
                .port(smtp_port)
                .build()
        };

        Self {
            mailer: transport,
            default_sender: Mailbox::new(Some(default_sender_name), sender),
        }
    }

    pub fn send_email(
        &self,
        subject: &str,
        to_name: &str,
        to_email: &str,
        html_template: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let to = Address::from_str(to_email)?;
        let email = Message::builder()
            .to(Mailbox::new(Some(to_name.to_owned()), to))
            .reply_to(self.default_sender.clone())
            .from(self.default_sender.clone())
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html_template.clone())?;
        logger::debug("Before mail send");
        let _r = self.mailer.send(&email);
        logger::debug("After mail send");
        Ok(())
    }

    pub fn check_connection(&self) -> bool {
        match self.mailer.test_connection() {
            Ok(b) => b,
            Err(e) => {
                logger::error(&format!("Failed to connect to SMTP server: {}", e));
                false
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_send_email() {
        config::init();
        let mailer = Mailer::new();
        assert!(mailer.check_connection(), "Error connecting to smtp");
        let result = mailer.send_email(
            "test",
            "test name",
            "test@test.com",
            "This is a test email".to_string(),
        );
        assert!(result.is_ok(), "Error in sending email {:#?}", result.err())
    }
}
