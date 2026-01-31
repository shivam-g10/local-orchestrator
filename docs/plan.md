---
name: Local Orchestrator Roadmap
overview: Build an async-first local workflow engine with strong typing, a block SDK, and phased demos; UI comes later.
todos:
  - id: core-graph-runtime
    content: Async graph core + typed IO + builder
    status: pending
  - id: block-sdk-samples
    content: Block SDK + example workflows + runner
    status: pending
  - id: eventing-logging
    content: Event bus + structured logs + subscribers
    status: pending
  - id: robustness
    content: Retries, timeouts, idempotency, pause/resume
    status: pending
  - id: persistence-yaml
    content: SQLite store + YAML import/export + versions
    status: pending
  - id: ai-harness
    content: Provider abstraction, usage tracking, evals
    status: pending
  - id: undo
    content: Undo system for blocks and runs
    status: pending
  - id: daemon-api
    content: Optional daemon API for headless use
    status: pending
  - id: ui-editor
    content: Next.js workflow editor + YAML UI
    status: pending
  - id: tauri-monitoring
    content: Tauri desktop + monitoring views
    status: pending
  - id: delivery-testing-perf
    content: Packaging + testing + performance controls
    status: pending
  - id: demo-workflows
    content: Maintain real-world demos per phase
    status: pending
isProject: false
---

# Local Orchestrator Roadmap

## Target architecture (async + daemon + embedded)

- Async Tokio runtime with a queue-based scheduler, worker pools, parallel links, and cycle-aware iteration limits.
- Dual integration: local daemon API for headless use + embedded library mode for desktop (selected).
- Event bus for run/block lifecycle events feeding structured logs, UI, and external subscribers.
- Local persistence with SQLite (default) plus YAML import/export for versioned workflows.

```mermaid
flowchart LR
  UIWeb["UI_Web"] -->|API| Daemon["Local_Daemon"]
  DesktopTauri["Desktop_Tauri"] -->|embed_or_rpc| Daemon
  Daemon --> Engine["Workflow_Engine"]
  Engine --> Blocks["Block_Executors"]
  Engine --> Store["Local_Store"]
  Engine --> Events["Event_Bus"]
  Events --> Logs["Log_Sink"]
```

## Demonstration rule

- Each phase ends with a working demo that can be run locally.
- Demos live as sample workflows in `crates/orchestrator-examples/`, with a minimal run command or UI path documented.

## User-facing API focus

- Users build workflows via `Workflow::new()`, `add(Block)`, `link(BlockId, BlockId)`, and `run()`.
- Blocks are created with strong configs; optional config (like file path) can be provided now or supplied at run time via input.
- The public surface stays at this level; `WorkflowDefinition`, `WorkflowRun`, `BlockRegistry`, and `runtime` remain internal.
- The user focus is on blocks, their configuration, and linking them into a workflow.

## Phase 1: Async core + typed IO (Rust library)

- Define `WorkflowDefinition` (nodes, edges; ports/conditions deferred) and `WorkflowRun` state machine in [crates/orchestrator-core/src/](crates/orchestrator-core/src/).
- Replace `Option<String>` IO with typed `BlockInput`/`BlockOutput` enums (serde-able, versioned) in [crates/orchestrator-core/src/block/](crates/orchestrator-core/src/block/).
- Implement async scheduler with parallel edges, multiple-next execution, cycle handling (iteration budget + run tokens), and concurrency controls.
- Build internal definition/builder and block registry while exposing a minimal build-and-run API (`Workflow`, `Block`, `BlockId`).
- Demo: parallel + cyclic workflow executed via `cargo run`, no UI required.

**Phase 1 follow-up (after Block SDK):** (1) Add cyclic workflow demo to `orchestrator-examples`. (2) Resolve multi-sink output: implement “last link” sink rule per decisions below, or document “first sink by Uuid” as current behavior. (3) Update any remaining plan/docs paths from `backend/` to `crates/orchestrator-core/`.

### Phase 1: Workflow output resolution (decisions)

How the runtime chooses which block’s output to return as the workflow result (priority order):

1. **User-designated output block** (future): If the workflow has `output_block: Option<Uuid>` set (e.g. via `set_output_block(BlockId)`), return that block’s output. The designated block need not be a sink.
2. **Single sink**: If there is exactly one block with no outgoing edges, return that block’s output.
3. **Last link**: If there are multiple sinks and no designated output, use the sink that is the destination of the last link (last edge’s `to`). If that node is not a sink, fall back to the first sink by sorted Uuid (deterministic).
4. **Last executed (implemented)**: If none of the above yield an available output (e.g. in iteration mode the primary sink never ran), return the output of the **last block to complete execution**. The runtime tracks `last_completed_id` in both DAG execution (`run_workflow`) and iteration execution (`run_workflow_iteration`); when resolving the result it prefers the primary sink’s output, then falls back to `last_completed_id`’s output.

## Phase 2: Block SDK + sample runner

- Create a simple block SDK: base trait, input/output helpers, error types, and block template docs.
- Add a minimal sample runner/CLI for `cargo run` to execute sample workflows using the `Workflow`/`Block` API.
- Demo: add a new custom block in <30 minutes and run a sample workflow using it.

## Phase 3: AI harness upgrades

- Add provider abstraction and model config; move OpenAI specifics behind a trait (see `poc/src/block/ai/` for reference).
- Add token usage tracking, cost budgets, eval mode, and tool-provider switching.
- Allow external subscriptions to AI events (prompt, response, tool call, cost).
- Demo: AI workflow that switches models and respects a cost budget.

## Phase 4: Eventing + logging

- Introduce `RunEvent`/`BlockEvent` bus with subscriptions; wire into engine and blocks.
- Expand logger to structured logs (see `poc/src/logger.rs` for reference) (JSON + file rotation) and link to the event bus.
- Add diagnostics helpers and trace correlation IDs per run.
- Demo: stream events to console + log file and show per-block timings.

## Phase 5: Robust runs

- Add retry policies, timeouts, failure classification, and idempotency keys.
- Implement pause/resume/cancel and partial restart (by block or checkpoint).
- Add run metrics and failure reporting.
- Demo: a flaky block retries, then a paused run resumes from last checkpoint.

## Phase 6: Persistence + YAML

- Add local store (SQLite) for workflows, versions, runs, and logs; include migrations.
- YAML import/export for versioned workflows and block configs.
- Point-in-time restart from persisted run checkpoints.
- Demo: import YAML, run it, then list run history and replay from a checkpoint.

## Phase 7: Undo system

- Define `UndoAction` per block and record side effects at run time.
- Implement run-level undo and undo-to-block for a run.
- Demo: file write workflow undone to the previous state.

## Phase 8: Daemon API (optional)

- Implement local daemon API for workflow CRUD, run control, logs, and replay.
- Keep embedded API for Tauri via feature flags to reuse the same core engine.
- Demo: start daemon, run a workflow via CLI, and tail live events.

## Phase 9: Web UI (Next.js static)

- Build a workflow canvas editor (React Flow), block library, detail editor.
- Load/save YAML; validate and visualize graph; download YAML.
- Provide a minimal "run locally" panel for debugging against the daemon.
- Demo: create a workflow in UI, run it locally, and view live logs.

## Phase 10: Desktop (Tauri) + monitoring

- Integrate the web UI in Tauri and connect to daemon or embedded mode.
- Add workflow list and run controls (play/pause/resume/stop).
- Add monitoring: run logs, block-level logs, replay.
- Demo: desktop app runs a workflow and shows a block timeline.

## Phase 11: Delivery + testing + performance

- Package installers and improve onboarding across OSs.
- Unit/integration tests for engine, blocks, and storage; E2E tests for UI.
- Performance controls (workers, max threads, queue sizes) with safe defaults.
- Demo: install, run, and benchmark with tuned concurrency settings.

## Cross-cutting quality rules

- Prefer strong types over ad-hoc strings or `serde_json::Value`.
- Keep modules small and cohesive; split large files early.
- Separate conceptual concerns at high levels (core, blocks, runtime, storage).
- Favor conventions and simple patterns over heavy abstractions.
- Prefer explicit, copyable boilerplate over complex generic type tricks.
- Do more with less: prefer fewer lines when clarity is equal.
- Prioritize ergonomics and ease of use in the public API.
- Use clear, unambiguous names for types, modules, and workflows.
- Add unit tests where behavior is non-trivial or regressions are likely.
- Treat demo workflows as first-class artifacts and keep them runnable.
