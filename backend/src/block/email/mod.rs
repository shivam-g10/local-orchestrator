mod executor;
mod mailer;
pub mod utils;

use std::str::FromStr;

pub use executor::execute_email;
use lettre::{Address, message::Mailbox};

#[derive(Debug, Clone)]
pub struct EmailBlockBody {
    pub email: String,
    pub name: String,
    pub subject: String,
    pub mailbox: Mailbox,
}

impl EmailBlockBody {
    pub fn new(
        email: &str,
        name: &str,
        subject: &str,
    ) -> Result<Self, lettre::address::AddressError> {
        let address = Address::from_str(email)?;
        let mailbox = Mailbox::new(Some(name.to_string()), address);
        Ok(Self {
            email: email.to_string(),
            name: name.to_string(),
            mailbox,
            subject: subject.to_string(),
        })
    }
}

#[cfg(test)]
mod test {

    use lettre::Address;

    use crate::{
        block::{Block, BlockBody, BlockType},
        config,
    };

    use super::*;

    #[test]
    fn create_new_email_block() {
        config::init();
        let mut block = Block::new(BlockType::EMAIL);
        let test_email = "test@test.com";
        let test_name = "Test Email";
        let mailbox = Mailbox::new(
            Some(test_name.to_string()),
            Address::from_str(test_email).unwrap(),
        );
        let result = EmailBlockBody::new(test_email, test_name, "Test Subject");
        let body: EmailBlockBody = result.unwrap();
        block.set_block_body(BlockBody::EMAIL(body));
        assert_eq!(block.block_type, BlockType::EMAIL);
        assert!(block.block_body.is_some());
        if let Some(BlockBody::EMAIL(block_body)) = block.block_body {
            assert_eq!(block_body.mailbox, mailbox);
        } else {
            panic!("No block available in create new CRON");
        };

        assert!(!block.id.is_nil());
    }

    #[test]
    fn email_error_test() {
        let test_email = "testtest.com";
        let test_name = "Test Email";
        let result = EmailBlockBody::new(test_email, test_name, "Test Error Subject");
        assert!(result.is_err())
    }
}
