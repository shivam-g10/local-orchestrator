# PM Workspace

This directory is the product-management workspace for local-orchestration.

## Workflow

All feature work follows this stage order:

1. PRD
2. Review
3. Tests
4. Implementation
5. QA
6. Ship

This repo setup only defines the process and templates. It does not execute the workflow.

## Scope rules

- Each PRD must be the smallest valuable update possible.
- One PRD should target one atomic behavior change.
- Every PRD must include linked design and flowchart artifacts.
- SemVer must stay below `1.0.0` until the first fully functioning product is complete.

## Version policy (pre-1.0)

- Use versions in the form `0.x.y`.
- Increment `x` for meaningful behavior or contract changes.
- Increment `y` for small, backward-compatible updates and fixes.
- Do not publish `1.0.0` until the product meets the criteria in [VISION.md](./VISION.md).

## Directory layout

- `prds/`: PRD documents.
- `reviews/`: review outcomes tied to PRDs.
- `tests/`: test plans and traceability docs.
- `qa/`: QA reports and sign-off docs.
- `ship/`: release notes and ship decisions.
- `designs/`: UX and interaction designs paired with PRDs.
- `flowcharts/`: workflow/decision flowcharts paired with PRDs.
- `templates/`: reusable PM templates.
- `workflow/`: stage gates and AI execution rules.

Technical implementation and architecture details live under [`docs/tech`](../tech/README.md).
