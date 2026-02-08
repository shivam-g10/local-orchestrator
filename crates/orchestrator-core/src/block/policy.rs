use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Exponential retry policy used by block-level reliability settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Additional attempts after the first execution.
    #[serde(default)]
    pub max_retries: u32,
    /// Initial backoff before the first retry.
    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
    /// Exponential multiplier per retry step.
    #[serde(default = "default_backoff_factor")]
    pub backoff_factor: f64,
    /// Upper bound for computed backoff.
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
}

const fn default_initial_backoff_ms() -> u64 {
    1_000
}

const fn default_backoff_factor() -> f64 {
    2.0
}

const fn default_max_backoff_ms() -> u64 {
    30_000
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::none()
    }
}

impl RetryPolicy {
    pub const fn none() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: default_initial_backoff_ms(),
            backoff_factor: default_backoff_factor(),
            max_backoff_ms: default_max_backoff_ms(),
        }
    }

    pub fn exponential(max_retries: u32, initial_backoff_ms: u64, backoff_factor: f64) -> Self {
        let initial = if initial_backoff_ms == 0 {
            default_initial_backoff_ms()
        } else {
            initial_backoff_ms
        };
        let factor = if backoff_factor <= 0.0 {
            default_backoff_factor()
        } else {
            backoff_factor
        };
        Self {
            max_retries,
            initial_backoff_ms: initial,
            backoff_factor: factor,
            max_backoff_ms: default_max_backoff_ms(),
        }
    }

    pub fn with_max_backoff_ms(mut self, max_backoff_ms: u64) -> Self {
        self.max_backoff_ms = max_backoff_ms.max(1);
        self
    }

    pub fn can_retry(&self, retries_done: u32) -> bool {
        retries_done < self.max_retries
    }

    pub fn backoff_duration(&self, retries_done: u32) -> Duration {
        if self.max_retries == 0 {
            return Duration::ZERO;
        }
        let exp = self.backoff_factor.powi(retries_done as i32);
        let base = (self.initial_backoff_ms as f64 * exp).round() as u64;
        let clamped = base.min(self.max_backoff_ms.max(1));
        Duration::from_millis(clamped)
    }
}

#[cfg(test)]
mod tests {
    use super::RetryPolicy;

    #[test]
    fn none_policy_has_zero_retries() {
        let p = RetryPolicy::none();
        assert_eq!(p.max_retries, 0);
        assert!(!p.can_retry(0));
    }

    #[test]
    fn exponential_policy_grows_with_cap() {
        let p = RetryPolicy::exponential(3, 100, 2.0).with_max_backoff_ms(250);
        assert_eq!(p.backoff_duration(0).as_millis(), 100);
        assert_eq!(p.backoff_duration(1).as_millis(), 200);
        assert_eq!(p.backoff_duration(2).as_millis(), 250);
    }
}
