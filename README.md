# Light weight workflow orchestration for local system

Currently PoC.

**Quality:** Node and block config use strong types only (`BlockConfig`, `FileReadConfig`); no `serde_json::Value` or ad-hoc string keys in the public API (per [docs/plan.md](docs/plan.md) cross-cutting quality rules).

## Workspace layout

- **crates/orchestrator-core** — library: workflow definition, run state, typed block IO, file-read block, minimal sync runner.
- **crates/orchestrator-examples** — binary that uses orchestrator-core to run a real workflow (read file from disk).
- **backend/** — existing crate (unchanged).

## Foundation demo

Run the “Read file from disk” workflow (one block reads `README.md` and prints its contents):

```bash
cargo run -p orchestrator-examples
```

## Testing

Run all workspace tests:

```bash
cargo test --workspace
```

Run only orchestrator-core unit tests:

```bash
cargo test -p orchestrator-core
```

## Linting (Clippy)

Run Clippy with warnings as errors on the orchestrator crates:

```bash
cargo clippy -p orchestrator-core -p orchestrator-examples -- -D warnings
```