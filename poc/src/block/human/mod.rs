use uuid::Uuid;

mod executor;
pub mod utils;

pub use executor::execute_human;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanBlockType {
    Approval, // approve/reject 
    Play, // Click and play
    Form, // Sumbit form data
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormDataType {
    SingleLine,
    TextArea,
    MultiSelect,
    SingleSelect,
    Switch,
    Date,
    DateTime,
    DateRange,
    DateTimeRange,
    Number,
    Phone,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldConfig {
    pub id: Uuid,
    pub label: String,
    pub field_data_type: FormDataType,
    pub placeholder: Option<String>,
    pub length: Option<u16>,
    pub min: Option<String>, // date or number as string
    pub max: Option<String>,
    pub file_type: Option<String>,
    pub options: Option<Vec<FormFieldOptions>>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldOptions {
    pub id: Uuid,
    pub value: String,
    pub label: String,
}

impl FormFieldConfig {
    pub fn new(label: &str, field_data_type: FormDataType, required: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.to_string(),
            field_data_type,
            placeholder: None,
            length: None,
            file_type: None,
            max: None,
            min: None,
            options: None,
            required,
        }
    }

    pub fn add_option(&mut self, label: &str, value: &str) -> &mut Self {
        let option = FormFieldOptions::new(label, value);
        match self.options {
            Some(ref mut option_list) => {
                option_list.push(option);
            },
            None => {
                self.options = Some(vec![option]);
            }
        }
        self
    }
}

impl FormFieldOptions {
    pub fn new(label: &str, value: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.to_string(),
            value: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSubmit {
    id: Uuid,
    value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HumanBlockBody {
    pub human_block_type: HumanBlockType,
    pub form_config: Option<Vec<FormFieldConfig>>,
}

impl HumanBlockBody {
    pub fn new(human_block_type: HumanBlockType) -> Self {
        Self {
            human_block_type,
            form_config: None,
        }
    }

    pub fn get_form_config(&self) -> &Option<Vec<FormFieldConfig>> {
        &self.form_config    
    }

    pub fn set_form_config(&mut self, form_config: Vec<FormFieldConfig>) {
        self.form_config = Some(form_config);
    }
}

#[cfg(test)]
mod test {

    use crate::block::{Block, BlockBody, BlockExecutionType, BlockType};

    use super::*;

    #[test]
    fn create_file_block() {
        let mut block = Block::new(BlockType::FILE, BlockExecutionType::Trigger);
        let body = HumanBlockBody::new(
            HumanBlockType::Play,
        );
        block.set_block_body(BlockBody::HUMAN(body));
        assert_eq!(block.block_type, BlockType::HUMAN);
        assert!(block.block_body.is_some());
        if let Some(BlockBody::HUMAN(block_body)) = block.block_body {
            assert_eq!(block_body.human_block_type, HumanBlockType::Play);
            assert_eq!(block_body.form_config, None);
        } else {
            panic!("No block available in create new FILE");
        };

        assert!(!block.id.is_nil());
    }
}
