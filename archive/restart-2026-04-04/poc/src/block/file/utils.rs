use super::FileBlockBody;
use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

pub use super::FileOperationType;

pub fn create_file_block(operation: FileOperationType, location: &str, file_name: &str) -> Block {
    let execution_type = match &operation {
        FileOperationType::WATCH => BlockExecutionType::Trigger,
        _ => BlockExecutionType::Response,
    };
    let body = FileBlockBody::new(operation, location.to_string(), file_name.to_string());
    let mut block = Block::new(BlockType::FILE, execution_type);
    block.set_block_body(BlockBody::FILE(body));
    block
}
