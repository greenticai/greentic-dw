# PR-11: Add multi-agent application model for digital worker packs

## Title
`feat(app-model): add multi-agent application model for digital worker packs`

## Objective
Formalize the target runtime and package shape for one generated application pack containing one or more digital workers.

## Deliverables
- `DwApplication`
- `DwApplicationAgentRef`
- `SharedCapabilityBinding`
- `AgentLocalBindingOverride`
- optional placeholder `InterAgentRoutingConfig`
- `ApplicationPackLayoutHints`

## Important Support
- shared providers across agents
- per-agent overrides
- one shared LLM provider with agent-specific model configuration if needed
- shared observer and control packs
- separate behavior assets per worker

## Acceptance Criteria
- app model supports N agents
- shared vs per-agent bindings are explicit
- future inter-agent workflows are not blocked
