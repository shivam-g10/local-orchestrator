use lettre::address::AddressError;

use super::EmailBlockBody;
use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

pub fn create_email_block(email: &str, name: &str, subject: &str) -> Result<Block, AddressError> {
    let body_result = EmailBlockBody::new(email, name, subject);
    let body = match body_result {
        Err(err) => {
            return Err(err);
        }
        Ok(body) => body,
    };
    let mut block = Block::new(BlockType::EMAIL, BlockExecutionType::Response);
    block.set_block_body(BlockBody::EMAIL(body));
    Ok(block)
}
