use crate::block::{Block, BlockType, BlockBody};
use super::CronBlockBody;

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
    Ok(block)
}