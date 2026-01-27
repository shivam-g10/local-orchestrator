use std::fmt;
pub mod ai;
pub mod cron;
mod executor_error;
pub mod file;
pub mod utils;
use uuid::Uuid;

use executor_error::ExecutorError;

use ai::{AIBlockBody, execute_ai};
use cron::{CronBlockBody, execute_cron};
use file::{FileBlockBody, execute_file};

#[derive(Debug, PartialEq, PartialOrd, Clone, Copy)]
pub enum BlockType {
    AI,
    CRON,
    FILE,
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait BlockExecutorTrait {
    fn get_id(&self) -> &Uuid;
    fn execute(&self, input: Option<String>) -> Result<Option<String>, ExecutorError>;
}

#[derive(Debug, Clone)]
pub enum BlockBody {
    AI(AIBlockBody),
    FILE(FileBlockBody),
    CRON(CronBlockBody),
}

#[derive(Debug, Clone)]
pub struct Block {
    id: Uuid,
    block_body: Option<BlockBody>,
    block_type: BlockType,
}

impl Block {
    pub fn new(block_type: BlockType) -> Self {
        Self {
            id: Uuid::new_v4(),
            block_type,
            block_body: None,
        }
    }

    pub fn set_block_body(&mut self, body: BlockBody) -> &mut Self {
        self.block_body = Some(body);
        self
    }

    pub fn get_body(&self) -> &Option<BlockBody> {
        &self.block_body
    }
    pub fn get_block_type(&self) -> &BlockType {
        &self.block_type
    }
}

impl BlockExecutorTrait for Block {
    fn get_id(&self) -> &Uuid {
        &self.id
    }
    fn execute(&self, input: Option<String>) -> Result<Option<String>, ExecutorError> {
        match self.block_body.clone() {
            None => Err(ExecutorError::NotImplemented("test".to_string())),
            Some(BlockBody::AI(body)) => execute_ai(input, body),
            Some(BlockBody::CRON(body)) => execute_cron(input, body),
            Some(BlockBody::FILE(body)) => execute_file(input, body),
        }
    }
}
