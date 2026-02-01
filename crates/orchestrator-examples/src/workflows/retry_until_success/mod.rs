//! Retry until success: check (custom) -> Conditional (success?) -> sink.
//! Demonstrates cycle + Delay + Conditional pattern; full cycle (Delay -> check) needs runtime branch selection.
//!
//! ```text
//!   [Check stub] --> [Conditional == "200"] --> [Echo] --> "then" | "else"
//! ```

#![allow(dead_code)]

mod blocks;

use orchestrator_core::block::RuleKind;
use orchestrator_core::{Block, BlockRegistry, RunError, Workflow};

use blocks::{CheckBlock, CheckConfig};

fn make_registry() -> BlockRegistry {
    let mut r = BlockRegistry::default_with_builtins();
    r.register_custom("check", |payload| {
        let status = payload
            .get("stub_status")
            .and_then(|v| v.as_str())
            .unwrap_or("200")
            .to_string();
        Ok(Box::new(CheckBlock::new(status)))
    });
    r
}

/// Run retry_until_success: check -> conditional (equals "200"?) -> echo.
/// When stub_status is "200", conditional outputs "then" and sink runs.
pub fn run_retry_until_success_workflow(stub_status: &str) -> Result<String, RunError> {
    let registry = make_registry();
    let mut w = Workflow::with_registry(registry);

    let check_id = w.add_custom(
        "check",
        CheckConfig {
            stub_status: Some(stub_status.to_string()),
        },
    )?;
    let cond_id = w.add(Block::conditional(RuleKind::Equals, "200"));
    let echo_id = w.add(Block::echo());

    w.link(check_id, cond_id);
    w.link(cond_id, echo_id);

    let output = w.run()?;
    let s: Option<String> = output.into();
    Ok(s.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_until_success_runs_when_200() {
        let out = run_retry_until_success_workflow("200").unwrap();
        assert_eq!(out, "then");
    }

    #[test]
    fn retry_until_success_else_when_not_200() {
        let out = run_retry_until_success_workflow("retry").unwrap();
        assert_eq!(out, "else");
    }
}
