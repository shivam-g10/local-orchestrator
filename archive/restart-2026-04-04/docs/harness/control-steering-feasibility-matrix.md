# AI Harness `I2` Control Steering Feasibility Matrix (Multi-Provider)

Date: February 24, 2026  
Phase type: Research memo only (no runtime code changes)

## 1. Current repo baseline (what exists vs `I2` requirements)

### Current implemented harness surface (repo-grounded)

- The harness crate already exists at `crates/orchestrator-ai-harness` (contradicting the older roadmap baseline note in `docs/ai-harness-plan.md:13`).
- `Harness`, `Session`, `RunBuilder`, and provider registration exist in `crates/orchestrator-ai-harness/src/harness.rs:21` and `crates/orchestrator-ai-harness/src/session.rs:41`.
- Streaming runs and cancellation exist via `RunStream` + `AbortHandle` in `crates/orchestrator-ai-harness/src/run.rs:15` and `crates/orchestrator-ai-harness/src/run.rs:223`.
- Public normalized events are currently minimal (`RunStarted`, `OutputDelta`, `Completed`, `Error`) in `crates/orchestrator-ai-harness/src/stream.rs:5`.
- The only implemented provider path is OpenAI Responses over HTTP streaming in `crates/orchestrator-ai-harness/src/vendors/openai/adapter.rs:57`.

### `I2` requirements from roadmap vs current status

Source checkpoint row: `docs/ai-harness-plan.md:20` (`I2` Conversation control usability).

| `I2` requirement (`docs/ai-harness-plan.md:20`) | Current status | Gap |
|---|---|---|
| `ControlHandle` (`abort`, runtime hint injection) | `AbortHandle` only (`run.rs:15`) | Missing unified control plane and `inject_command(...)` |
| Bounded in-memory conversation history | Sessions explicitly say no history yet (`session.rs:23`) | Missing session turn storage and trimming policy |
| Streaming consumption API | Exists (`RunStream`) | Needs contract-reset migration to `RunHandle` (planned) |
| Deterministic lifecycle ordering guarantees + latency fields | Partial ordering works in practice; no explicit contract or latency metadata | Missing event envelope metadata and invariants |

### Stale roadmap note to correct in follow-on docs work

- `docs/ai-harness-plan.md:13` says the workspace has no harness crate. This is now stale because `crates/orchestrator-ai-harness` exists and tests pass locally.

## 2. Research question and non-goals

### Research question

Should `I2` include **live in-flight runtime hint injection** (`ControlHandle::inject_command(RuntimeCommand::UpdateSystemHint { ... })`) or should `I2` defer live steering and ship a **non-live control contract** first?

### Locked constraints (from prior planning decisions; not re-opened here)

- `I2` remains the target checkpoint.
- History default for follow-on implementation is **opt-in per run**.
- Public API churn is allowed (**contract reset**).
- Event API direction is **envelope event structs**.
- Crate strategy remains **reset in place** in `crates/orchestrator-ai-harness` (no crate split in next implementation slice).
- This phase is **research memo only**.
- Provider scope is **multi-provider matrix** (OpenAI, Anthropic, Gemini minimum).

### Candidate interfaces being evaluated for feasibility impact (no implementation in this phase)

- `ControlHandle` (replacing/augmenting `AbortHandle`)
- `RuntimeCommand`
  - `Abort`
  - `UpdateSystemHint { text: String }`
- `RunHandle` (contract-reset replacement for current `RunStream`)
- Event envelope model
  - `RunEventEnvelope { meta: EventMeta, kind: RunEventKind }`
  - `EventMeta` fields under evaluation: `run_id`, `trace_id`, timestamps, latency fields
- Session history controls
  - opt-in per run history inclusion

### Non-goals

- No code changes in `crates/orchestrator-ai-harness`.
- No `I3` tool-calling design or implementation.
- No `I5` memory/context implementation.
- No crate-family split (`orchestrator-ai-harness-core`, daemon crate, etc.).
- No provider SDK prototyping in this memo phase.

## 3. Provider capability matrix (OpenAI, Anthropic, Gemini minimum)

Evidence dates below are absolute access dates for this memo: **February 24, 2026**.

| Provider | Primary streaming transport(s) | Duplex client->server control during active generation (`Yes/No/Partial/Unknown`) | In-flight cancel support (`Yes/No/Partial/Unknown`) | In-flight instruction/system update support (`Yes/No/Partial/Unknown`) | Scope of steering (`session`, `response`, `future-turn-only`, `unknown`) | Documentation evidence (link) | Evidence date (absolute date) | Confidence (`High/Med/Low`) | Harness integration complexity (`Low/Med/High`) | Notes / caveats |
|---|---|---:|---:|---:|---|---|---|---|---|---|
| OpenAI | Responses API HTTP streaming (`text/event-stream` SSE) and Realtime API (`WebSocket` / `WebRTC`) | Partial | Yes | Partial | `session` + `response` on Realtime; no duplex steering on Responses SSE | [Streaming Responses guide](https://platform.openai.com/docs/guides/streaming-responses), [Realtime WebSocket guide](https://platform.openai.com/docs/guides/realtime-websocket), [Realtime `session.update`](https://platform.openai.com/docs/api-reference/realtime-client-events/session/update), [Realtime `response.cancel`](https://platform.openai.com/docs/api-reference/realtime-client-events/response/cancel), [Realtime `response.create`](https://platform.openai.com/docs/api-reference/realtime-client-events/response/create), [Background mode guide](https://platform.openai.com/docs/guides/background) | February 24, 2026 | High (transport/control existence), Med (active-response `session.update` effect) | High | Current repo adapter uses Responses SSE only (`openai/adapter.rs`), so live steering would require a new Realtime transport path. OpenAI docs explicitly document `response.cancel` and `session.update`, but active-response impact of `session.update` is not clearly specified in the cited pages. |
| Anthropic (Claude Messages API) | HTTP streaming via SSE (`text/event-stream`) | No (documented in sourced pages) | Partial | No (documented in sourced pages) | `future-turn-only` (inference from request-per-turn API shape); no in-band control channel documented in sourced streaming docs | [Streaming Messages](https://docs.claude.com/en/docs/build-with-claude/streaming), [Anthropic Python SDK helpers (official repo)](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/main/helpers.md) | February 24, 2026 | Med | High | Streaming docs describe server event types over SSE but do not document client control events. Official SDK helpers document `stream.close()` / `await ...close()` aborting the request (SDK/transport-level cancellation, not an API control event). |
| Google Gemini | REST `streamGenerateContent` (HTTP streaming endpoint) and Live API over `BidiGenerateContent` `WebSocket` | Partial | Partial | No (for live session config while connection is open) | `response` interruption via `clientContent` / activity; config changes are not allowed while connection is open (effectively future-session / future-turn) | [Generate content API (`streamGenerateContent`)](https://ai.google.dev/api/generate-content), [Live API guide](https://ai.google.dev/gemini-api/docs/live-guide), [Live API reference](https://ai.google.dev/api/live) | February 24, 2026 | High | High | Gemini Live is duplex and docs state a client `clientContent` message interrupts current generation, but the same reference states config (including setup config) cannot be updated while the connection is open. That blocks true in-flight `UpdateSystemHint` semantics as defined for `I2`. |

## 4. Evidence log with dated primary-source citations

### Documented facts (primary-source backed)

| Fact ID | Documented fact | Primary source(s) | Evidence date (absolute date) | Confidence |
|---|---|---|---|---|
| F1 | OpenAI Responses streaming uses HTTP `text/event-stream` (SSE) and emits streamed events over a server response stream. | [OpenAI Streaming Responses guide](https://platform.openai.com/docs/guides/streaming-responses) | February 24, 2026 | High |
| F2 | OpenAI background-mode docs state that synchronous response cancellation is done by terminating the connection. | [OpenAI Background mode guide](https://platform.openai.com/docs/guides/background) | February 24, 2026 | High |
| F3 | OpenAI Realtime supports bidirectional event exchange over WebSocket. | [OpenAI Realtime WebSocket guide](https://platform.openai.com/docs/guides/realtime-websocket) | February 24, 2026 | High |
| F4 | OpenAI Realtime documents `session.update` and states it can be sent at any time to update session configuration. | [Realtime `session.update` client event](https://platform.openai.com/docs/api-reference/realtime-client-events/session/update) | February 24, 2026 | High |
| F5 | OpenAI Realtime documents `response.cancel` client event to stop an in-progress response. | [Realtime `response.cancel` client event](https://platform.openai.com/docs/api-reference/realtime-client-events/response/cancel) | February 24, 2026 | High |
| F6 | OpenAI Realtime documents `response.create`, including per-response overrides (e.g., `instructions`) for the next response. | [Realtime `response.create` client event](https://platform.openai.com/docs/api-reference/realtime-client-events/response/create) | February 24, 2026 | High |
| F7 | Anthropic streaming messages docs describe streaming over SSE and enumerate server event types (`message_start`, `content_block_delta`, `message_stop`, etc.). | [Anthropic Streaming Messages](https://docs.claude.com/en/docs/build-with-claude/streaming) | February 24, 2026 | High |
| F8 | Anthropic official Python SDK helpers document stream closing/aborting (`stream.close()` / `await ...close()`) as request cancellation behavior. | [Anthropic SDK helpers (official repo)](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/main/helpers.md) | February 24, 2026 | High |
| F9 | Gemini REST supports a `streamGenerateContent` HTTP endpoint for streaming content generation. | [Google Gemini Generate content API](https://ai.google.dev/api/generate-content) | February 24, 2026 | High |
| F10 | Gemini Live API uses `BidiGenerateContent` over WebSocket and supports client/server messages on one connection. | [Gemini Live API guide](https://ai.google.dev/gemini-api/docs/live-guide), [Gemini Live API reference](https://ai.google.dev/api/live) | February 24, 2026 | High |
| F11 | Gemini Live API reference states setup configuration cannot be changed while the connection is open. | [Gemini Live API reference](https://ai.google.dev/api/live) | February 24, 2026 | High |
| F12 | Gemini Live API reference states a `clientContent` message interrupts any current model generation. | [Gemini Live API reference](https://ai.google.dev/api/live) | February 24, 2026 | High |
| F13 | Gemini Live guide documents VAD behavior that can interrupt/cancel ongoing generation when user speech starts. | [Gemini Live API guide](https://ai.google.dev/gemini-api/docs/live-guide) | February 24, 2026 | High |

### Inferences (explicitly not documented facts)

| Inference ID | Inference | Why this follows from sources + repo state | Evidence date (absolute date) | Confidence |
|---|---|---|---|---|
| I1 | OpenAI Realtime `session.update` likely cannot be assumed to mutate an already-running response deterministically for `I2` without a targeted spike. | Docs say `session.update` can be sent any time, but cited pages do not specify whether active response token generation is reconditioned mid-response. | February 24, 2026 | Med |
| I2 | Anthropic Messages streaming is unsuitable for provider-agnostic live `UpdateSystemHint` in `I2` without a different transport/product path. | Sourced docs describe SSE server events and SDK stream abort, but no documented duplex command/update protocol. | February 24, 2026 | Med |
| I3 | Gemini Live can support live interruption semantics, but not live system-hint mutation semantics as defined for `UpdateSystemHint`. | Live API provides interruption (`clientContent`, VAD) but disallows config updates while connection is open. | February 24, 2026 | High |
| I4 | A provider-agnostic `ControlHandle` should separate `Abort` from `UpdateSystemHint` capability flags/events because provider support diverges materially. | OpenAI Realtime supports explicit cancel; Anthropic sourced docs only show SDK stream close; Gemini shows interruption semantics and config immutability. | February 24, 2026 | High |

## 5. Feasibility analysis for three harness branches

### Research validation scenarios (evidence-backed outcomes)

| Scenario | Outcome | Evidence | Confidence |
|---|---|---|---|
| OpenAI Responses HTTP+SSE: Can client send steering commands mid-stream? | No on the current Responses SSE path (one request, server stream only). | [Streaming Responses guide](https://platform.openai.com/docs/guides/streaming-responses), plus current adapter design in `crates/orchestrator-ai-harness/src/vendors/openai/adapter.rs:57` | High |
| OpenAI Responses HTTP+SSE: How does cancellation work for synchronous streaming responses? | Cancel by terminating the connection (transport-level). | [OpenAI Background mode guide](https://platform.openai.com/docs/guides/background) | High |
| OpenAI Realtime: `session.update` semantics | Documented client event, can be sent any time, updates provided fields on session configuration. | [Realtime `session.update`](https://platform.openai.com/docs/api-reference/realtime-client-events/session/update) | High |
| OpenAI Realtime: `response.cancel` semantics | Documented client event to stop an in-progress response. | [Realtime `response.cancel`](https://platform.openai.com/docs/api-reference/realtime-client-events/response/cancel) | High |
| OpenAI Realtime: Do `session.update` changes affect active response vs subsequent responses? | Unclear in cited docs; requires prototype verification. | `session.update` + `response.create` docs (no explicit active-response guarantee found) | Low |
| Anthropic path: Duplex/control channel for active-generation updates? | No documented duplex client-event channel in sourced Messages streaming docs. | [Anthropic Streaming Messages](https://docs.claude.com/en/docs/build-with-claude/streaming) | Med |
| Anthropic path: Cancel/interrupt semantics | SDK-level request abort via stream close is documented; provider in-band cancel event not found in sourced docs. | [Anthropic SDK helpers](https://raw.githubusercontent.com/anthropics/anthropic-sdk-python/main/helpers.md) | Med |
| Gemini path: Duplex/control channel for active generation | Yes on Live API (WebSocket), no on REST `streamGenerateContent`. | [Gemini Live API guide](https://ai.google.dev/gemini-api/docs/live-guide), [Gemini Live API reference](https://ai.google.dev/api/live), [Generate content API](https://ai.google.dev/api/generate-content) | High |
| Gemini path: Cancel/interrupt semantics | Partial: interruption is documented (`clientContent`, VAD), but no dedicated generic `cancel` event is confirmed in sourced docs. | [Gemini Live API reference](https://ai.google.dev/api/live), [Gemini Live API guide](https://ai.google.dev/gemini-api/docs/live-guide) | High |
| Gemini path: In-flight system/instruction update semantics | No for live open connection config updates (docs explicitly disallow config changes while connection open). | [Gemini Live API reference](https://ai.google.dev/api/live) | High |
| API migration fit: Can envelope events represent provider control acks/errors? | Yes, with provider-specific control event kinds and capability flags; no blocker found. | Inference from provider divergence + planned contract-reset direction | Med |

### Branch A: Non-live `I2`

Definition:
- `ControlHandle` exists.
- `abort` works.
- `inject_command(UpdateSystemHint)` returns typed unsupported/deferred on current providers.

Feasibility summary:
- **Technically straightforward** on current repo because `AbortHandle` already exists (`run.rs:15`) and can be lifted into a new `ControlHandle`.
- Avoids provider transport expansion and keeps `I2` bounded to control-plane API scaffolding + history + event envelopes.
- Risk: user-visible `UpdateSystemHint` command exists but provides no functional steering on supported providers, which weakens the checkpoint’s product value.

Impact on planned `I2` interfaces:
- `ControlHandle`: easy to introduce with capability-aware erroring.
- `RuntimeCommand::Abort`: maps cleanly to current watch channel cancel path.
- `RuntimeCommand::UpdateSystemHint`: becomes a typed unsupported path.
- `RunHandle` + event envelopes: unaffected (still a good fit).
- Session history (opt-in per run): still required for conversation usability.

### Branch B: Next-turn soft steering `I2`

Definition:
- `ControlHandle` accepts hint commands.
- Hints are stored in harness/session state and applied to future turns/history assembly only.
- No in-flight provider mutation requirement.

Feasibility summary:
- **Best balance** of immediate user value and provider portability.
- Works with the already chosen `I2` history default (opt-in per run) and does not require a WebSocket provider path.
- Cleanly decouples `Abort` (transport/runtime concern) from `UpdateSystemHint` (conversation-state concern).
- Still preserves a future upgrade path to live steering (`Branch C`) by keeping `ControlHandle` and event envelopes provider-capability-aware.

Impact on planned `I2` interfaces:
- `ControlHandle`: supports both `Abort` and accepted `UpdateSystemHint`.
- `RuntimeCommand::UpdateSystemHint`: deterministic semantics in `I2` = queued for next turn only.
- `RunHandle`: can emit steering-related envelope events like `CommandAccepted`, `SteeringQueued`, `SteeringAppliedNextTurn`.
- `RunEventEnvelope` / `EventMeta`: can carry timestamps and trace IDs for control-plane actions and future provider acks.
- Session history (opt-in): becomes a meaningful prerequisite for hint application semantics.

### Branch C: Live steering `I2`

Definition:
- `ControlHandle` supports in-flight `UpdateSystemHint` for at least one provider path.
- Requires documented provider control protocol and deterministic runtime semantics.

Feasibility summary:
- **Not decision-safe for immediate `I2`** based on current evidence.
- OpenAI Realtime provides promising primitives (`session.update`, `response.cancel`), but active-response effect for `session.update` is not clearly documented in the cited pages.
- Gemini Live provides live interruption, but explicitly blocks config updates while connection is open.
- Anthropic sourced docs do not show a duplex control channel.
- Would force substantial provider-specific capability branching and new transport/runtime architecture before `I2` conversation usability basics land.

Impact on planned `I2` interfaces:
- `ControlHandle` would need provider capability negotiation and richer error/ack semantics immediately.
- `RuntimeCommand::UpdateSystemHint` semantics become provider-dependent unless narrowed.
- `RunHandle`/event envelopes would need provider control-ack and rejected-command events from day one.
- Highest chance of `I2` scope spill into `I4`-style reliability and negotiation work.

## 6. Decision rubric and scored recommendation

Scoring rule used in this memo: **5 = most favorable** (lowest risk / highest value / clearest path), **1 = least favorable**.

### Branch scores

| Criterion | Branch A: Non-live `I2` | Branch B: Next-turn soft steering `I2` | Branch C: Live steering `I2` |
|---|---:|---:|---:|
| Delivery risk for next `I2` slice | 5 | 4 | 1 |
| User value in the next checkpoint | 2 | 4 | 5 |
| Provider dependence / portability risk | 5 | 5 | 1 |
| Runtime complexity added to current harness | 5 | 4 | 1 |
| API clarity for contract-reset path | 3 | 5 | 2 |
| Testability in current repo | 5 | 5 | 2 |
| Alignment with future `I3` / `I4` / `I5` | 3 | 5 | 3 |
| **Total** | **28** | **32** | **15** |

### Mandatory decision rule application (explicit)

1. Rule 1 (`Branch A` if no provider has documented testable in-flight steering semantics suitable for the harness runtime): **Not strictly triggered**, because OpenAI Realtime documents control events and Gemini Live documents live interruption semantics.
2. Rule 2 (`Branch B` if at least one provider supports documented command/update events but semantics are session-scoped or future-turn-only): **Triggered**.
   - OpenAI Realtime documents `session.update`, but active-response effect is unclear in the cited docs.
   - Gemini Live explicitly disallows config changes while the connection is open.
3. Rule 3 (`Branch C` only if deterministic in-flight control semantics and failure behavior can be specified without unresolved critical unknowns): **Not satisfied**.
4. Rule 4 (if key provider behavior remains unknown, choose `A` or `B` and defer `C`): **Triggered** because OpenAI active-response `session.update` effect remains unverified.

### Scored recommendation

Recommend **Branch B: Next-turn soft steering `I2`**.

Rationale:
- Highest rubric score.
- Satisfies the mandatory decision rules.
- Delivers real user value in `I2` without blocking on provider-specific realtime semantics.
- Keeps a clean upgrade path to `Branch C` after a targeted provider prototype/spike.

## 7. Explicit next-step branch for `I2` planning

### Chosen branch for the next planning phase

- **Next planning branch:** `Branch B: Next-turn soft steering I2`

### Deferred branch (explicit)

- **Deferred branch:** `Branch C: Live steering I2` (defer pending targeted provider prototype research, starting with OpenAI Realtime active-response `session.update` semantics)

### What the next `I2` implementation plan should cover (decision-complete target)

- Contract-reset migration from `RunStream` to `RunHandle` (in-place within `crates/orchestrator-ai-harness`)
- `ControlHandle` introduction with:
  - `Abort`
  - `UpdateSystemHint { text: String }` (queued, next-turn semantics)
- Bounded in-memory session history (opt-in per run inclusion)
- Event envelope structs + `EventMeta` (`run_id`, `trace_id`, timestamps, latency fields)
- Event ordering guarantees and tests
- Provider capability/error signaling for unsupported live steering (documented as deferred, not implemented in `I2`)

## 8. Unknowns and follow-up probes (if any)

### Unknowns requiring prototype verification

- **OpenAI Realtime `session.update` active-response effect:** The cited docs confirm the event exists and can be sent any time, but do not clearly specify whether it deterministically affects an already in-progress response.
- **OpenAI Realtime ordering semantics for mixed control + output events:** The cited docs do not provide enough detail to define a provider-agnostic ordering contract for `I2` live steering.
- **Anthropic alternative realtime/control surfaces (if any) beyond sourced Messages streaming docs:** Not established in this memo because research scope used the sourced streaming docs plus official SDK helper cancellation behavior.
- **Gemini dedicated cancel event semantics (vs interruption semantics):** Sourced docs clearly support interruption, but a dedicated generic cancel event was not confirmed here.

### Follow-up probes (deferred to future research/spike; not part of this memo phase)

1. Timeboxed OpenAI Realtime prototype:
   - Verify whether `session.update` changes affect active generation vs subsequent `response.create`.
   - Capture event ordering for `session.update`, `response.cancel`, and output deltas.
2. Provider capability flag design spike:
   - Model separate capabilities for `abort`, `interrupt`, and `live_system_hint_update`.
3. Anthropic/Gemini follow-up doc sweep:
   - Re-check for newly documented live control or cancel semantics before revisiting `Branch C`.

Next planning target: **Plan `I2` Branch B implementation (next-turn soft steering) in `crates/orchestrator-ai-harness` with contract-reset event envelopes and opt-in per-run history.**
