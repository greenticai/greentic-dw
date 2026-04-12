# PR-10: Add resolved digital worker composition document

## Title
`feat(composition): add resolved digital worker composition document`

## Objective
Create the central handoff contract between QA answers, template resolution, provider selection, and packaging or materialization.

## Deliverables
- `DwCompositionDocument`
- `DwAgentComposition`
- `CapabilityBinding`
- `ProviderBinding`
- `BehaviorConfig`
- `SetupRequirement`
- `PackDependencyRef`
- `BundleInclusionPlan`

## Top-level Shape
- application metadata
- one or more agents
- selected template per agent
- selected capability bindings per agent
- shared resources if any
- app-pack output plan
- pack dependency list
- bundle plan
- unresolved setup items
- source provenance

## Acceptance Criteria
- one composition can describe multiple agents
- each agent can have its own template and providers
- composition captures selected provider and source refs
- composition captures packaging intent
- composition is serializable and schema-exportable
