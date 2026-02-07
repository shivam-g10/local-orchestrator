mod executor;
pub mod utils;

use std::str::FromStr;

use cron::Schedule;

pub use executor::execute_cron;

#[derive(Debug, Clone)]
pub struct CronBlockBody {
    pub cron: String,
}

impl CronBlockBody {
    pub fn new(cron: String) -> Result<Self, cron::error::Error> {
        match Schedule::from_str(cron.as_str()) {
            Ok(_) => Ok(Self { cron }),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod test {

    use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

    use super::*;

    #[test]
    fn create_new_cron_block() {
        let mut block = Block::new(BlockType::CRON, BlockExecutionType::Trigger);
        let result = CronBlockBody::new("* * * * * * *".to_string());
        let body: CronBlockBody = result.unwrap();
        block.set_block_body(BlockBody::CRON(body));
        assert_eq!(block.block_type, BlockType::CRON);
        assert!(block.block_body.is_some());
        if let Some(BlockBody::CRON(block_body)) = block.block_body {
            assert_eq!(block_body.cron, "* * * * * * *".to_string());
        } else {
            panic!("No block available in create new CRON");
        };

        assert!(!block.id.is_nil());
    }

    #[test]
    fn cron_error_test() {
        let result = CronBlockBody::new("* * * * * * * *".to_string());
        assert!(result.is_err())
    }
}
