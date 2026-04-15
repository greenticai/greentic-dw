# Deep-Agent Migration Notes

Existing Digital Worker apps do not need to change unless they want deep-agent behavior.

## Stay in normal DW mode when

- A single deterministic engine decision sequence is enough.
- You do not need multi-step replanning.
- You do not need explicit workspace artifacts for intermediate outputs.
- You do not need formal reflection or delegation documents.

## Enable deep mode when

- The worker needs an inspectable multi-step plan.
- Context has to be assembled from multiple sources with provenance.
- Intermediate artifacts need to be stored and versioned.
- Review outcomes should control retry, revision, or delegation.
- Subtasks need isolated envelopes and explicit merge behavior.

## Minimal adoption path

1. Add a `deep_agent` section to the manifest and set `enabled: true`.
2. Configure planning and context capabilities first.
3. Add workspace when intermediate artifacts matter.
4. Add reflection when review should gate progress.
5. Add delegation only when `delegate` plan steps are present.

## Compatibility notes

- Deep mode is opt-in.
- Existing CLI and runtime flows keep working with `deep_agent: null` or the field omitted.
- Manifest validation rejects deep-agent configurations that enable the loop without planning and context.
- Delegate steps require a delegation capability.
- Mandatory reflection policies require a reflection capability.

## Recommended rollout

- Start with stub providers in tests.
- Validate your plan and context documents against the fixture style in `fixtures/deep/`.
- Keep old non-deep flows intact until your deep-agent path is proven.

## Debugging checklist

- validate the manifest before testing runtime behavior
- confirm planning and context capabilities are present when deep mode is enabled
- confirm delegate steps are not present without delegation capability wiring
- confirm mandatory reflection policies have a reflection capability
- inspect the generated workspace artifacts for expected intermediate outputs
- compare your documents with the examples in `docs/deep-agents/contracts/`

## Common anti-patterns

- enabling deep mode for a trivial one-step worker
- storing critical intermediate state only in prompts
- delegating work without a stable `SubtaskEnvelope`
- letting review logic silently mutate control flow without a typed `ReviewOutcome`
- mixing deterministic runtime concerns and provider-family concerns in one opaque component
