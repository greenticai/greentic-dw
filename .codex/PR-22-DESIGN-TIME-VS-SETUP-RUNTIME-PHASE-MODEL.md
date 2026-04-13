# PR-22 — Design, setup, and runtime phase model

## Title
`feat(question-phases): define design/setup/runtime phase model for dw question assembly`

## Objective
Define the canonical question-phase model for the DW flow so design can complete without requiring environment-bound values, while still preserving everything needed for later setup and startup validation.

This PR is the contract boundary between:

- design-time DW composition
- later setup-time environment binding
- later runtime-state validation

## Final position
The contract should use exactly three phases:

- `design`
- `setup`
- `runtime`

Do not introduce `always` as a real phase.

If something needs to be shown in every mode, treat that as a visibility or rendering rule, not as lifecycle meaning.

## Why this model
The purpose of a phase is to describe when a value becomes meaningful.

- `design` means the value is required to define what is being built
- `setup` means the value is required to bind the design to a real environment
- `runtime` means the value is only known, validated, or materialized while running or just before launch

`always` does not describe meaning. It only describes display behavior. Mixing those ideas would make the later contracts harder to reason about.

## Phase definitions

### `design`
Questions in this phase define the digital worker or application composition itself.

Typical examples:

- application name
- worker or agent name
- template selection
- one-worker vs multi-agent choice
- provider family or provider selection
- shared vs per-agent provider strategy
- behavior configuration
- escalation behavior
- output or locale policy
- app-pack naming and high-level generation options

Design-phase outputs must be sufficient to produce:

- resolved composition
- application pack contribution
- bundle inclusion intent
- deferred setup requirements

### `setup`
Questions in this phase bind the designed system to a concrete environment.

Typical examples:

- secret names or secret refs
- service URLs
- deployment names
- region or tenant environment bindings
- auth configuration
- endpoint and connection details
- provider-specific installation bindings

Setup-phase outputs must be preservable and reviewable, but they must not block design-time composition.

### `runtime`
Questions or values in this phase are only meaningful during validation, launch preparation, or execution.

Typical examples:

- runtime-generated identifiers
- startup readiness checks
- values sourced from runtime state
- environment-discovered paths or endpoints
- final validation of unresolved startup requirements

Runtime phase is mostly about validation and state, not ordinary design-time prompting.

## Visibility is separate from phase
Visibility and depth must not be encoded as phases.

Recommended visibility policies:

- `required`
- `optional`
- `review_all`
- `hidden_unless_needed`

Recommended interpretation:

- phase answers when the value matters
- visibility answers whether the question is shown in the current UX mode

This keeps lifecycle and UX concerns separate.

## Depth selection model
The flow should not begin with the old `default` vs `personalised` framing.

Instead, after the template selection and initial provider-plan preview, the user chooses one of:

- use the recommended setup and answer only required questions
- review and configure all options

This choice controls design-time question depth only.

It does not change phase semantics.
It does not pull setup/runtime values into the design flow.

## Required contract changes

### Question and block metadata
Question contracts and assembled question blocks should carry phase explicitly.

Minimum fields:

- `phase`
- `visibility`
- `scope`
- `owner`
- `path`

This aligns with the later PR-25 assembly rules.

### QA assembly behavior
Assembly and filtering logic should be able to:

- include only `design` questions in the hosted design flow
- preserve `setup` items as deferred requirements
- surface `runtime` items only in validation or startup-oriented contexts

### Composition outputs
Resolved composition and downstream generated artifacts must preserve unresolved `setup` needs as first-class requirements, not as lost or implied metadata.

That means later contracts need an explicit deferred requirement shape rather than relying on comments or informal warnings.

## Contract consequences for downstream repos

### `greentic-pack`

- hosts only the design-time creation and edit flow in v1
- should not force environment secrets or endpoints into the hosted DW creation path

### `greentic-setup`

- becomes the primary consumer of deferred `setup` requirements
- should receive a deterministic translation of unresolved setup needs from DW outputs

### `greentic-start`

- consumes unresolved `runtime` and startup-readiness requirements
- validates what still must exist before launch

### `greentic-bundle`

- consumes design-time bundle inclusion intent and resolved pack inclusions
- should not own setup/runtime prompting behavior

## Recommended output behavior
The design flow should finish with a reviewable result even when setup values are absent.

That result should include at least:

- resolved design-time composition
- generated pack contribution
- generated bundle plan or inclusion intent
- deferred setup requirements
- warnings or unresolved runtime-readiness notes when relevant

This lines up with the review envelope defined later in PR-29.

## Examples

### Example: provider model selection

- phase: `design`
- reason: selecting the provider model changes what is being built

### Example: provider API key secret name

- phase: `setup`
- reason: it binds the selected design to a real environment

### Example: final startup dependency check

- phase: `runtime`
- reason: it validates launch readiness rather than design intent

## Non-goals

- Do not force the setup repo to ask every setup question during design
- Do not treat `review all` as a separate lifecycle phase
- Do not collapse startup validation into setup prompting
- Do not model visibility behavior through fake phases like `always`

## Acceptance criteria
PR-22 is complete when the phase model is unambiguous enough for later implementation PRs to rely on it:

- only `design`, `setup`, and `runtime` exist as phases
- visibility is defined separately from phase
- design-time flows can complete without secrets, URLs, or deployment bindings
- unresolved setup needs are preserved as explicit deferred requirements
- runtime readiness remains a separate downstream concern
- the depth choice is defined as a design-time visibility decision, not a phase change
