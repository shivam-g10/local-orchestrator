mod executor;
pub mod utils;
pub use executor::execute_delay;

#[derive(Debug, Clone)]
pub struct DelayBlockBody {
    pub delay_ms: u64,
    pub forward_message: bool,
}

impl DelayBlockBody {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            forward_message: false,
        }
    }

    pub fn set_forward_message(&mut self, forward_message: bool) {
        self.forward_message = forward_message;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

    #[test]
    fn test_delay_block() {
        let body = DelayBlockBody::new(1000);
        let mut block = Block::new(BlockType::DELAY, BlockExecutionType::Trigger);
        block.set_block_body(BlockBody::DELAY(body));

        assert_eq!(block.block_type, BlockType::DELAY);
        assert!(block.block_body.is_some());
        if let Some(BlockBody::DELAY(block_body)) = block.block_body {
            assert_eq!(block_body.delay_ms, 1000);
        } else {
            panic!("No block available in create new CRON");
        };

        assert!(!block.id.is_nil());
    }
}
