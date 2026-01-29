pub mod block;
pub mod core;
pub mod runtime;
pub mod workflow;

// Minimal user-facing API: Workflow, Block, BlockId, BlockOutput, RunError.
pub use block::BlockOutput;
pub use workflow::{Block, BlockId, RunError, Workflow};
