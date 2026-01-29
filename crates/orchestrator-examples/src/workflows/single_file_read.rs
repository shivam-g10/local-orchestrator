//! Example: single file_read block (minimal workflow).

use orchestrator_core::{Block, RunError, Workflow};

/// Build and run a workflow that reads a single file. Returns the file contents or an error.
pub fn single_file_read_workflow(path: &str) -> Result<String, RunError> {
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
    fn single_file_read_workflow_runs() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello from single block").unwrap();
        let path = f.path().to_string_lossy();
        let result = single_file_read_workflow(&path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello from single block");
    }
}
