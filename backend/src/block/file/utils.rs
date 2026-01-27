use crate::block::{Block, BlockType, BlockBody};
use super::FileBlockBody;

pub use super::FileOperationType;

pub fn create_file_block(operation: FileOperationType, location: &str, file_name: &str) -> Block {
    let body = FileBlockBody::new(operation, location.to_string(), file_name.to_string());
    let mut block = Block::new(BlockType::FILE);
    block.set_block_body(BlockBody::FILE(body));
    block
}
