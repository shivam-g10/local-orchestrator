# AI Execution Rules

These rules allow AI systems to run the PM workflow consistently.

## Global rules

1. Do not skip stages.
2. Do not start a stage without required inputs from the previous stage.
3. Keep each PRD atomic and minimal.
4. Keep product version below `1.0.0` until the `VISION.md` exit criteria are met.
5. Every PRD must reference both a design artifact and a flowchart artifact.

## Stage contract

1. PRD
- Input: idea or problem statement.
- Output: completed PRD from template in `docs/pm/prds/`.

2. Review
- Input: PRD.
- Output: review decision in `docs/pm/reviews/`.

3. Tests
- Input: approved PRD and review feedback.
- Output: test plan in `docs/pm/tests/` with traceability to PRD acceptance criteria.

4. Implementation
- Input: approved test plan.
- Output: implementation spec in `docs/tech/implementation/`.

5. QA
- Input: implementation output and test plan.
- Output: QA report in `docs/pm/qa/`.

6. Ship
- Input: approved QA report.
- Output: ship note in `docs/pm/ship/`.

## Naming convention

Use a shared work item key:

`prd-XXXX-short-name`

Example companion files:

- `docs/pm/prds/prd-0001-crm-lead-dedupe.md`
- `docs/pm/designs/prd-0001-crm-lead-dedupe-design.md`
- `docs/pm/flowcharts/prd-0001-crm-lead-dedupe-flowchart.md`
- `docs/pm/reviews/prd-0001-crm-lead-dedupe-review.md`
