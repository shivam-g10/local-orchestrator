//! Built-in blocks for the orchestrator. Use with [`default_registry`] or [`new_workflow`].
//!
//! ## Registry and extensibility
//!
//! - **SendEmail**: [`default_registry`] does *not* register `send_email`. Use [`registry_with_mailer`]
//!   when your workflow uses the SendEmail block; otherwise you get "unknown custom block type: send_email" at run time.
//! - **Overriding a block**: You can replace a default impl by calling the same `register_XXX(registry, your_impl)`
//!   again (same `type_id` overwrites the previous registration). Example: start from `default_registry()`,
//!   then `file_read::register_file_read(&mut r, Arc::new(MyReader))` to use a custom file reader.
//! - **Custom registry**: Build a registry with only the blocks you need by creating `BlockRegistry::new()` and
//!   calling each `register_XXX(registry, impl)` for the blocks you use. Third-party blocks use
//!   `BlockConfig::Custom { type_id, payload }` and their own `registry.register_custom(type_id, factory)`.

mod block;
mod combine;
mod cron;
mod custom_transform;
mod file_read;
mod file_write;
mod list_directory;
mod markdown_to_html;
mod select_first;
mod send_email;
mod split_by_keys;
mod template_handlebars;

pub use block::Block;
pub use combine::{
    CombineBlock, CombineConfig, CombineError, CombineStrategy, KeyedCombineStrategy,
};
pub use cron::{CronBlock, CronConfig, CronError, CronRunner, StdCronRunner};
pub use custom_transform::{
    CustomTransformBlock, CustomTransformConfig, CustomTransformError, IdentityTransform,
    Transform,
};
pub use file_read::{
    FileReadBlock, FileReadConfig, FileReadError, FileReader, StdFileReader,
};
pub use file_write::{
    FileWriteBlock, FileWriteConfig, FileWriteError, FileWriter, StdFileWriter,
};
pub use list_directory::{
    DirectoryLister, ListDirectoryBlock, ListDirectoryConfig, ListDirectoryError,
    StdDirectoryLister,
};
pub use markdown_to_html::{
    MarkdownError, MarkdownToHtml, MarkdownToHtmlBlock, MarkdownToHtmlConfig,
    PulldownMarkdownRenderer, register_markdown_to_html,
};
pub use select_first::{
    ListSelector, SelectError, SelectFirstBlock, SelectFirstConfig, StdListSelector,
};
pub use send_email::{SendEmail, SendEmailBlock, SendEmailConfig, SendEmailError};
pub use split_by_keys::{
    KeyExtractSplitStrategy, SplitByKeysBlock, SplitByKeysConfig, SplitByKeysError,
    SplitByKeysStrategy,
};
pub use template_handlebars::{
    HandlebarsTemplateRenderer, TemplateHandlebarsBlock, TemplateHandlebarsConfig,
    TemplateError, TemplateRenderer,
};

pub use orchestrator_core::{
    BlockConfig, BlockId, BlockOutput, BlockRegistry, RunError, Workflow, WorkflowDefinition,
};

/// Create a registry with built-in blocks (Cron, FileRead, FileWrite, etc.)
/// using default implementations for each trait. Does not include send_email;
/// use [`registry_with_mailer`] if you need email.
pub fn default_registry() -> BlockRegistry {
    let mut r = BlockRegistry::new();
    cron::register_cron(&mut r, std::sync::Arc::new(cron::StdCronRunner));
    list_directory::register_list_directory(&mut r, std::sync::Arc::new(list_directory::StdDirectoryLister));
    combine::register_combine(&mut r, std::sync::Arc::new(combine::KeyedCombineStrategy));
    custom_transform::register_custom_transform(&mut r, std::sync::Arc::new(custom_transform::IdentityTransform));
    split_by_keys::register_split_by_keys(&mut r, std::sync::Arc::new(split_by_keys::KeyExtractSplitStrategy));
    file_write::register_file_write(&mut r, std::sync::Arc::new(file_write::StdFileWriter));
    markdown_to_html::register_markdown_to_html(&mut r, std::sync::Arc::new(markdown_to_html::PulldownMarkdownRenderer));
    file_read::register_file_read(&mut r, std::sync::Arc::new(file_read::StdFileReader));
    select_first::register_select_first(&mut r, std::sync::Arc::new(select_first::StdListSelector));
    template_handlebars::register_template_handlebars(&mut r, std::sync::Arc::new(template_handlebars::HandlebarsTemplateRenderer));
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
