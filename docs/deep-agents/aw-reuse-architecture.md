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

- Inter-agent routing policy: which agent can delegate to which target, which
  specialist workers are callable through the coordinator tool surface, and
  which agent finalizes the response.
- Handoff envelope: structured task, context scope, expected output schema,
  source/target agent identifiers, callable worker tool identifier, return
  policy, permissions profile, and correlation identifiers.
- Runtime worker-tool registry: materialize callable worker metadata from the
  application routing contract or `DwApplicationPackSpec` and reject handoffs
  with unknown tools, disallowed routes, target mismatches, or output-schema
  mismatches.
- Coordinator worker-tool execution API: build typed handoff envelopes from a
  coordinator request, start validated subtasks through the delegation provider,
  and validate returned worker result envelopes before finalization.
- Shared context policy: what memory, artifacts, and tool results are visible to
  each agent.
- Finalization policy: which agent is responsible for composing the final answer.
- Final-response composition API: validate finalizer identity, require validated
  worker result sources, and preserve source references in the final response.
- Deterministic coordinator flow API: run a coordinator request with selected
  worker-tool calls through handoff start, worker result validation, and final
  response composition.
- Coordinator planner interface: transform an inbound user request into a
  deterministic coordinator flow request, leaving LLM/provider-specific planning
  behind a generic trait.
- Observability event names for agent selection, handoff, tool call, review, and
  final response. Coordinator flow emits stable observer events for start,
  worker handoff start, result receipt, final response creation, and completion.

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
- Which concrete LLM/provider implementation should back the generic
  coordinator planner for production AW packs?
- Which runtime APIs should consume the typed handoff/result envelopes from
  `greentic-dw-delegation` directly?
- Which additional rejection/error observer events should be emitted for failed
  planner decisions or failed worker-tool calls?
- Should DWBase be the default memory backend for AW demos, or only an optional
  provider choice?
