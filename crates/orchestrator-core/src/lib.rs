pub mod block;
pub mod core;
pub mod runtime;
pub mod workflow;

// Minimal user-facing API: Workflow, Block, BlockId, BlockOutput, BlockRegistry, RunError.
pub use block::BlockOutput;
pub use block::BlockRegistry;
pub use workflow::{Block, BlockId, RunError, Workflow};
