use crate::block::{Block, BlockBody, BlockExecutionType, BlockType, ai::fs_tools::{FsPolicy, FsTools}};
use super::AIBlockBody;

pub use super::AIProvider;


pub fn create_ai_block(provider: AIProvider, api_key: &str, prompt: &str) -> Block {
    let body = AIBlockBody::new(provider, prompt.to_string(), api_key.to_string());
    let mut block = Block::new(BlockType::AI, BlockExecutionType::Response);
    block.set_block_body(BlockBody::AI(body));
    block
}

pub fn create_fs_tools(path: &str) -> Result<FsTools, anyhow::Error> {
    let policy = FsPolicy::new(path)?;
    FsTools::new(policy)
}

pub fn add_fs_tools_to_ai_block(mut block: Block, fs_tools: FsTools) -> Result<Block, anyhow::Error> {
    if let Some(BlockBody::AI(mut body)) = block.get_body().clone() {
        body.set_fs_tools(Some(fs_tools));
        block.set_block_body(BlockBody::AI(body));
        Ok(block)
    } else {
        Err(anyhow::Error::msg("AI Block body not found in block"))
    }
}
