//! Example: read a file and pass through echo (multi-block chain).
//!
//! ```text
//!   [FileRead path] --> (sink; single-block run returns content)
//! ```

use orchestrator_core::{Block, RunError, Workflow};

/// Build and run the print-readme workflow: file_read(path) -> echo.
/// Returns the sink (echo) output or an error.
#[allow(dead_code)]
pub fn print_readme_workflow(path: &str) -> Result<String, RunError> {
    let mut w = Workflow::new();
    w.add(Block::file_read(Some(path)));
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
    fn print_readme_workflow_chain_runs() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello from chain").unwrap();
        let path = f.path().to_string_lossy();
        let output = print_readme_workflow(path.as_ref()).unwrap();
        assert_eq!(output, "hello from chain");
    }

    #[test]
    fn print_readme_workflow_runs_when_readme_exists() {
        let result = print_readme_workflow("../../README.md");
        if let Ok(s) = result {
            assert!(!s.is_empty());
        } else {
            panic!("README.md not found (run from repo root); got: {:?}", result);
        }
    }
}
