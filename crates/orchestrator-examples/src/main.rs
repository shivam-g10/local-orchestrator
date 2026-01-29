use orchestrator_core::{Block, BlockOutput, RunError, Workflow};

fn main() -> Result<(), RunError> {
    let mut w = Workflow::new();
    let _read = w.add(Block::file_read(Some("README.md")));
    let output = w.run()?;
    if let BlockOutput::String { value } = output {
        println!("{}", value);
    }
    Ok(())
}
