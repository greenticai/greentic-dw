# PR-19: Add end-to-end tests for default and personalised composition flows

## Title
`test(dw-wizard-core): add end-to-end tests for default and personalised composition flows`

## Objective
Prove the full design works before UI refinement.

## Deliverables
- tests for one-agent default flow
- tests for one-agent personalised flow
- tests for multi-agent default flow
- tests for multi-agent personalised flow
- tests for provider override handling
- tests for bundle plan generation
- tests for app pack spec generation

## Scenario Coverage
- support assistant with default providers
- approval worker with customised provider overrides
- app pack with two agents sharing observer and control but different LLM config
- app pack with two agents using different templates

## Acceptance Criteria
- default mode only asks unresolved mandatory inputs
- personalised mode exposes optional sections
- both modes produce the same shape of resolved composition
- multi-agent pack generation works
