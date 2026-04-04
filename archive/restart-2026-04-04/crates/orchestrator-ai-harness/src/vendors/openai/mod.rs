//! OpenAI provider integration and request options.
//!
//! Vendor-specific configuration lives here so the root harness API can remain
//! provider-agnostic.
mod adapter;
mod config;
mod options;
pub(crate) mod transport;

pub use adapter::OpenAiProvider;
pub use config::OpenAiClientConfig;
pub use options::{OpenAiReasoningEffort, OpenAiRequestOptions};

use crate::ProviderId;
use crate::run::RunBuilder;

/// Extension trait for attaching OpenAI-specific options to a `RunBuilder`.
pub trait OpenAiRunBuilderExt {
    /// Adds OpenAI request options for the current run.
    ///
    /// These options are stored internally under the `openai` provider key and
    /// read only by `OpenAiProvider`.
    fn openai_options(self, options: OpenAiRequestOptions) -> Self;
}

impl OpenAiRunBuilderExt for RunBuilder {
    fn openai_options(self, options: OpenAiRequestOptions) -> Self {
        let value = serde_json::to_value(options)
            .expect("OpenAiRequestOptions serialization should be infallible");
        self.set_vendor_options_json(ProviderId::new("openai"), value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderAdapter, ProviderRequest, ProviderStreamHandle};
    use crate::{Harness, SessionConfig};
    use crate::{ProviderError, ProviderId};
    use std::sync::Arc;

    struct Dummy;

    #[async_trait::async_trait]
    impl ProviderAdapter for Dummy {
        fn id(&self) -> ProviderId {
            ProviderId::new("openai")
        }

        async fn start_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<ProviderStreamHandle, ProviderError> {
            unreachable!()
        }
    }

    #[test]
    fn openai_run_builder_ext_stores_options_under_openai_key() {
        let harness = Harness::builder()
            .register_provider(Arc::new(Dummy))
            .build()
            .expect("harness");
        let builder = harness
            .session(SessionConfig::named("t"))
            .run(crate::ModelRef::new("openai", "gpt-5-nano"))
            .user_text("hello")
            .openai_options(OpenAiRequestOptions::default().store(true));

        let value = builder
            .vendor_options_value(&ProviderId::new("openai"))
            .expect("stored option");
        assert_eq!(value.get("store").and_then(|v| v.as_bool()), Some(true));
    }
}
