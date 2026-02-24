/// OpenAI reasoning effort hint (when supported by the selected model/API).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiReasoningEffort {
    /// Lower latency / cost-oriented reasoning.
    Low,
    /// Balanced reasoning.
    Medium,
    /// Higher effort reasoning.
    High,
}

/// Per-run OpenAI request options.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OpenAiRequestOptions {
    /// Whether OpenAI should store the response server-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// Optional reasoning effort hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<OpenAiReasoningEffort>,
}

impl OpenAiRequestOptions {
    /// Sets the `store` flag for the request.
    pub fn store(mut self, store: bool) -> Self {
        self.store = Some(store);
        self
    }

    /// Sets the reasoning effort hint.
    pub fn reasoning_effort(mut self, effort: OpenAiReasoningEffort) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }
}
