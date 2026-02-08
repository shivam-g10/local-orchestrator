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
use orchestrator_core::block::{BlockConfig, ChildWorkflowConfig};
use orchestrator_core::{BlockId, RetryPolicy, Workflow, WorkflowDefinition, WorkflowEndpoint};

/// User-facing block: build with optional config, then add to a workflow.
#[derive(Debug, Clone)]
pub enum Block {
    AiGenerate {
        provider: String,
        model: String,
        prompt: String,
        api_key_env: String,
        timeout_ms: Option<u64>,
        retry_policy: RetryPolicy,
    },
    Cron {
        cron: String,
    },
    HttpRequest {
        url: Option<String>,
        timeout_ms: Option<u64>,
        user_agent: Option<String>,
        retry_policy: RetryPolicy,
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
        timeout_ms: Option<u64>,
        retry_policy: RetryPolicy,
    },
    ChildWorkflow {
        definition: WorkflowDefinition,
        timeout_ms: Option<u64>,
        retry_policy: RetryPolicy,
    },
}

impl Block {
    fn default_http_retry_policy() -> RetryPolicy {
        RetryPolicy::exponential(2, 1_000, 2.0)
    }

    fn default_email_retry_policy() -> RetryPolicy {
        RetryPolicy::exponential(3, 1_000, 2.0)
    }

    fn default_ai_retry_policy() -> RetryPolicy {
        RetryPolicy::exponential(2, 2_000, 2.0)
    }

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
            timeout_ms: Some(120_000),
            retry_policy: Self::default_ai_retry_policy(),
        }
    }

    pub fn cron(cron: impl Into<String>) -> Self {
        Block::Cron { cron: cron.into() }
    }

    pub fn http_request(url: Option<impl Into<String>>) -> Self {
        Block::HttpRequest {
            url: url.map(Into::into),
            timeout_ms: Some(30_000),
            user_agent: None,
            retry_policy: Self::default_http_retry_policy(),
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
            timeout_ms: Some(30_000),
            retry_policy: Self::default_email_retry_policy(),
        }
    }

    pub fn child_workflow(definition: WorkflowDefinition) -> Self {
        Block::ChildWorkflow {
            definition,
            timeout_ms: None,
            retry_policy: RetryPolicy::none(),
        }
    }

    pub fn set_timeout_ms(mut self, timeout_ms: u64) -> Self {
        let timeout = Some(timeout_ms.max(1));
        match &mut self {
            Block::AiGenerate { timeout_ms, .. }
            | Block::HttpRequest { timeout_ms, .. }
            | Block::SendEmail { timeout_ms, .. }
            | Block::ChildWorkflow { timeout_ms, .. } => {
                *timeout_ms = timeout;
            }
            _ => {}
        }
        self
    }

    pub fn clear_timeout(mut self) -> Self {
        match &mut self {
            Block::AiGenerate { timeout_ms, .. }
            | Block::HttpRequest { timeout_ms, .. }
            | Block::SendEmail { timeout_ms, .. }
            | Block::ChildWorkflow { timeout_ms, .. } => {
                *timeout_ms = None;
            }
            _ => {}
        }
        self
    }

    pub fn set_retry_exponential(
        mut self,
        max_retries: u32,
        initial_backoff_ms: u64,
        backoff_factor: f64,
    ) -> Self {
        let retry_policy =
            RetryPolicy::exponential(max_retries, initial_backoff_ms, backoff_factor);
        match &mut self {
            Block::AiGenerate {
                retry_policy: r, ..
            }
            | Block::HttpRequest {
                retry_policy: r, ..
            }
            | Block::SendEmail {
                retry_policy: r, ..
            }
            | Block::ChildWorkflow {
                retry_policy: r, ..
            } => {
                *r = retry_policy;
            }
            _ => {}
        }
        self
    }

    pub fn clear_retry(mut self) -> Self {
        match &mut self {
            Block::AiGenerate { retry_policy, .. }
            | Block::HttpRequest { retry_policy, .. }
            | Block::SendEmail { retry_policy, .. }
            | Block::ChildWorkflow { retry_policy, .. } => {
                *retry_policy = RetryPolicy::none();
            }
            _ => {}
        }
        self
    }

    pub fn set_max_backoff_ms(mut self, max_backoff_ms: u64) -> Self {
        match &mut self {
            Block::AiGenerate { retry_policy, .. }
            | Block::HttpRequest { retry_policy, .. }
            | Block::SendEmail { retry_policy, .. }
            | Block::ChildWorkflow { retry_policy, .. } => {
                *retry_policy = retry_policy.clone().with_max_backoff_ms(max_backoff_ms);
            }
            _ => {}
        }
        self
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
                retry_policy,
            } => BlockConfig::Custom {
                type_id: "ai_generate".to_string(),
                payload: serde_json::to_value(AiGenerateConfig {
                    provider,
                    model,
                    prompt,
                    api_key_env,
                    timeout_ms,
                    retry_policy,
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
                retry_policy,
            } => BlockConfig::Custom {
                type_id: "http_request".to_string(),
                payload: serde_json::to_value(HttpRequestConfig {
                    url,
                    timeout_ms,
                    user_agent,
                    retry_policy,
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
            Block::SendEmail {
                to,
                subject,
                timeout_ms,
                retry_policy,
            } => BlockConfig::Custom {
                type_id: "send_email".to_string(),
                payload: serde_json::to_value(SendEmailConfig {
                    to,
                    subject,
                    smtp_host: None,
                    smtp_port: None,
                    timeout_ms,
                    retry_policy,
                })
                .unwrap(),
            },
            Block::ChildWorkflow {
                definition,
                timeout_ms,
                retry_policy,
            } => BlockConfig::ChildWorkflow(
                ChildWorkflowConfig::new(definition)
                    .with_timeout_ms(timeout_ms)
                    .with_retry_policy(retry_policy),
            ),
        }
    }
}

impl WorkflowEndpoint for Block {
    fn resolve(self, workflow: &mut Workflow) -> BlockId {
        workflow.add(self)
    }
}

impl WorkflowEndpoint for &Block {
    fn resolve(self, workflow: &mut Workflow) -> BlockId {
        workflow.add_ref(self as *const Block as usize, self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_block_policy_setters_are_serialized_into_config() {
        let cfg: BlockConfig = Block::http_request(Some("https://example.com"))
            .set_timeout_ms(45_000)
            .set_retry_exponential(4, 500, 2.0)
            .set_max_backoff_ms(5_000)
            .into();

        match cfg {
            BlockConfig::Custom { type_id, payload } => {
                assert_eq!(type_id, "http_request");
                assert_eq!(
                    payload.get("timeout_ms").and_then(|v| v.as_u64()),
                    Some(45_000)
                );
                let retry = payload.get("retry_policy").expect("retry_policy");
                assert_eq!(retry.get("max_retries").and_then(|v| v.as_u64()), Some(4));
                assert_eq!(
                    retry.get("initial_backoff_ms").and_then(|v| v.as_u64()),
                    Some(500)
                );
                assert_eq!(
                    retry.get("max_backoff_ms").and_then(|v| v.as_u64()),
                    Some(5_000)
                );
            }
            _ => panic!("expected custom http_request config"),
        }
    }

    #[test]
    fn send_email_defaults_include_timeout_and_retry_policy() {
        let cfg: BlockConfig = Block::send_email("user@example.com", Some("Subject")).into();
        match cfg {
            BlockConfig::Custom { type_id, payload } => {
                assert_eq!(type_id, "send_email");
                assert_eq!(
                    payload.get("timeout_ms").and_then(|v| v.as_u64()),
                    Some(30_000)
                );
                assert_eq!(
                    payload
                        .get("retry_policy")
                        .and_then(|p| p.get("max_retries"))
                        .and_then(|v| v.as_u64()),
                    Some(3)
                );
            }
            _ => panic!("expected custom send_email config"),
        }
    }
}
