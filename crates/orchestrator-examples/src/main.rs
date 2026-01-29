//! Run workflow examples from the workflows module (one example per file).

mod workflows;

use crate::workflows::{
    copy_files_workflow, print_readme_workflow, single_file_read_workflow,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Example: single_file_read (README.md) ===\n");
    let content = single_file_read_workflow("README.md")?;
    println!("{}\n", content);

    println!("=== Example: print_readme (file_read -> echo) ===\n");
    let content = print_readme_workflow("README.md")?;
    println!("{}\n", content);

    println!("=== Example: copy_files (parallel read -> write chains) ===\n");
    let out_dir = std::path::PathBuf::from("./out");
    copy_files_workflow(&[
        ("Cargo.toml", out_dir.join("Cargo.toml").to_str().unwrap()),
        ("LICENSE", out_dir.join("LICENSE").to_str().unwrap()),
    ])?;
    println!("Copied Cargo.toml and LICENSE to {:?}\n", out_dir);

    Ok(())
}
