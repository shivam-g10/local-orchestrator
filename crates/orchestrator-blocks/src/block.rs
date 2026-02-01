//! User-facing Block enum: build blocks and convert to BlockConfig for adding to a workflow.
//!
//! This enum covers only the built-in block types in this crate. For third-party or custom block types,
//! construct [`BlockConfig::Custom`](orchestrator_core::BlockConfig) directly with the appropriate
//! `type_id` and `payload`, and ensure that `type_id` is registered in the workflow's registry.

use crate::{
    CombineConfig, CronConfig, CustomTransformConfig, FileReadConfig, FileWriteConfig,
    ListDirectoryConfig, SendEmailConfig, SplitByKeysConfig, TemplateHandlebarsConfig,
};
use orchestrator_core::block::{BlockConfig, ChildWorkflowConfig};
use orchestrator_core::WorkflowDefinition;

/// User-facing block: build with optional config, then add to a workflow.
#[derive(Debug, Clone)]
pub enum Block {
    Cron { cron: String },
    ListDirectory { path: Option<String> },
    Combine { keys: Vec<String> },
    CustomTransform { template: Option<String> },
    SplitByKeys { keys: Vec<String> },
    FileWrite { path: Option<String> },
    MarkdownToHtml,
    FileRead { path: Option<String> },
    TemplateHandlebars {
        template: Option<String>,
        partials: Option<serde_json::Value>,
    },
    SendEmail { to: String, subject: Option<String> },
    ChildWorkflow {
        definition: WorkflowDefinition,
    },
}

impl Block {
    pub fn cron(cron: impl Into<String>) -> Self {
        Block::Cron { cron: cron.into() }
    }

    pub fn list_directory(path: Option<impl AsRef<str>>) -> Self {
        Block::ListDirectory {
            path: path.map(|p| p.as_ref().to_string()),
        }
    }

    pub fn combine(keys: impl Into<Vec<String>>) -> Self {
        Block::Combine { keys: keys.into() }
    }

    pub fn custom_transform(template: Option<impl Into<String>>) -> Self {
        Block::CustomTransform {
            template: template.map(|t| t.into()),
        }
    }

    pub fn split_by_keys(keys: impl Into<Vec<String>>) -> Self {
        Block::SplitByKeys { keys: keys.into() }
    }

    pub fn file_write(path: Option<impl AsRef<str>>) -> Self {
        Block::FileWrite {
            path: path.map(|p| p.as_ref().to_string()),
        }
    }

    pub fn markdown_to_html() -> Self {
        Block::MarkdownToHtml
    }

    pub fn file_read(path: Option<impl AsRef<str>>) -> Self {
        Block::FileRead {
            path: path.map(|p| p.as_ref().to_string()),
        }
    }

    pub fn template_handlebars(template: Option<impl Into<String>>, partials: Option<serde_json::Value>) -> Self {
        Block::TemplateHandlebars {
            template: template.map(|t| t.into()),
            partials,
        }
    }

    pub fn send_email(to: impl Into<String>, subject: Option<impl Into<String>>) -> Self {
        Block::SendEmail {
            to: to.into(),
            subject: subject.map(|s| s.into()),
        }
    }

    pub fn child_workflow(definition: WorkflowDefinition) -> Self {
        Block::ChildWorkflow { definition }
    }

    /// Convert this block to a BlockConfig for adding to a workflow.
    pub fn into_config(self) -> BlockConfig {
        self.into()
    }
}

impl From<Block> for BlockConfig {
    fn from(b: Block) -> Self {
        match b {
            Block::Cron { cron } => BlockConfig::Custom {
                type_id: "cron".to_string(),
                payload: serde_json::to_value(CronConfig::new(cron)).unwrap(),
            },
            Block::ListDirectory { path } => BlockConfig::Custom {
                type_id: "list_directory".to_string(),
                payload: serde_json::to_value(ListDirectoryConfig::new(path)).unwrap(),
            },
            Block::Combine { keys } => BlockConfig::Custom {
                type_id: "combine".to_string(),
                payload: serde_json::to_value(CombineConfig::new(keys)).unwrap(),
            },
            Block::CustomTransform { template } => BlockConfig::Custom {
                type_id: "custom_transform".to_string(),
                payload: serde_json::to_value(CustomTransformConfig::new(template)).unwrap(),
            },
            Block::SplitByKeys { keys } => BlockConfig::Custom {
                type_id: "split_by_keys".to_string(),
                payload: serde_json::to_value(SplitByKeysConfig::new(keys)).unwrap(),
            },
            Block::FileWrite { path } => BlockConfig::Custom {
                type_id: "file_write".to_string(),
                payload: serde_json::to_value(FileWriteConfig::new(path)).unwrap(),
            },
            Block::MarkdownToHtml => BlockConfig::Custom {
                type_id: "markdown_to_html".to_string(),
                payload: serde_json::json!({}),
            },
            Block::FileRead { path } => BlockConfig::Custom {
                type_id: "file_read".to_string(),
                payload: serde_json::to_value(FileReadConfig::new(path)).unwrap(),
            },
            Block::TemplateHandlebars { template, partials } => BlockConfig::Custom {
                type_id: "template_handlebars".to_string(),
                payload: serde_json::to_value(TemplateHandlebarsConfig {
                    template,
                    partials,
                })
                .unwrap(),
            },
            Block::SendEmail { to, subject } => BlockConfig::Custom {
                type_id: "send_email".to_string(),
                payload: serde_json::to_value(SendEmailConfig {
                    to,
                    subject,
                    smtp_host: None,
                    smtp_port: None,
                })
                .unwrap(),
            },
            Block::ChildWorkflow { definition } => {
                BlockConfig::ChildWorkflow(ChildWorkflowConfig::new(definition))
            }
        }
    }
}
