use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};
use super::AIBlockBody;

pub use super::AIProvider;


pub fn create_ai_block(provider: AIProvider, api_key: &str, prompt: &str) -> Block {
    let body = AIBlockBody::new(provider, prompt.to_string(), api_key.to_string());
    let mut block = Block::new(BlockType::AI, BlockExecutionType::Response);
    block.set_block_body(BlockBody::AI(body));
    block
}

