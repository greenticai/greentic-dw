# Deep-Agent Authoring Handbook

This handbook is for pack and app authors who want to adopt deep mode gradually.

## Minimal viable configuration

Start with:

1. `deep_agent.enabled = true`
2. a planning capability
3. a context capability

Then layer in:

- workspace when intermediate outputs matter
- reflection when review should gate progress
- delegation when plans include delegate steps

## Debugging checklist

- confirm the manifest validates with deep mode enabled
- confirm planning and context capabilities are configured
- confirm delegate steps are paired with a delegation capability
- confirm reflection policy settings match the available reflection capability
- validate fixtures and sample documents against the repo examples in `fixtures/deep/`
- inspect workspace artifacts to confirm outputs are persisted as expected

## Common anti-patterns

- turning on deep mode for single-step deterministic flows
- using prompt text as the only source of plan state
- skipping artifact persistence for important intermediate outputs
- mixing parent and subagent authority without an explicit envelope
- using reflection as hidden control logic instead of a typed review outcome

## Rollout advice

- begin with stub providers and local tests
- keep existing non-deep paths working during migration
- add one family at a time rather than implementing everything at once
- use checked-in fixtures and contract docs as the baseline shape for new providers
