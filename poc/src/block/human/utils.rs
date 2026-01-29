
use crate::block::{Block, BlockExecutionType, BlockType, BlockBody};
use super::{FormFieldConfig, HumanBlockBody};

pub use super::HumanBlockType;

pub fn create_human_block(human_type: HumanBlockType, form_config: Option<Vec<FormFieldConfig>>) -> Block {
    let mut body = HumanBlockBody::new(human_type);
    if let Some(form_config) = form_config {
        body.set_form_config(form_config);
    }
    let mut block = Block::new(BlockType::HUMAN, BlockExecutionType::Trigger);
    block.set_block_body(BlockBody::HUMAN(body));
    block
}
