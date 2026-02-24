//! Standalone AI harness crate with a builder-first async API.
//!
//! Vendor-specific APIs are namespaced under `vendors::*`.
//!
//! # Builder-first usage (OpenAI)
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use orchestrator_ai_harness::prelude::*;
//! use orchestrator_ai_harness::vendors::openai::{
//!     OpenAiProvider, OpenAiRequestOptions, OpenAiRunBuilderExt,
//! };
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), HarnessError> {
//! let harness = Harness::builder()
//!     .register_provider(Arc::new(OpenAiProvider::from_env()?))
//!     .build()?;
//!
//! let text = harness
//!     .session(SessionConfig::named("demo"))
//!     .run(ModelRef::new("openai", "gpt-5-nano"))
//!     .system_prompt("Answer briefly.")
//!     .user_text("Say hello")
//!     .openai_options(OpenAiRequestOptions::default().store(false))
//!     .collect_text()
//!     .await?;
//!
//! println!("{text}");
//! # Ok(())
//! # }
//! ```

/// Input/output content types and final run output helpers.
pub mod content;
/// Public error types used by the harness API.
pub mod errors;
/// Harness entry point and builder.
pub mod harness;
/// Model and provider identifiers plus generic run options.
pub mod model;
/// Common imports for typical usage.
pub mod prelude;
/// Provider adapter contracts used by vendor integrations.
pub mod provider;
/// Run builder, streaming handle, and cancellation handle.
pub mod run;
/// Session configuration and session handle.
pub mod session;
/// Normalized public stream events.
pub mod stream;
/// Vendor-specific integrations and extension traits.
pub mod vendors;

pub use content::{InputPart, OutputPart, RunOutput};
pub use errors::{HarnessError, ProviderError, RunFailure};
pub use harness::{Harness, HarnessBuilder};
pub use model::{ModelRef, ProviderId, RunOptions};
pub use provider::{
    ProviderAdapter, ProviderEvent, ProviderRequest, ProviderResponseMeta, ProviderStreamHandle,
};
pub use run::{AbortHandle, RunBuilder, RunStream};
pub use session::{Session, SessionConfig};
pub use stream::StreamEvent;
