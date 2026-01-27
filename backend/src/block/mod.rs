use std::str::FromStr;

use cron::Schedule;
use std::fmt;
mod executor;
pub mod utils;
use uuid::Uuid;

use crate::block::executor::{ExecutorError, execute_ai, execute_cron, execute_file};
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

#[derive(Debug, PartialEq, Clone)]
pub enum AIProvider {
    OpenAi,
}

#[derive(Debug, PartialEq, Clone)]
pub struct AIBlockBody {
    pub provider: AIProvider,
    pub prompt: String,
    pub api_key: String,
}
impl AIBlockBody {
    pub fn new(provider: AIProvider, prompt: String, api_key: String) -> Self {
        Self {
            provider,
            prompt,
            api_key,
        }
    }

    pub fn set_provider(&mut self, provider: AIProvider) -> &mut Self {
        self.provider = provider;
        self
    }

    pub fn set_prompt(&mut self, prompt: String) -> &mut Self {
        self.prompt = prompt;
        self
    }
    pub fn set_api_key(&mut self, api_key: String) -> &mut Self {
        self.api_key = api_key;
        self
    }
}

#[derive(Debug, Clone)]
pub struct CronBlockBody {
    pub cron: String,
}

impl CronBlockBody {
    pub fn new(cron: String) -> Result<Self, cron::error::Error> {
        match Schedule::from_str(cron.as_str()) {
            Ok(_) => {
                Ok(Self { cron })
            }
            Err(err) => {
                Err(err)
            }
        }
    }
}

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
    next: Option<Vec<Uuid>>,
    block_body: Option<BlockBody>,
    block_type: BlockType,
}

impl Block {
    pub fn new(block_type: BlockType) -> Self {
        Self {
            id: Uuid::new_v4(),
            block_type,
            block_body: None,
            next: None,
        }
    }

    pub fn add_next_id(&mut self, next: Uuid) -> &mut Self {
        let mut next_array = match &self.next {
            Some(next) => next.clone(),
            None => Vec::new(),
        };
        if next_array.contains(&next) {
            return self;
        }

        next_array.push(next);
        self.next = Some(next_array);
        self
    }

    pub fn set_block_body(&mut self, body: BlockBody) -> &mut Self {
        self.block_body = Some(body);
        self
    }

    pub fn get_next_ids(&self) -> &Option<Vec<Uuid>> {
        &self.next
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

#[cfg(test)]
mod test {

    use super::*;
    #[test]
    fn create_new_ai_block() {
        let mut block = Block::new(BlockType::AI);
        let body = AIBlockBody::new(AIProvider::OpenAi, "Hi".to_string(), "ABC".to_string());
        block.set_block_body(BlockBody::AI(body));
        assert_eq!(block.block_type, BlockType::AI);
        assert_eq!(block.next, None);
        assert!(block.block_body.is_some());
        if let Some(BlockBody::AI(block_body)) = block.block_body {
            assert_eq!(block_body.api_key, "ABC".to_string());
            assert_eq!(block_body.prompt, "Hi".to_string());
            assert_eq!(block_body.provider, AIProvider::OpenAi);
        } else {
            panic!("No block available in create new AI");
        };

        assert!(!block.id.is_nil());
    }

    #[test]
    fn create_new_cron_block() {
        let mut block = Block::new(BlockType::CRON);
        let result = CronBlockBody::new("* * * * * * *".to_string());
        let body = result.unwrap();
        block.set_block_body(BlockBody::CRON(body));
        assert_eq!(block.block_type, BlockType::CRON);
        assert_eq!(block.next, None);
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
        assert_eq!(block.next, None);
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
