use crate::block::{
    AIBlockBody, AIProvider, Block, BlockBody, BlockType, CronBlockBody, FileBlockBody,
    FileOperationType,
};

pub fn create_ai_block(provider: AIProvider, api_key: &str, prompt: &str) -> Block {
    let body = AIBlockBody::new(provider, prompt.to_string(), api_key.to_string());
    let mut block = Block::new(BlockType::AI);
    block.set_block_body(BlockBody::AI(body));
    return block;
}

pub fn create_cron_block(cron: &str) -> Result<Block, cron::error::Error> {
    let body_result = CronBlockBody::new(cron.to_string());
    let body = match body_result {
        Err(err) => {
            return Err(err);
        }
        Ok(body) => body,
    };
    let mut block = Block::new(BlockType::CRON);
    block.set_block_body(BlockBody::CRON(body));
    return Ok(block);
}

pub fn create_file_block(operation: FileOperationType, locaation: &str, file_name: &str) -> Block {
    let body = FileBlockBody::new(operation, locaation.to_string(), file_name.to_string());
    let mut block = Block::new(BlockType::FILE);
    block.set_block_body(BlockBody::FILE(body));
    return block;
}
