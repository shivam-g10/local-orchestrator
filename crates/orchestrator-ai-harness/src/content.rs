/// Input content sent to a model run.
///
/// `v1` is text-first, but the enum is non-exhaustive so new content kinds can
/// be added without breaking callers.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum InputPart {
    /// Plain text input.
    Text(String),
    /// Structured JSON input.
    Json(serde_json::Value),
}

/// Output content produced by a model run.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum OutputPart {
    /// Plain text output.
    Text(String),
    /// Structured JSON output.
    Json(serde_json::Value),
}

/// Final aggregated output for a completed run.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct RunOutput {
    /// Output parts in the order they were produced.
    pub parts: Vec<OutputPart>,
    /// Vendor-specific finish reason when available (for example `stop`).
    pub finish_reason: Option<String>,
}

impl RunOutput {
    /// Concatenates all text parts in order and ignores non-text parts.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            if let OutputPart::Text(text) = part {
                out.push_str(text);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_concatenates_text_parts_only() {
        let output = RunOutput {
            parts: vec![
                OutputPart::Text("hello".into()),
                OutputPart::Json(serde_json::json!({"a":1})),
                OutputPart::Text(" world".into()),
            ],
            finish_reason: None,
        };
        assert_eq!(output.text(), "hello world");
    }
}
