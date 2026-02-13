//! User-facing Block API: build blocks, declare input dependencies, and convert to BlockConfig.
//!
//! This crate's `Block` supports ergonomic wiring with `Workflow::link` / `Workflow::on_error`.

use smallvec::SmallVec;

use crate::{
    AiGenerateConfig, CombineConfig, CronConfig, CustomTransformConfig, FileReadConfig,
    FileWriteConfig, HttpRequestConfig, ListDirectoryConfig, RssParseConfig, SelectFirstConfig,
    SendEmailConfig, SplitByKeysConfig, SplitLinesConfig, TemplateHandlebarsConfig,
};
use orchestrator_core::block::{BlockConfig, ChildWorkflowConfig};
use orchestrator_core::{BlockId, RetryPolicy, Workflow, WorkflowDefinition, WorkflowEndpoint};

#[derive(Debug, Clone)]
enum BlockKind {
    AiGenerate {
        provider: String,
        model: String,
        prompt: Option<String>,
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
        to: Option<String>,
        subject: Option<String>,
        timeout_ms: Option<u64>,
        retry_policy: RetryPolicy,
    },
    ChildWorkflow {
        definition: WorkflowDefinition,
        timeout_ms: Option<u64>,
        retry_policy: RetryPolicy,
    },
    Custom {
        type_id: String,
        payload: serde_json::Value,
    },
}

/// User-facing block with optional explicit input dependencies.
#[derive(Debug, Clone)]
pub struct Block {
    kind: BlockKind,
    input_from: SmallVec<[usize; 2]>,
}

impl Block {
    fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            input_from: SmallVec::new(),
        }
    }

    fn default_http_retry_policy() -> RetryPolicy {
        RetryPolicy::exponential(2, 1_000, 2.0)
    }

    fn default_email_retry_policy() -> RetryPolicy {
        RetryPolicy::exponential(3, 1_000, 2.0)
    }

    fn default_ai_retry_policy() -> RetryPolicy {
        RetryPolicy::exponential(2, 2_000, 2.0)
    }

    pub fn with_input_from(mut self, source: &Block) -> Self {
        let source_ref_key = source as *const Block as usize;
        if !self.input_from.contains(&source_ref_key) {
            self.input_from.push(source_ref_key);
        }
        self
    }

    pub fn with_inputs_from(mut self, sources: &[&Block]) -> Self {
        for source in sources {
            let source_ref_key = *source as *const Block as usize;
            if !self.input_from.contains(&source_ref_key) {
                self.input_from.push(source_ref_key);
            }
        }
        self
    }

    pub fn input_source_ref_keys(&self) -> &[usize] {
        &self.input_from
    }

    pub fn ai_generate(
        prompt: impl Into<String>,
        provider: Option<impl Into<String>>,
        model: Option<impl Into<String>>,
        api_key_env: Option<impl Into<String>>,
    ) -> Self {
        Self::new(BlockKind::AiGenerate {
            provider: provider
                .map(|p| p.into())
                .unwrap_or_else(|| "openai".to_string()),
            model: model
                .map(|m| m.into())
                .unwrap_or_else(|| "gpt-5-nano".to_string()),
            prompt: Some(prompt.into()),
            api_key_env: api_key_env
                .map(|k| k.into())
                .unwrap_or_else(|| "OPENAI_API_KEY".to_string()),
            timeout_ms: Some(120_000),
            retry_policy: Self::default_ai_retry_policy(),
        })
    }

    pub fn cron(cron: impl Into<String>) -> Self {
        Self::new(BlockKind::Cron { cron: cron.into() })
    }

    pub fn http_request(url: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::HttpRequest {
            url: url.map(Into::into),
            timeout_ms: Some(30_000),
            user_agent: None,
            retry_policy: Self::default_http_retry_policy(),
        })
    }

    pub fn list_directory(path: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::ListDirectory {
            path: path.map(Into::into),
            force_config_path: false,
        })
    }

    /// List directory at config path, ignoring upstream input (e.g. when entry is Cron).
    pub fn list_directory_force_config(path: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::ListDirectory {
            path: path.map(Into::into),
            force_config_path: true,
        })
    }

    pub fn combine(keys: impl Into<Vec<String>>) -> Self {
        Self::new(BlockKind::Combine { keys: keys.into() })
    }

    pub fn custom_transform(template: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::CustomTransform {
            template: template.map(|t| t.into()),
        })
    }

    pub fn split_by_keys(keys: impl Into<Vec<String>>) -> Self {
        Self::new(BlockKind::SplitByKeys { keys: keys.into() })
    }

    pub fn file_write(path: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::FileWrite {
            path: path.map(Into::into),
            append: false,
        })
    }

    pub fn file_write_append(path: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::FileWrite {
            path: path.map(Into::into),
            append: true,
        })
    }

    pub fn markdown_to_html() -> Self {
        Self::new(BlockKind::MarkdownToHtml)
    }

    pub fn file_read(path: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::FileRead {
            path: path.map(Into::into),
            force_config_path: false,
        })
    }

    pub fn file_read_force_config(path: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::FileRead {
            path: path.map(Into::into),
            force_config_path: true,
        })
    }

    pub fn rss_parse() -> Self {
        Self::new(BlockKind::RssParse)
    }

    pub fn select_first(strategy: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::SelectFirst {
            strategy: strategy.map(|s| s.into()),
        })
    }

    pub fn split_lines() -> Self {
        let cfg = SplitLinesConfig::default();
        Self::new(BlockKind::SplitLines {
            delimiter: cfg.delimiter,
            trim_each: cfg.trim_each,
            skip_empty: cfg.skip_empty,
        })
    }

    pub fn template_handlebars(
        template: Option<impl Into<String>>,
        partials: Option<serde_json::Value>,
    ) -> Self {
        Self::new(BlockKind::TemplateHandlebars {
            template: template.map(|t| t.into()),
            partials,
        })
    }

    pub fn send_email(to: impl Into<String>, subject: Option<impl Into<String>>) -> Self {
        Self::new(BlockKind::SendEmail {
            to: Some(to.into()),
            subject: subject.map(|s| s.into()),
            timeout_ms: Some(30_000),
            retry_policy: Self::default_email_retry_policy(),
        })
    }

    pub fn child_workflow(definition: WorkflowDefinition) -> Self {
        Self::new(BlockKind::ChildWorkflow {
            definition,
            timeout_ms: None,
            retry_policy: RetryPolicy::none(),
        })
    }

    pub fn custom(type_id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self::new(BlockKind::Custom {
            type_id: type_id.into(),
            payload,
        })
    }

    pub fn set_timeout_ms(mut self, timeout_ms: u64) -> Self {
        let timeout = Some(timeout_ms.max(1));
        match &mut self.kind {
            BlockKind::AiGenerate { timeout_ms, .. }
            | BlockKind::HttpRequest { timeout_ms, .. }
            | BlockKind::SendEmail { timeout_ms, .. }
            | BlockKind::ChildWorkflow { timeout_ms, .. } => {
                *timeout_ms = timeout;
            }
            _ => {}
        }
        self
    }

    pub fn clear_timeout(mut self) -> Self {
        match &mut self.kind {
            BlockKind::AiGenerate { timeout_ms, .. }
            | BlockKind::HttpRequest { timeout_ms, .. }
            | BlockKind::SendEmail { timeout_ms, .. }
            | BlockKind::ChildWorkflow { timeout_ms, .. } => {
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
        match &mut self.kind {
            BlockKind::AiGenerate {
                retry_policy: r, ..
            }
            | BlockKind::HttpRequest {
                retry_policy: r, ..
            }
            | BlockKind::SendEmail {
                retry_policy: r, ..
            }
            | BlockKind::ChildWorkflow {
                retry_policy: r, ..
            } => {
                *r = retry_policy;
            }
            _ => {}
        }
        self
    }

    pub fn clear_retry(mut self) -> Self {
        match &mut self.kind {
            BlockKind::AiGenerate { retry_policy, .. }
            | BlockKind::HttpRequest { retry_policy, .. }
            | BlockKind::SendEmail { retry_policy, .. }
            | BlockKind::ChildWorkflow { retry_policy, .. } => {
                *retry_policy = RetryPolicy::none();
            }
            _ => {}
        }
        self
    }

    pub fn set_max_backoff_ms(mut self, max_backoff_ms: u64) -> Self {
        match &mut self.kind {
            BlockKind::AiGenerate { retry_policy, .. }
            | BlockKind::HttpRequest { retry_policy, .. }
            | BlockKind::SendEmail { retry_policy, .. }
            | BlockKind::ChildWorkflow { retry_policy, .. } => {
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
        match b.kind {
            BlockKind::AiGenerate {
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
                input_from: Box::new([]),
            },
            BlockKind::Cron { cron } => BlockConfig::Custom {
                type_id: "cron".to_string(),
                payload: serde_json::to_value(CronConfig::new(cron)).unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::HttpRequest {
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
                input_from: Box::new([]),
            },
            BlockKind::ListDirectory {
                path,
                force_config_path,
            } => BlockConfig::Custom {
                type_id: "list_directory".to_string(),
                payload: serde_json::to_value(
                    ListDirectoryConfig::new(path).with_force_config_path(force_config_path),
                )
                .unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::Combine { keys } => BlockConfig::Custom {
                type_id: "combine".to_string(),
                payload: serde_json::to_value(CombineConfig::new(keys)).unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::CustomTransform { template } => BlockConfig::Custom {
                type_id: "custom_transform".to_string(),
                payload: serde_json::to_value(CustomTransformConfig::new(template)).unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::SplitByKeys { keys } => BlockConfig::Custom {
                type_id: "split_by_keys".to_string(),
                payload: serde_json::to_value(SplitByKeysConfig::new(keys)).unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::FileWrite { path, append } => BlockConfig::Custom {
                type_id: "file_write".to_string(),
                payload: serde_json::to_value(FileWriteConfig::new(path).with_append(append))
                    .unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::MarkdownToHtml => BlockConfig::Custom {
                type_id: "markdown_to_html".to_string(),
                payload: serde_json::json!({}),
                input_from: Box::new([]),
            },
            BlockKind::FileRead {
                path,
                force_config_path,
            } => BlockConfig::Custom {
                type_id: "file_read".to_string(),
                payload: serde_json::to_value(
                    FileReadConfig::new(path).with_force_config_path(force_config_path),
                )
                .unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::RssParse => BlockConfig::Custom {
                type_id: "rss_parse".to_string(),
                payload: serde_json::to_value(RssParseConfig::default()).unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::SelectFirst { strategy } => BlockConfig::Custom {
                type_id: "select_first".to_string(),
                payload: serde_json::to_value(SelectFirstConfig::new(strategy)).unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::SplitLines {
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
                input_from: Box::new([]),
            },
            BlockKind::TemplateHandlebars { template, partials } => BlockConfig::Custom {
                type_id: "template_handlebars".to_string(),
                payload: serde_json::to_value(TemplateHandlebarsConfig { template, partials })
                    .unwrap(),
                input_from: Box::new([]),
            },
            BlockKind::SendEmail {
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
                input_from: Box::new([]),
            },
            BlockKind::ChildWorkflow {
                definition,
                timeout_ms,
                retry_policy,
            } => BlockConfig::ChildWorkflow(
                ChildWorkflowConfig::new(definition)
                    .with_timeout_ms(timeout_ms)
                    .with_retry_policy(retry_policy),
            ),
            BlockKind::Custom { type_id, payload } => BlockConfig::Custom {
                type_id,
                payload,
                input_from: Box::new([]),
            },
        }
    }
}

impl WorkflowEndpoint for Block {
    fn resolve(self, workflow: &mut Workflow) -> BlockId {
        let source_keys: Vec<usize> = self.input_source_ref_keys().to_vec();
        workflow.add_with_input_sources(self.into_config(), &source_keys)
    }
}

impl WorkflowEndpoint for &Block {
    fn resolve(self, workflow: &mut Workflow) -> BlockId {
        workflow.add_ref_with_input_sources(
            self as *const Block as usize,
            self.clone().into_config(),
            self.input_source_ref_keys(),
        )
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
            BlockConfig::Custom {
                type_id, payload, ..
            } => {
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
            BlockConfig::Custom {
                type_id, payload, ..
            } => {
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
