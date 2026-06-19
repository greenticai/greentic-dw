# Agentic Worker Reuse Architecture

## Decision

Agentic Worker (AW) should evolve from the existing Digital Worker (DW) pack,
runtime, and provider model. We should not introduce a separate Telco-X-specific
agent runtime for tools, memory, state, observability, or delegation.

The existing DW architecture already contains the right boundaries:

- `greentic-dw-types` owns the application and multi-agent pack contracts.
- `greentic-dw-runtime` owns provider-agnostic runtime execution, capability
  bindings, state, and memory access paths.
- `greentic-dw-pack` owns pack hooks, observers, and capability-pack integration.
- `greentic-dw-providers` owns provider families for LLM, memory, state, engine,
  planning, workspace, delegation, reflection, context, observer, and tool.
- `greentic-dwbase` can be used as an optional memory/state backend, not as the
  primary AW orchestration model.

## Naming

Use "Agentic Worker" as the external product concept for agent-capable workers.
Keep existing `dw` identifiers until a coordinated rename is planned across
pack schemas, provider IDs, capability URIs, CLI, docs, and downstream repos.

Recommended transition:

- Product/docs: introduce "Agentic Worker (AW), formerly/implemented via DW".
- Code/contracts: keep `dw` names stable for now.
- Future migration: add aliases before renaming identifiers.

## Reuse Model

AW should reuse the DW pack model directly:

- A worker application is represented as a `DwApplicationPackSpec`.
- Single-agent workers use one `agents[]` entry and `multi_agent=false`.
- Multi-agent workers use multiple `agents[]` entries and `multi_agent=true`.
- Shared provider packs are represented through `dependency_pack_refs`.
- Agent-specific provider needs are represented through `requirements`.
- Environment-specific setup remains in `setup_requirements`.
- Common assets live under the pack layout's shared asset roots.

This keeps pack generation, setup, deployment, and runtime binding in one path.

## Multi-Agent Execution

Multi-agent behavior should be expressed as additive DW/AW application metadata,
not as a new runtime stack.

Required additions should be small and contract-first:

- Inter-agent routing policy: which agent can delegate to which target.
- Handoff envelope: structured task, context scope, expected output schema, and
  correlation identifiers.
- Shared context policy: what memory, artifacts, and tool results are visible to
  each agent.
- Finalization policy: which agent is responsible for composing the final answer.
- Observability event names for agent selection, handoff, tool call, review, and
  final response.

The existing delegation provider family should be used for target selection and
rationale. It already models route targets, capabilities, schemas, fanout, and
decision rationale.

## Telco-X Boundary

Telco-X should be a domain package on top of AW, not an AW runtime fork.

Telco-X should provide:

- Domain agent templates, such as network assistant, traffic engineering, BGP
  diagnostics, and capacity planning.
- Domain tools, such as prefix resolver, device resolver, traffic lookup, and
  BGP/session lookup.
- Domain prompts, fixtures, schemas, and examples.

Telco-X should not own:

- Generic agent registry.
- Generic tool registry.
- Generic memory/state implementation.
- Generic observer or tracing implementation.
- Generic agent-to-agent handoff runtime.

Those belong in the DW/AW architecture.

## Greentic-X Boundary

Greentic-X should consume DW/AW packs through the normal pack/setup/runtime path.
If Greentic-X needs a routing layer for deterministic flow versus agentic worker,
that routing should dispatch into an AW pack rather than bypassing the DW/AW
contracts.

Recommended routing behavior:

- Clear deterministic request: dispatch to deterministic flow.
- Ambiguous or investigative request: dispatch to AW primary agent.
- Complex investigation: primary agent uses delegation policy to call specialist
  agents/tools and returns one final response.

## Implementation Sequence

1. Keep Telco-X main free of runnable AW demos and runtime experiments.
2. Keep Telco-X domain tools as reusable domain assets.
3. Add or confirm AW aliases in docs while preserving existing `dw` identifiers.
4. Extend the DW application pack contract only where existing fields cannot
   represent routing, handoff, shared context, or finalization policy.
5. Add a multi-agent fixture in `greentic-dw` that materializes:
   - two or more agents,
   - shared provider dependencies,
   - delegation routing,
   - observer requirements,
   - setup requirements.
6. Wire Greentic-X/Telco-X demo through the AW pack path, not a separate
   Telco-X runtime.

## Open Questions

- Should AW be a pure product/docs alias first, or do we need schema-level alias
  fields immediately?
- Is `DwApplicationPackSpec` enough for inter-agent routing policy, or should it
  gain an explicit routing section?
- Should handoff envelopes live in `greentic-dw-types`, `greentic-dw-delegation`,
  or the runtime crate?
- Which observer event names should become stable contract names?
- Should DWBase be the default memory backend for AW demos, or only an optional
  provider choice?
