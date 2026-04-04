use std::error::Error;

use super::DelayBlockBody;
use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

pub fn create_delay_block(
    delay_ms: u64,
    forward_message: Option<bool>,
) -> Result<Block, Box<dyn Error>> {
    let mut body = DelayBlockBody::new(delay_ms);
    if let Some(forward_message) = forward_message {
        body.set_forward_message(forward_message);
    }
    let mut block = Block::new(BlockType::DELAY, BlockExecutionType::Trigger);
    block.set_block_body(BlockBody::DELAY(body));
    Ok(block)
}
