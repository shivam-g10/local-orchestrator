//! Invoice line processor: read file -> Split -> process one line at a time.
//! Demonstrates Split; optional cron (daily) is external.
//!
//! ```text
//!   [FileRead] --> [Split "\n"] --> [Echo] --> output (first line + rest as Json)
//! ```

#![allow(dead_code)]

use std::path::Path;

use orchestrator_core::{Block, RunError, Workflow};

/// Path to sample invoice lines (one item per line).
pub fn default_invoice_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("workflows")
        .join("invoice_line_processor")
        .join("data")
        .join("sample_lines.txt")
}

/// Run invoice line processor: file_read -> split (newline) -> echo.
/// Returns the sink output or error.
pub fn run_invoice_line_processor_workflow(
    input_path: impl AsRef<Path>,
) -> Result<String, RunError> {
    let path = input_path.as_ref();
    let mut w = Workflow::new();

    let path_str = path.to_string_lossy().into_owned();
    let read_id = w.add(Block::file_read(Some(path_str.as_str())));
    let split_id = w.add(Block::split("\n"));
    let echo_id = w.add(Block::echo());

    w.link(read_id, split_id);
    w.link(split_id, echo_id);

    let output = w.run()?;
    let s: Option<String> = output.into();
    Ok(s.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn invoice_line_processor_runs() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "line1").unwrap();
        writeln!(f, "line2").unwrap();
        f.flush().unwrap();
        let out = run_invoice_line_processor_workflow(f.path()).unwrap();
        assert!(out.contains("line1"));
    }
}
