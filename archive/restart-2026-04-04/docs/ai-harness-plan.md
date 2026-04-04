# Expanded AI Harness + Platform Rollout (Checkpoint-Based, Events First)

## Summary
1. This plan expands beyond provider/control/tools and explicitly includes events, observability, memory/context, reliability, eval/safety, persistence, daemon APIs, web monitoring, and desktop packaging.
2. Eventing and observability are mandatory from the first usable checkpoint.
3. The harness remains independent from block/workflow systems; platform integration is layered on top, not coupled into core harness semantics.
4. Roadmap format is checkpoint-based (`I1`-`I10`) with hard usability gates at each checkpoint.

## Grounded Baseline (Current Repo State)
1. Existing core observability already exists in `/Users/shivam/Documents/personal-code/local-orchestration/crates/orchestrator-core/src/observability.rs` (env-gated tracing + JSONL/console output).
2. Current roadmap in `/Users/shivam/Documents/personal-code/local-orchestration/docs/plan.md` already includes eventing/logging, robustness, persistence, daemon, UI, desktop phases.
3. Harness contract in `/Users/shivam/Documents/personal-code/local-orchestration/docs/ai-harness-api-contract.md` already defines stream/event/Xray concepts but is monolithic and not yet implemented as crates.
4. Workspace currently has no harness crate in `/Users/shivam/Documents/personal-code/local-orchestration/Cargo.toml`.

## Checkpoint Roadmap (`I1`-`I10`)

| Checkpoint | Scope | Mandatory API/Interface Additions | Observability/Event Requirement | Exit Gate |
|---|---|---|---|---|
| `I1` | Harness core bootstrap + provider foundation | `HarnessBuilder`, `Harness`, `SessionConfig`, `Session`, `RunBuilder`, `RunHandle`, `ProviderAdapter`, `ProviderId`, `ModelRef`, `ModelPlan` in new crate `crates/orchestrator-ai-harness-core` | `StreamEvent::{RunStarted,OutputDelta,Completed,Error}` + `run_id`/`trace_id` correlation in all events | Runnable first-response example + 15-min quickstart |
| `I2` | Conversation control usability | `ControlHandle` (`abort`, runtime hint injection), bounded in-memory conversation history, streaming consumption API | Deterministic lifecycle ordering guarantees and per-run latency fields | Runnable multi-turn streaming example + abort demo + event-order tests |
| `I3` | Serial strict tool calling | `Tool`, `ToolSpec`, `ToolRegistry`, `ToolResult`, schema-validation pipeline, serial tool loop | `StreamEvent::{ToolCallRequested,ToolCallFinished}` with tool call IDs | Runnable one-tool success + one invalid-args flow + typed errors |
| `I4` | Reliability + hardening | Retry/timeout policies, provider error classification, minimal negotiation constraints in `ModelPlan` | Retry/timeout events and failure taxonomy (`provider`, `runtime`, `tool`) | Reliability scenario demo (timeout + retry + recover) + conformance tests |
| `I5` | Memory + context (next priority) | `ContextPolicy`, memory lane traits (`working`, `episodic` interface; in-memory impl first), context assembly pipeline | Context-trim and memory-retrieval events with token/cost counters | Runnable continuity example across turns + bounded-context behavior tests |
| `I6` | Evals + safety controls | Eval interfaces, shadow/enforcing gate interfaces, safety policy interfaces (no external MCP dependency yet) | Gate decision events + redaction/filtered-trace sink path | Shadow+enforcing evaluation demo with policy pass/fail scenarios |
| `I7` | Persistence + replay + undo | Persistent run/session/checkpoint store interfaces, replay token contract, undo/compensation contract | Immutable run journal + replay/undo event streams | Replay/undo demo from persisted run + consistency invariants |
| `I8` | Daemon/API layer | New daemon crate (`crates/orchestrator-ai-harness-daemon`) with run/session CRUD and stream endpoints | Live event streaming API (`SSE`/stream transport) and queryable run traces | Start daemon, run harness task remotely, tail live events/logs |
| `I9` | Web monitoring UX | Web monitoring surface for run list, event timeline, tool traces, replay/undo actions | End-to-end trace visualization sourced from daemon events | Create/run/inspect/replay flow in web UI |
| `I10` | Desktop + delivery/perf | Desktop integration, packaging scripts, performance controls and benchmarks | Production-like telemetry profile validation under load | Installable build + benchmark report + stability checklist |

## Capability Track Mapping (So “Other Parts” Are Explicit)
1. Track A: Provider + conversation + tools (`I1`-`I4`).
2. Track B: Memory/context + eval/safety (`I5`-`I6`).
3. Track C: Persistence/replay/undo (`I7`).
4. Track D: Daemon/platform interfaces (`I8`).
5. Track E: Monitoring UI + desktop + delivery (`I9`-`I10`).

## Important Public API and Type Contract Changes
1. New core crate: `/Users/shivam/Documents/personal-code/local-orchestration/crates/orchestrator-ai-harness-core`.
2. New core modules required in `lib.rs`: `harness`, `provider`, `model`, `session`, `run`, `stream`, `tools`, `errors`, `context`, `memory`, `reliability`, `journal`, `replay`, `compensation`.
3. Event contract required from `I1` onward:
   - `RunStarted`
   - `OutputDelta`
   - `Completed`
   - `Error`
   - `ToolCallRequested` (from `I3`)
   - `ToolCallFinished` (from `I3`)
4. Error taxonomy contract required by `I4`:
   - `ProviderError`
   - `RuntimeError`
   - `ToolError`
   - Typed mapping rules and retryability flags.
5. Persistence/replay/undo contracts become public at `I7`.
6. Daemon API contracts become public at `I8` (run control + event stream + trace query).

## Contract/Doc Phase-Out Execution
1. Keep `/Users/shivam/Documents/personal-code/local-orchestration/docs/ai-harness-api-contract.md` as top-level index and migration map only.
2. Create split docs:
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/overview.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/provider-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/control-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/tools-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/observability-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/memory-context-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/reliability-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/persistence-replay-undo-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/platform-daemon-ui-track.md`
   - `/Users/shivam/Documents/personal-code/local-orchestration/docs/harness/deferred-capabilities.md`
3. Language gate: no block/workflow-coupled terminology in new harness docs.

## Test Cases and Scenarios
1. Unit tests:
   - Provider request/response mapping.
   - Session ordering and bounded history trimming.
   - Tool schema validation pass/fail.
   - Abort cancellation propagation.
   - Retry/timeout classification.
2. Integration tests:
   - OpenAI adapter smoke test (env-gated).
   - Multi-turn stream ordering and lifecycle correctness.
   - Serial tool call success + invalid args failure.
   - Memory/context assembly behavior across turns.
   - Replay and undo on persisted runs.
3. Contract tests:
   - Fake provider adapter conformance suite.
   - Stream event invariant suite (ordering, required fields).
   - Error category invariants.
4. Platform E2E tests:
   - Daemon remote run + event stream subscription.
   - Web monitoring timeline fidelity.
   - Desktop run-control and trace visualization.
5. Performance tests:
   - Stream-to-first-token latency.
   - Event throughput under concurrent runs.
   - Memory ceiling and backpressure behavior.

## Rollout and Compatibility Rules
1. Existing block-based AI paths remain untouched during `I1`-`I10`.
2. No block adapter is required for checkpoint completion; any adapter decision happens only after `I10` stabilization review.
3. Versioning cadence:
   - `0.1.x` for `I1`-`I4`
   - `0.2.x` for `I5`-`I7`
   - `0.3.x` for `I8`-`I10`
4. Every checkpoint must include:
   - one runnable example,
   - one quickstart validated to complete in ~15 minutes.

## Assumptions and Defaults
1. OpenAI Responses remains the first concrete provider adapter.
2. Async-first runtime (`tokio`) is non-negotiable from `I1`.
3. Single core crate starts first; supporting crates (daemon/UI) are added at designated checkpoints.
4. Eventing/observability is foundational, not deferred.
5. Memory/context is prioritized immediately after provider/control/tools.
6. End-to-end platform coverage is required in this roadmap, not just harness internals.
