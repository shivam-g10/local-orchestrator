//! Built-in blocks for the orchestrator. Use with [`default_registry`] or [`new_workflow`].

mod block;
mod combine;
mod cron;
mod custom_transform;
mod file_read;
mod file_write;
mod list_directory;
mod markdown_to_html;
mod send_email;
mod split_by_keys;
mod template_handlebars;

pub use block::Block;
pub use combine::{CombineBlock, CombineConfig};
pub use cron::{CronBlock, CronConfig};
pub use custom_transform::{CustomTransformBlock, CustomTransformConfig};
pub use file_read::{FileReadBlock, FileReadConfig};
pub use file_write::{FileWriteBlock, FileWriteConfig};
pub use list_directory::{ListDirectoryBlock, ListDirectoryConfig};
pub use markdown_to_html::{MarkdownToHtmlBlock, MarkdownToHtmlConfig};
pub use send_email::{SendEmail, SendEmailBlock, SendEmailConfig, SendEmailError};
pub use split_by_keys::{SplitByKeysBlock, SplitByKeysConfig};
pub use template_handlebars::{TemplateHandlebarsBlock, TemplateHandlebarsConfig};

pub use orchestrator_core::{
    BlockConfig, BlockId, BlockOutput, BlockRegistry, RunError, Workflow, WorkflowDefinition,
};

/// Create a registry with built-in blocks (Cron, FileRead, FileWrite, etc.).
/// Does not include send_email; use [`registry_with_mailer`] if you need email.
pub fn default_registry() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    cron::register_cron(&mut r);
    list_directory::register_list_directory(&mut r);
    combine::register_combine(&mut r);
    custom_transform::register_custom_transform(&mut r);
    split_by_keys::register_split_by_keys(&mut r);
    file_write::register_file_write(&mut r);
    markdown_to_html::register_markdown_to_html(&mut r);
    file_read::register_file_read(&mut r);
    template_handlebars::register_template_handlebars(&mut r);
    r
}

/// Create a registry with all built-in blocks including send_email, using the given mailer.
/// Use this when you want to use the SendEmail block; pass your implementation of [`SendEmail`].
pub fn registry_with_mailer(mailer: std::sync::Arc<dyn send_email::SendEmail>) -> BlockRegistry {
    let mut r = default_registry();
    send_email::register_send_email(&mut r, mailer);
    r
}

/// Create a workflow with the default built-in blocks registry. Equivalent to
/// `Workflow::with_registry(default_registry())`.
pub fn new_workflow() -> Workflow {
    Workflow::with_registry(default_registry())
}
