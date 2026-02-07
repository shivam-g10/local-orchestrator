//! User-facing Block enum: build blocks and convert to BlockConfig for adding to a workflow.
//!
//! This enum covers only the built-in block types in this crate. For third-party or custom block types,
//! construct [`BlockConfig::Custom`](orchestrator_core::BlockConfig) directly with the appropriate
//! `type_id` and `payload`, and ensure that `type_id` is registered in the workflow's registry.

use crate::{
    AiGenerateConfig, CombineConfig, CronConfig, CustomTransformConfig, FileReadConfig,
    FileWriteConfig, HttpRequestConfig, ListDirectoryConfig, RssParseConfig, SelectFirstConfig,
    SendEmailConfig, SplitByKeysConfig, SplitLinesConfig, TemplateHandlebarsConfig,
};
use orchestrator_core::WorkflowDefinition;
use orchestrator_core::block::{BlockConfig, ChildWorkflowConfig};

/// User-facing block: build with optional config, then add to a workflow.
#[derive(Debug, Clone)]
pub enum Block {
    AiGenerate {
        provider: String,
        model: String,
        prompt: String,
        api_key_env: String,
        timeout_ms: Option<u64>,
    },
    Cron {
        cron: String,
    },
    HttpRequest {
        url: Option<String>,
        timeout_ms: Option<u64>,
        user_agent: Option<String>,
    },
    ListDirectory {
        path: Option<String>,
        force_config_path: bool,
    },
    Combine {
        keys: Vec<String>,
    },
    CustomTransform {
        template: Option<String>,
    },
    SplitByKeys {
        keys: Vec<String>,
    },
    FileWrite {
        path: Option<String>,
        append: bool,
    },
    MarkdownToHtml,
    FileRead {
        path: Option<String>,
        force_config_path: bool,
    },
    RssParse,
    SelectFirst {
        strategy: Option<String>,
    },
    SplitLines {
        delimiter: String,
        trim_each: bool,
        skip_empty: bool,
    },
    TemplateHandlebars {
        template: Option<String>,
        partials: Option<serde_json::Value>,
    },
    SendEmail {
        to: String,
        subject: Option<String>,
    },
    ChildWorkflow {
        definition: WorkflowDefinition,
    },
}

impl Block {
    pub fn ai_generate(
        prompt: impl Into<String>,
        provider: Option<impl Into<String>>,
        model: Option<impl Into<String>>,
        api_key_env: Option<impl Into<String>>,
    ) -> Self {
        Block::AiGenerate {
            provider: provider
                .map(|p| p.into())
                .unwrap_or_else(|| "openai".to_string()),
            model: model
                .map(|m| m.into())
                .unwrap_or_else(|| "gpt-5-nano".to_string()),
            prompt: prompt.into(),
            api_key_env: api_key_env
                .map(|k| k.into())
                .unwrap_or_else(|| "OPENAI_API_KEY".to_string()),
            timeout_ms: None,
        }
    }

    pub fn cron(cron: impl Into<String>) -> Self {
        Block::Cron { cron: cron.into() }
    }

    pub fn http_request(url: Option<impl Into<String>>) -> Self {
        Block::HttpRequest {
            url: url.map(Into::into),
            timeout_ms: None,
            user_agent: None,
        }
    }

    pub fn list_directory(path: Option<impl Into<String>>) -> Self {
        Block::ListDirectory {
            path: path.map(Into::into),
            force_config_path: false,
        }
    }

    /// List directory at config path, ignoring upstream input (e.g. when entry is Cron).
    pub fn list_directory_force_config(path: Option<impl Into<String>>) -> Self {
        Block::ListDirectory {
            path: path.map(Into::into),
            force_config_path: true,
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

    pub fn file_write(path: Option<impl Into<String>>) -> Self {
        Block::FileWrite {
            path: path.map(Into::into),
            append: false,
        }
    }

    pub fn file_write_append(path: Option<impl Into<String>>) -> Self {
        Block::FileWrite {
            path: path.map(Into::into),
            append: true,
        }
    }

    pub fn markdown_to_html() -> Self {
        Block::MarkdownToHtml
    }

    pub fn file_read(path: Option<impl Into<String>>) -> Self {
        Block::FileRead {
            path: path.map(Into::into),
            force_config_path: false,
        }
    }

    pub fn file_read_force_config(path: Option<impl Into<String>>) -> Self {
        Block::FileRead {
            path: path.map(Into::into),
            force_config_path: true,
        }
    }

    pub fn rss_parse() -> Self {
        Block::RssParse
    }

    pub fn select_first(strategy: Option<impl Into<String>>) -> Self {
        Block::SelectFirst {
            strategy: strategy.map(|s| s.into()),
        }
    }

    pub fn split_lines() -> Self {
        let cfg = SplitLinesConfig::default();
        Block::SplitLines {
            delimiter: cfg.delimiter,
            trim_each: cfg.trim_each,
            skip_empty: cfg.skip_empty,
        }
    }

    pub fn template_handlebars(
        template: Option<impl Into<String>>,
        partials: Option<serde_json::Value>,
    ) -> Self {
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
            Block::AiGenerate {
                provider,
                model,
                prompt,
                api_key_env,
                timeout_ms,
            } => BlockConfig::Custom {
                type_id: "ai_generate".to_string(),
                payload: serde_json::to_value(AiGenerateConfig {
                    provider,
                    model,
                    prompt,
                    api_key_env,
                    timeout_ms,
                })
                .unwrap(),
            },
            Block::Cron { cron } => BlockConfig::Custom {
                type_id: "cron".to_string(),
                payload: serde_json::to_value(CronConfig::new(cron)).unwrap(),
            },
            Block::HttpRequest {
                url,
                timeout_ms,
                user_agent,
            } => BlockConfig::Custom {
                type_id: "http_request".to_string(),
                payload: serde_json::to_value(HttpRequestConfig {
                    url,
                    timeout_ms,
                    user_agent,
                })
                .unwrap(),
            },
            Block::ListDirectory {
                path,
                force_config_path,
            } => BlockConfig::Custom {
                type_id: "list_directory".to_string(),
                payload: serde_json::to_value(
                    ListDirectoryConfig::new(path).with_force_config_path(force_config_path),
                )
                .unwrap(),
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
            Block::FileWrite { path, append } => BlockConfig::Custom {
                type_id: "file_write".to_string(),
                payload: serde_json::to_value(FileWriteConfig::new(path).with_append(append))
                    .unwrap(),
            },
            Block::MarkdownToHtml => BlockConfig::Custom {
                type_id: "markdown_to_html".to_string(),
                payload: serde_json::json!({}),
            },
            Block::FileRead {
                path,
                force_config_path,
            } => BlockConfig::Custom {
                type_id: "file_read".to_string(),
                payload: serde_json::to_value(
                    FileReadConfig::new(path).with_force_config_path(force_config_path),
                )
                .unwrap(),
            },
            Block::RssParse => BlockConfig::Custom {
                type_id: "rss_parse".to_string(),
                payload: serde_json::to_value(RssParseConfig::default()).unwrap(),
            },
            Block::SelectFirst { strategy } => BlockConfig::Custom {
                type_id: "select_first".to_string(),
                payload: serde_json::to_value(SelectFirstConfig::new(strategy)).unwrap(),
            },
            Block::SplitLines {
                delimiter,
                trim_each,
                skip_empty,
            } => BlockConfig::Custom {
                type_id: "split_lines".to_string(),
                payload: serde_json::to_value(SplitLinesConfig {
                    delimiter,
                    trim_each,
                    skip_empty,
                })
                .unwrap(),
            },
            Block::TemplateHandlebars { template, partials } => BlockConfig::Custom {
                type_id: "template_handlebars".to_string(),
                payload: serde_json::to_value(TemplateHandlebarsConfig { template, partials })
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
