//! Example: parallel copy — for each (src, dst) pair, file_read(src) -> file_write(dst).
//! Entry (echo) fans out to all reads so they use their config paths; each read feeds one write.
//! Read–write chains at the same level run in parallel.
//!
//! ```text
//!   [Echo] --> [FileRead src1] --> [FileWrite dst1]
//!   [Echo] --> [FileRead src2] --> [FileWrite dst2]
//!   [Echo] --> [FileRead srcN] --> [FileWrite dstN]
//! ```

use orchestrator_core::{Block, RunError, Workflow};

/// Copy files in parallel: for each (src_path, dst_path) pair, read src and write to dst.
/// Returns Ok(()) on success; the workflow result is the last sink output (often empty from write).
#[allow(dead_code)]
pub fn copy_files_workflow(pairs: &[(&str, &str)]) -> Result<(), RunError> {
    if pairs.is_empty() {
        return Ok(());
    }

    let mut w = Workflow::new();
    let entry_id = w.add(Block::echo());

    for (src, dst) in pairs {
        let read_id = w.add(Block::file_read(Some(*src)));
        let write_id = w.add(Block::file_write(Some(*dst)));
        w.link(entry_id, read_id);
        w.link(read_id, write_id);
    }

    w.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn copy_files_workflow_copies_one_file() {
        let mut src = NamedTempFile::new().unwrap();
        write!(src, "content to copy").unwrap();
        let src_path = src.path().to_string_lossy().to_string();
        let dir = tempfile::tempdir().unwrap();
        let dst_path = dir.path().join("copied.txt");

        copy_files_workflow(&[(src_path.as_str(), dst_path.to_str().unwrap())]).unwrap();
        assert_eq!(std::fs::read_to_string(&dst_path).unwrap(), "content to copy");
    }

    #[test]
    fn copy_files_workflow_copies_multiple_in_parallel() {
        let mut a = NamedTempFile::new().unwrap();
        let mut b = NamedTempFile::new().unwrap();
        write!(a, "from_a").unwrap();
        write!(b, "from_b").unwrap();
        let ap = a.path().to_string_lossy().to_string();
        let bp = b.path().to_string_lossy().to_string();
        let dir = tempfile::tempdir().unwrap();
        let dst_a = dir.path().join("out_a.txt");
        let dst_b = dir.path().join("out_b.txt");

        copy_files_workflow(&[
            (ap.as_str(), dst_a.to_str().unwrap()),
            (bp.as_str(), dst_b.to_str().unwrap()),
        ])
        .unwrap();

        assert_eq!(std::fs::read_to_string(&dst_a).unwrap(), "from_a");
        assert_eq!(std::fs::read_to_string(&dst_b).unwrap(), "from_b");
    }

    #[test]
    fn copy_files_workflow_empty_pairs_succeeds() {
        copy_files_workflow(&[]).unwrap();
    }
}
