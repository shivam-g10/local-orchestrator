use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::errors::HarnessError;
use crate::model::ProviderId;
use crate::provider::ProviderAdapter;
use crate::session::{Session, SessionConfig};

pub(crate) struct HarnessInner {
    providers: HashMap<ProviderId, Arc<dyn ProviderAdapter>>,
}

impl HarnessInner {
    pub(crate) fn provider(&self, id: &ProviderId) -> Option<Arc<dyn ProviderAdapter>> {
        self.providers.get(id).cloned()
    }
}

/// Entry point for creating sessions and running models.
#[derive(Clone)]
pub struct Harness {
    pub(crate) inner: Arc<HarnessInner>,
}

impl Harness {
    /// Starts a builder for registering providers and creating a `Harness`.
    pub fn builder() -> HarnessBuilder {
        HarnessBuilder::default()
    }

    /// Creates a logical session for grouping related runs.
    pub fn session(&self, config: SessionConfig) -> Session {
        Session::new(self.inner.clone(), config)
    }
}

/// Builder used to register provider adapters before creating a `Harness`.
#[derive(Default)]
pub struct HarnessBuilder {
    providers: Vec<Arc<dyn ProviderAdapter>>,
}

impl HarnessBuilder {
    /// Registers a provider adapter.
    ///
    /// Register one adapter per provider id (for example one `openai` adapter).
    pub fn register_provider(mut self, provider: Arc<dyn ProviderAdapter>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Builds the harness and validates provider registration (including duplicates).
    pub fn build(self) -> Result<Harness, HarnessError> {
        let mut map: HashMap<ProviderId, Arc<dyn ProviderAdapter>> = HashMap::new();
        let mut seen: HashSet<ProviderId> = HashSet::new();
        for provider in self.providers {
            let id = provider.id();
            if !seen.insert(id.clone()) {
                return Err(HarnessError::Config(format!(
                    "duplicate provider registration: {id}"
                )));
            }
            map.insert(id, provider);
        }
        Ok(Harness {
            inner: Arc::new(HarnessInner { providers: map }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderAdapter, ProviderRequest, ProviderStreamHandle};
    use crate::{errors::ProviderError, model::ProviderId};

    struct DummyProvider;

    #[async_trait::async_trait]
    impl ProviderAdapter for DummyProvider {
        fn id(&self) -> ProviderId {
            ProviderId::new("dummy")
        }

        async fn start_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<ProviderStreamHandle, ProviderError> {
            unreachable!("not used in this test")
        }
    }

    #[test]
    fn build_rejects_duplicate_provider_ids() {
        let result = Harness::builder()
            .register_provider(Arc::new(DummyProvider))
            .register_provider(Arc::new(DummyProvider))
            .build();
        assert!(
            matches!(result, Err(HarnessError::Config(message)) if message.contains("duplicate provider"))
        );
    }
}
