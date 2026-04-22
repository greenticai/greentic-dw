# Deep-Agent Authoring Notes

Deep-agent mode is optional. Start with the normal DW flow unless your worker genuinely needs multi-step planning, explicit context compilation, artifact persistence, or formal review/delegation.

## Authoring principles

- Keep each family document typed and inspectable.
- Prefer small deterministic steps over one large opaque action.
- Store intermediate outputs in workspace artifacts rather than burying them in prompts.
- Use reflection to express acceptance, revision, retry, delegation, or failure explicitly.
- Use delegation only when a subtask needs isolation or a different capability profile.

## Planning guidance

- Give every plan step a stable `step_id`.
- Use `depends_on` for ordering rather than relying on list position alone.
- Keep `success_criteria` concrete enough that a completion check can answer yes or no.
- Reserve `delegate` steps for work that truly belongs to a subagent boundary.

## Context guidance

- Treat context as a compiled document, not a free-form prompt string.
- Include provenance on every fragment so later review can trace why something was included.
- Keep fragment ordering deterministic.
- Budget aggressively so later providers can compress or summarize predictably.

## Workspace guidance

- Persist evidence, drafts, and tool outputs as versioned artifacts.
- Preserve `derived_from` when one artifact is produced from another.
- Prefer stable artifact IDs that are easy to correlate with plan steps.

## Reflection guidance

- Use `binding: true` only when the review outcome must gate further execution.
- Put actionable detail into findings and suggested actions.
- Keep review targets explicit: step, artifact, agent, or final output.

## Delegation guidance

- Delegation should emit a `SubtaskEnvelope` that can be inspected and replayed.
- Keep the permission profile and expected output schema explicit.
- Pick merge policy deliberately rather than relying on default behavior.
