//! Cyclic workflow demo: entry -> A -> B -> A (cycle), and A -> sink.
//! Demonstrates cycle handling and iteration budget. The runtime runs in iteration mode
//! until the budget is exhausted or the sink has produced output (here the cycle runs
//! until budget; sink receives from A on each pass through the cycle).
//!
//! ```text
//!   [entry] --> [repeater] --> [repeater2] --\
//!                  ^                |        |
//!                  \________________/       v
//!                                          [sink]
//! ```

use orchestrator_core::{Block, RunError, Workflow};

/// Build and run a workflow that contains a cycle: entry -> repeater -> repeater2 -> repeater,
/// and repeater -> sink. The runtime detects the cycle and runs in iteration mode.
/// Returns the sink (echo) output or an error.
#[allow(dead_code)]
pub fn cyclic_demo_workflow() -> Result<String, RunError> {
    let mut w = Workflow::new();
    let entry_id = w.add(Block::echo());
    let repeater_id = w.add(Block::echo());
    let repeater2_id = w.add(Block::echo());
    let sink_id = w.add(Block::echo());

    w.link(entry_id, repeater_id);
    w.link(repeater_id, repeater2_id);
    w.link(repeater2_id, repeater_id);
    w.link(repeater_id, sink_id);

    let output = w.run()?;
    let s: Option<String> = output.into();
    Ok(s.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclic_demo_runs() {
        let result = cyclic_demo_workflow();
        assert!(result.is_ok());
    }
}
