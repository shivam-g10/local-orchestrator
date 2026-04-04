# Vision

## Product vision

Build a local, founder-first automation assistant that can turn natural-language requirements into reliable on-device workflows with human control points, clear auditability, and iterative improvement.

## Target user

- Solo founder operating without a dedicated ops or engineering team.

## Core outcomes

1. Founder explains a business need in chat.
2. Agent converts the need into a draft workflow (or grouped workflows).
3. Founder reviews, edits, and approves execution.
4. System runs reliably on triggers/schedules.
5. Every run is explainable, steerable, and auditable.

## Representative use cases

1. PLG CRM automation on Excel/Google Sheets.
2. LinkedIn post generation and revision.
3. Market research automation.
4. Newsletter research with human-in-the-loop approval and edits.
5. Meeting preparation and follow-up automation.

## Product principles

- Local-first execution by default.
- Human approval for high-impact actions.
- Clear state, logs, and replayability.
- Small, reversible product increments.
- Minimal operator burden for the founder.

## Non-goals for pre-1.0

- Broad enterprise multi-tenant platform scope.
- Maximum connector breadth over reliability.
- Fully autonomous high-risk outbound automation without approvals.

## Delivery model

Feature work is managed only through:

1. PRD
2. Review
3. Tests
4. Implementation
5. QA
6. Ship

Each stage must produce an artifact and pass gates before the next stage.

## Pre-1.0 release policy

- Versioning remains `0.x.y` until the first fully functioning product is complete.
- Every PRD must be the smallest viable improvement.
- Every PRD must include:
  - a design doc in `docs/pm/designs/`
  - a flowchart doc in `docs/pm/flowcharts/`

## Exit criteria for 1.0.0

Move to `1.0.0` only when all are true:

1. End-to-end founder workflow works from chat intake to approved execution and run reporting.
2. Core reliability safeguards are in place (retries, idempotency/dedupe, audit logs).
3. Human-in-the-loop approvals are working for high-impact actions.
4. Product supports at least one complete, repeatable real-world workflow with documented QA evidence.
