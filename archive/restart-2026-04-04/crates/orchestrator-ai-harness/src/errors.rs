use crate::model::ProviderId;

/// Errors returned by a provider adapter before they are normalized for the
/// public run stream.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// Provider returned an application-level failure (HTTP status, auth, etc.).
    #[error("provider error ({provider}): {message}")]
    Provider {
        provider: ProviderId,
        message: String,
        status_code: Option<u16>,
    },
    /// Transport or stream I/O failed.
    #[error("transport error ({provider}): {message}")]
    Transport {
        provider: ProviderId,
        message: String,
    },
    /// Provider response shape or event sequencing was invalid.
    #[error("protocol error ({provider}): {message}")]
    Protocol {
        provider: ProviderId,
        message: String,
    },
}

impl ProviderError {
    /// Creates a provider-level error.
    pub fn provider(
        provider: impl Into<ProviderId>,
        message: impl Into<String>,
        status_code: Option<u16>,
    ) -> Self {
        Self::Provider {
            provider: provider.into(),
            message: message.into(),
            status_code,
        }
    }

    /// Creates a transport-level error.
    pub fn transport(provider: impl Into<ProviderId>, message: impl Into<String>) -> Self {
        Self::Transport {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Creates a protocol-level error.
    pub fn protocol(provider: impl Into<ProviderId>, message: impl Into<String>) -> Self {
        Self::Protocol {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Returns the provider associated with this error.
    pub fn provider_id(&self) -> &ProviderId {
        match self {
            Self::Provider { provider, .. }
            | Self::Transport { provider, .. }
            | Self::Protocol { provider, .. } => provider,
        }
    }

    /// Returns the human-readable message for this error.
    pub fn message(&self) -> &str {
        match self {
            Self::Provider { message, .. }
            | Self::Transport { message, .. }
            | Self::Protocol { message, .. } => message,
        }
    }
}

/// Terminal run failure sent through `StreamEvent::Error`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum RunFailure {
    /// Provider returned a non-retryable or terminal failure.
    #[error("provider failure ({provider}): {message}")]
    Provider { provider: String, message: String },
    /// Network/stream transport failed.
    #[error("transport failure ({provider}): {message}")]
    Transport { provider: String, message: String },
    /// The harness detected a protocol or invariant error.
    #[error("protocol failure: {message}")]
    Protocol { message: String },
    /// The run was cancelled by the caller.
    #[error("run cancelled")]
    Cancelled,
}

/// Top-level error type for the public harness API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HarnessError {
    /// Invalid harness/provider configuration.
    #[error("config error: {0}")]
    Config(String),
    /// Invalid user input to the builder API.
    #[error("validation error: {0}")]
    Validation(String),
    /// Requested provider is not registered in the harness.
    #[error("provider not found: {provider}")]
    ProviderNotFound { provider: ProviderId },
    /// Provider startup/request error before the run stream is established.
    #[error(transparent)]
    Provider(ProviderError),
    /// Transport error surfaced outside the run stream.
    #[error("transport error: {0}")]
    Transport(String),
    /// Terminal failure returned from a started run.
    #[error(transparent)]
    RunFailed(RunFailure),
    /// Operation was cancelled before a terminal run result was returned.
    #[error("cancelled")]
    Cancelled,
    /// Internal protocol misuse or invariant violation.
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl HarnessError {
    pub(crate) fn run_failed(failure: RunFailure) -> Self {
        Self::RunFailed(failure)
    }

    pub(crate) fn protocol_msg(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }
}

impl From<RunFailure> for HarnessError {
    fn from(value: RunFailure) -> Self {
        HarnessError::RunFailed(value)
    }
}

pub(crate) fn run_failure_from_provider_error(err: &ProviderError) -> RunFailure {
    match err {
        ProviderError::Provider {
            provider, message, ..
        } => RunFailure::Provider {
            provider: provider.to_string(),
            message: message.clone(),
        },
        ProviderError::Transport { provider, message } => RunFailure::Transport {
            provider: provider.to_string(),
            message: message.clone(),
        },
        ProviderError::Protocol { provider, message } => RunFailure::Protocol {
            message: format!("provider={provider}: {message}"),
        },
    }
}
