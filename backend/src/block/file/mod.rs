mod executor;
pub mod utils;

pub use executor::execute_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOperationType {
    READ,
    WRITE,
}

#[derive(Debug, Clone)]
pub struct FileBlockBody {
    pub location: String,
    pub file_name: String,
    pub operation: FileOperationType,
}

impl FileBlockBody {
    pub fn new(operation: FileOperationType, location: String, file_name: String) -> Self {
        Self {
            operation,
            location,
            file_name,
        }
    }
}

#[cfg(test)]
mod test {

    use crate::block::{Block, BlockBody, BlockType};

    use super::*;

    #[test]
    fn create_file_block() {
        let mut block = Block::new(BlockType::FILE);
        let body = FileBlockBody::new(
            FileOperationType::READ,
            "dir".to_string(),
            "test".to_string(),
        );
        block.set_block_body(BlockBody::FILE(body));
        assert_eq!(block.block_type, BlockType::FILE);
        assert!(block.block_body.is_some());
        if let Some(BlockBody::FILE(block_body)) = block.block_body {
            assert_eq!(block_body.operation, FileOperationType::READ);
            assert_eq!(block_body.location, "dir".to_string());
            assert_eq!(block_body.file_name, "test".to_string());
        } else {
            panic!("No block available in create new FILE");
        };

        assert!(!block.id.is_nil());
    }
}
