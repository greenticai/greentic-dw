# PR-21 — Cross-repo integration findings and compatibility matrix

## Title
`docs(integration-matrix): add cross-repo compatibility matrix for dw wizard integration`

## Objective
Turn the PR-20 audit into a stable planning artifact that shows:

- what each adjacent repo already supports
- what can be reused directly
- what needs a follow-on PR
- which repo should own each part of the DW design to setup chain
- what order the follow-on work should happen in

Primary source:

- [PR-20 audit](/Users/maarten/Documents/GitHub/agentic/greentic-dw/.codex/PR-20-AUDIT-ADJACENT-REPOS-FOR-DW-WIZARD-INTEGRATION.md)

## Confirmed hosting model
The audit supports this model and subsequent PRs should treat it as the default:

- `greentic-pack` owns the visible DW wizard entry in v1
- `greentic-dw` owns templates, provider catalogs, QA assembly, composition, review envelopes, and pack/bundle handoff contracts
- a dedicated resolver layer owns capability/provider requirement to concrete pack resolution
- `greentic-bundle` consumes resolved inclusion plans and builds bundles
- `greentic-setup` binds deferred setup/runtime values
- `greentic-start` validates readiness and unresolved startup/runtime requirements
- `greentic-flow` is a reuse target for generated flow assets
- `greentic-component` is informative but not required on the v1 critical path

## Locked planning decisions
These positions should be treated as settled for the next implementation PRs:

- visible v1 entrypoint: `greentic-pack`
- expert/dev direct flow remains in `greentic-dw`
- question phases: `design`, `setup`, `runtime`
- visibility policy stays separate from phase
- `DwApplicationPackSpec` should be treated as a mergeable pack contribution, not always the final artifact
- capability-to-pack resolution belongs in a dedicated resolver layer
- multi-agent editing in v1 should use deterministic full recomposition with stable agent IDs

## Compatibility matrix

| Repo | Current wizard entrypoints | Current QA model | Relevant downstream role | Reuse now | Needs new PR | Main missing extension points | Proposed follow-on PRs | Dependency order |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `greentic-pack` | Pack wizard and pack editing UX | Catalog-driven QA model | Visible host for DW create/edit inside app packs | Partial | Yes | DW sub-wizard hosting, replayable answer contract, validate/apply split, DW pack-contribution ingestion | `feat(pack): host add/edit digital workers in pack wizard`, `feat(pack): consume dw review envelope or pack contribution` | After PR-24 and PR-26 |
| `greentic-bundle` | `wizard run`, `wizard validate`, `wizard apply` | Stable answer document and replayable wizard flow | Bundle builder consuming resolved pack inclusion plan | Partial | Yes | Consume resolver output for capability packs, app-pack to bundle inclusion bridge | `feat(bundle): consume resolved capability inclusion plan for dw-driven app packs` | After PR-26 and PR-27 |
| `greentic-setup` | Interactive setup plus `--answers` and `--emit-answers` | QA bridge plus legacy setup-spec bridge | Bind deferred setup requirements from DW outputs | Partial | Yes | DW `SetupRequirement` bridge, review-envelope or pack-contribution translation into setup prompts | `feat(setup): consume dw setup requirements from pack contribution` | After PR-22, PR-26, and PR-29 |
| `greentic-start` | Startup and runtime validation flows, not a design wizard | Startup contract and runtime/provider state checks | Validate unresolved readiness before launch | Partial | Yes | Startup-readiness contract for unresolved DW setup/runtime requirements and missing packs | `feat(start): validate dw startup readiness and unresolved requirements` | After PR-27 and PR-29 |
| `greentic-flow` | Flow-oriented wizard surfaces and machine-readable flow outputs | Flow questions, setup-related summaries, resolution outputs | Reuse target for generated DW flow assets | Partial | Maybe | Explicit DW flow-asset compatibility contract | `docs(flow): confirm dw-generated flow asset compatibility` or `feat(flow): consume dw-generated flow assets` | After PR-25 and PR-29 if needed |
| `greentic-component` | `run`, `validate`, `apply`, `--answers`, `--emit-answers` | Schema-aware answer document workflow | Optional later reuse for component generation patterns | No for v1 critical path | No for v1 | None required for initial app-pack centric DW flow | None in v1 unless templates later require component generation | Out of critical path |

## Repo notes behind the matrix

### `greentic-pack`
Current fit is strong for hosting the visible DW flow because it already owns application-pack editing UX. The main problem is not placement but contract strength: it still needs a machine-readable answer/review boundary and a stable way to consume a DW-generated pack contribution.

### `greentic-bundle`
Bundle is already strong at replayable wizard flows and deterministic apply/validate behavior. The audit does not support letting bundle invent capability resolution rules. It should consume a resolved inclusion plan from upstream.

### `greentic-setup`
Setup is already the cleanest place for deferred values. It can likely consume DW-driven setup requirements once the translation contract is formalized.

### `greentic-start`
Start is correctly positioned for validation and readiness gating. It should receive a clear unresolved-requirement signal from upstream rather than learning about DW semantics itself.

### `greentic-flow`
Flow should be treated as a reuse target for assets or conventions. It does not currently justify becoming the top-level host for the DW creation flow.

### `greentic-component`
Component provides good design patterns for replayable wizards and downstream handoff, but it is not needed to get the first hosted DW path working.

## Reuse classification

### Can reuse directly

- `greentic-bundle` answer-document and validate/apply workflow patterns
- `greentic-setup` deferred-answer and dry-run setup workflow patterns
- `greentic-start` startup/readiness validation positioning
- `greentic-component` wizard-contract patterns as reference only

### Must extend

- `greentic-pack` as the visible DW host
- `greentic-bundle` to consume resolved capability inclusion plans
- `greentic-setup` to consume DW-specific setup requirements
- `greentic-start` to validate DW-specific unresolved readiness state
- `greentic-flow` only if explicit generated DW flow-asset reuse becomes necessary

### Should not change in v1

- `greentic-dw` remains the source of truth for DW domain logic
- `greentic-start` should not become a design wizard host
- `greentic-component` should not be pulled into the critical path without a demonstrated template need
- bundle should not become the owner of capability resolution logic

## Ownership split by contract area

### Wizard hosting

- owner: `greentic-pack`
- source of truth: `greentic-dw`

### Question assembly and composition

- owner: `greentic-dw`

### Pack contribution generation

- owner: `greentic-dw`
- consumer: `greentic-pack`

### Capability-to-pack resolution

- owner: dedicated resolver layer
- consumers: `greentic-dw`, `greentic-bundle`

### Bundle inclusion application

- owner: `greentic-bundle`

### Setup requirement binding

- owner: `greentic-setup`

### Startup readiness validation

- owner: `greentic-start`

## Risks and blockers

### Risk: `greentic-pack` embedding contract stays underspecified
If pack consumes ad hoc DW outputs instead of a stable envelope or pack contribution contract, later editing and review flows will fragment.

### Risk: capability resolution is split across repos
If `greentic-dw`, `greentic-pack`, and `greentic-bundle` all improvise partial capability mapping rules, the bundle result will drift from the reviewed DW design.

### Risk: setup translation is left implicit
If deferred setup requirements are not translated deterministically into setup prompts, the design-to-setup handoff will become lossy and hard to review.

### Risk: readiness validation lacks a single contract
If `greentic-start` does not receive a stable unresolved-requirements signal, missing runtime prerequisites may surface too late.

## Recommended dependency order
This is the suggested order for the next implementation PRs:

1. PR-22: lock the `design` / `setup` / `runtime` phase model and visibility split
2. PR-24: define `greentic-pack` as the visible host contract
3. PR-25: define deterministic DW question assembly, merge order, and collision rules
4. PR-26: define `DwApplicationPackSpec` as a mergeable pack contribution
5. PR-27: define dedicated capability resolution and bundle inclusion handoff
6. PR-28: define multi-agent app-pack flow on top of the above contracts
7. PR-29: define the combined deterministic review envelope consumed by downstream repos
8. Downstream repo PRs in `greentic-pack`, `greentic-bundle`, `greentic-setup`, and `greentic-start`

## Concrete follow-on PR inventory by repo

### `greentic-pack`

- `feat(pack): host add/edit digital workers in application-pack wizard`
- `feat(pack): consume dw review envelope and merge dw pack contribution`

### `greentic-bundle`

- `feat(bundle): consume resolved capability inclusion plan from dw-driven app packs`

### `greentic-setup`

- `feat(setup): consume dw setup requirements from pack contribution or review envelope`

### `greentic-start`

- `feat(start): validate unresolved dw startup requirements before launch`

### `greentic-flow`

- `docs(flow): confirm compatibility for dw-generated flow assets`
- optional only if generated asset reuse becomes part of the implementation path

## Acceptance outcome
PR-21 is complete when used as a matrix artifact because it now:

- translates the PR-20 audit into a concrete compatibility table
- distinguishes direct reuse from required extension work
- captures the validated ownership split
- identifies the downstream repos that need follow-on work
- gives a dependency order that the later PRs can follow
