# Block SDK template: add a custom block in under 30 minutes

This guide shows how to implement a custom block, register it, and run it in a workflow using the orchestrator-core API.

## 1. Implement the block

Your block must implement the `BlockExecutor` trait and work with `BlockInput` / `BlockOutput`.

- **BlockExecutor:** `fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError>`
- **BlockInput:** today `Empty` or `String(s)`. Use `BlockInput::empty()` when there is no upstream, or match on the input to read upstream output.
- **BlockOutput:** `Empty` or `String { value }`. Return `BlockOutput::empty()` or `BlockOutput::String { value: s }`.

Example (uppercase block):

```rust
use orchestrator_core::block::{BlockExecutor, BlockInput, BlockError, BlockOutput};

struct UppercaseBlock {
    prefix: String,
}

impl BlockExecutor for UppercaseBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let s = match &input {
            BlockInput::String(t) => t.to_uppercase(),
            BlockInput::Empty => String::new(),
        };
        Ok(BlockOutput::String {
            value: format!("{}{}", self.prefix, s),
        })
    }
}
```

The block must be `Send + Sync` (use only `Send + Sync` types).

## 2. Define config (typed + Serialize)

Use a typed config struct and derive `Serialize` so it can be passed to `add_custom`. The registry receives the config as `serde_json::Value`; your factory deserializes or reads fields from it.

```rust
use serde::Serialize;

#[derive(Serialize)]
struct UppercaseConfig {
    prefix: String,
}
```

## 3. Register the custom block

Create a `BlockRegistry` (e.g. `BlockRegistry::default_with_builtins()`) and register your block type:

```rust
use orchestrator_core::BlockRegistry;

let mut registry = BlockRegistry::default_with_builtins();
registry.register_custom("uppercase", |payload: serde_json::Value| {
    let prefix = payload
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Box::new(UppercaseBlock { prefix }))
});
```

Use a stable `type_id` string (e.g. `"uppercase"`); the same string is used when adding the block to a workflow.

## 4. Build a workflow and add the block

Create a workflow with `Workflow::with_registry(registry)`, then add built-in blocks with `add` and custom blocks with `add_custom`:

```rust
use orchestrator_core::{Workflow, Block};

let mut w = Workflow::with_registry(registry);
let read_id = w.add(Block::file_read(Some("/path/to/file.txt")));
let upper_id = w.add_custom("uppercase", UppercaseConfig { prefix: ">> ".to_string() })?;
w.link(read_id, upper_id);

let output = w.run()?;
```

## 5. Run the workflow

Call `w.run()` (sync) or `w.run_async().await` (async). The result is `Result<BlockOutput, RunError>`. Convert `BlockOutput` to `Option<String>` with `output.into()` when using the string variant.

## Checklist

- [ ] Struct implementing `BlockExecutor` (execute with `BlockInput` -> `Result<BlockOutput, BlockError>`).
- [ ] Config struct with `Serialize`; factory that builds the block from `serde_json::Value`.
- [ ] `register_custom(type_id, factory)` on the registry.
- [ ] `Workflow::with_registry(registry)` and `add_custom(type_id, config)`.
- [ ] `link(from, to)` to connect blocks; `run()` or `run_async().await`.

For more examples, see `crates/orchestrator-examples/src/workflows/` (e.g. expense_report, stock_report) and the tests in `crates/orchestrator-core/src/workflow.rs`.
