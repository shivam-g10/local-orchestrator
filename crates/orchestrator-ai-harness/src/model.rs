use std::fmt;
use std::time::Duration;

/// Stable identifier for a provider implementation (for example `openai`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    /// Creates a provider id from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the provider id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ProviderId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ProviderId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Model selection for a run.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelRef {
    /// Provider that owns the model.
    pub provider: ProviderId,
    /// Provider-specific model name (for example `gpt-5-nano`).
    pub model: String,
}

impl ModelRef {
    /// Creates a model reference.
    pub fn new(provider: impl Into<ProviderId>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

/// Generic run behavior options.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RunOptions {
    /// Optional per-run timeout.
    pub timeout: Option<Duration>,
    /// Bounded event buffer size used by the streaming channel.
    pub stream_buffer_capacity: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: None,
            stream_buffer_capacity: 128,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_options_default_buffer_capacity() {
        assert_eq!(RunOptions::default().stream_buffer_capacity, 128);
    }
}
