use std::fmt;
pub mod ai;
pub mod cron;
mod executor_error;
pub mod file;
pub mod utils;
pub mod email;
mod human;
mod delay;

use uuid::Uuid;
use crossbeam::channel::Receiver;

use executor_error::ExecutorError;

use ai::{AIBlockBody, execute_ai};
use cron::{CronBlockBody, execute_cron};
use file::{FileBlockBody, execute_file};
use email::{EmailBlockBody, execute_email};
use human::{HumanBlockBody, execute_human};
use delay::{DelayBlockBody, execute_delay};

#[derive(Debug, PartialEq, PartialOrd, Clone, Copy)]
pub enum BlockType {
    AI,
    CRON,
    FILE,
    EMAIL,
    HUMAN,
    DELAY,
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait BlockExecutorTrait {
    fn get_id(&self) -> &Uuid;
    fn get_execution_type(&self) -> &BlockExecutionType;
    fn execute(&self, input: Option<String>) -> ExecutionRunResult;
    fn get_block_type(&self) -> &BlockType;
}

#[derive(Debug, Clone)]
pub enum BlockBody {
    AI(AIBlockBody),
    FILE(FileBlockBody),
    CRON(CronBlockBody),
    EMAIL(EmailBlockBody),
    HUMAN(HumanBlockBody),
    DELAY(DelayBlockBody),
}

#[derive(Debug, Clone)]
pub enum BlockExecutionType {
    Response,
    Trigger
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerType {
    OneShot,
    Recurring
}

#[derive(Debug, Clone)]
pub struct Block {
    id: Uuid,
    block_body: Option<BlockBody>,
    block_type: BlockType,
    execution_type: BlockExecutionType,
}

impl Block {
    pub fn new(block_type: BlockType, execution_type: BlockExecutionType) -> Self {
        Self {
            id: Uuid::new_v4(),
            block_type,
            block_body: None,
            execution_type,
        }
    }

    pub fn set_block_body(&mut self, body: BlockBody) -> &mut Self {
        self.block_body = Some(body);
        self
    }

    pub fn get_body(&self) -> &Option<BlockBody> {
        &self.block_body
    }
}

impl BlockExecutorTrait for Block {
    fn get_id(&self) -> &Uuid {
        &self.id
    }

    fn get_execution_type(&self) -> &BlockExecutionType {
        &self.execution_type
    }

    fn get_block_type(&self) -> &BlockType {
        &self.block_type
    }

    fn execute(&self, input: Option<String>) -> ExecutionRunResult {
        match self.block_body.clone() {
            Some(BlockBody::AI(body)) => execute_ai(input, body),
            Some(BlockBody::CRON(body)) => execute_cron(input, body),
            Some(BlockBody::FILE(body)) => execute_file(input, body),
            Some(BlockBody::EMAIL(body)) => execute_email(input, body),
            Some(BlockBody::HUMAN(body)) => execute_human(input, body),
            Some(BlockBody::DELAY(body)) => execute_delay(input, body),
            _ => Err(ExecutorError::NotImplemented("test".to_string())),

        }
    }
}

#[derive(Debug, Clone)]
pub enum ExecutionResult {
    Trigger(Receiver<Option<String>>, TriggerType),
    Response(Option<String>)
}

pub type ExecutionRunResult = Result<Option<ExecutionResult>, ExecutorError>;
