mod executor;
pub mod utils;
mod open_ai;
mod fs_tools;
pub use executor::execute_ai;

use crate::block::ai::fs_tools::FsTools;

#[derive(Debug, PartialEq, Clone)]
pub enum AIProvider {
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct AIBlockBody {
    pub provider: AIProvider,
    pub prompt: String,
    pub api_key: String,
    pub fs_tools: Option<FsTools>,
}
impl AIBlockBody {
    pub fn new(provider: AIProvider, prompt: String, api_key: String) -> Self {
        Self {
            provider,
            prompt,
            api_key,
            fs_tools: None,
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

    pub fn set_fs_tools(&mut self, fs_tools: Option<FsTools>) -> &mut Self {
        self.fs_tools = fs_tools;
        self
    }
}



#[cfg(test)]
mod test {

    use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

    use super::*;
    #[test]
    fn create_new_ai_block() {
        let mut block = Block::new(BlockType::AI, BlockExecutionType::Response);
        let body = AIBlockBody::new(AIProvider::OpenAi, "Hi".to_string(), "ABC".to_string());
        block.set_block_body(BlockBody::AI(body));
        assert_eq!(block.block_type, BlockType::AI);
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
}