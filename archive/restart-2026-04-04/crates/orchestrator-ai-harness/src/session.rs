use std::sync::Arc;

use crate::harness::HarnessInner;
use crate::model::ModelRef;
use crate::run::RunBuilder;

/// Configuration used to create a `Session`.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Human-readable session name (useful for logs and future persistence).
    pub name: String,
}

impl SessionConfig {
    /// Creates a named session config.
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Logical grouping for runs.
///
/// `v1` sessions are lightweight and in-memory only; they do not persist
/// history yet.
#[derive(Clone)]
pub struct Session {
    pub(crate) harness: Arc<HarnessInner>,
    pub(crate) session_id: uuid::Uuid,
    pub(crate) config: SessionConfig,
}

impl Session {
    pub(crate) fn new(harness: Arc<HarnessInner>, config: SessionConfig) -> Self {
        Self {
            harness,
            session_id: uuid::Uuid::new_v4(),
            config,
        }
    }

    /// Starts building a run for the given model.
    pub fn run(&self, model: ModelRef) -> RunBuilder {
        RunBuilder::new(
            self.harness.clone(),
            self.session_id,
            self.config.name.clone(),
            model,
        )
    }
}
