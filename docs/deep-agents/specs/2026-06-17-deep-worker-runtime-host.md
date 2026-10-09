# Spec: Deep-Worker Runtime Host

**Status:** Proposed · **Date:** 2026-06-17 · **Repos:** `greentic-dw`, `greentic-runner`, `greentic-dw-providers`

## 1. Problem

The deep-worker (plan → act → observe → reflect, with delegation) exists today as a
**library only**. `DeepLoopCoordinator` (`greentic-dw-runtime/src/deep_loop.rs`) is
constructed **exclusively in tests** — there is no production host in any repo that
builds it with real providers and runs it against live tasks. Concretely:

- `greentic-runner` / `greentic-start` contain **zero** references to
  `DeepLoopCoordinator` / `DwRuntime` / `deep_loop`.
- `DeepLoopCoordinator { runtime, planner, context, workspace, reflector, delegator }`
  holds **borrowed** trait objects (`&'a dyn …`), supplied by whoever runs the loop —
  currently only unit/harness tests.

Consequences:

- The deep-worker cannot be invoked from a flow / pack in production.
- **Deep-worker RAG (Track 4) is blocked at the host edge.** The RAG *core* is merged
  (greentic-dw #77 spec + #78: `BuildContextRequest.query`, inline-content
  `ContextFragment.content` + `ContextFragmentKind::KnowledgeChunk`, `render_context`,
  and `DeepLoopCoordinator::run` threading `plan.goal` / `step.title` context into the
  planner and reflector). But the two remaining tasks have nowhere to land:
  - **Task 2 — `KnowledgeContextProvider`**: a `ContextProvider` backed by
    `greentic-dw-knowledge::Knowledge`. Buildable as a library component, but it would
    plug into a host that does not exist.
  - **Task 5 — host mount**: *impossible* until a deep-worker host exists.

This spec defines the epic to **stand up a production deep-worker runtime host**,
mirroring the agentic-worker host, which unblocks Task 2/5 and makes the deep-worker
runnable from a flow.

## 2. Current state (inventory)

**Library (greentic-dw) — present:**

| Crate | Provides |
|---|---|
| `greentic-dw-runtime` | `DeepLoopCoordinator` + `run(&mut TaskEnvelope, PlanDocument) -> DeepLoopRun` (sync), `DwRuntime<E>` |
| `greentic-dw-engine` | `DwEngine` trait (+ a default impl) |
| `greentic-dw-planning` | `PlanningProvider` (`create_plan`, `next_actions`, `record_step_result`) |
| `greentic-dw-context` | `ContextProvider` (`build_context`/`compress_context`/`summarize_context`, **sync**) + RAG model |
| `greentic-dw-reflection` | `ReflectionProvider` trait |
| `greentic-dw-workspace` | `WorkspaceProvider` trait |
| `greentic-dw-delegation` | `DelegationProvider` trait |
| `greentic-dw-types` | `TaskEnvelope`, lifecycle states, plan/subtask contracts |

**Provider implementations (greentic-dw-providers) — present:**

- Planning: `planning/core`, `planning/llm-outline`, `planning/static`
- Delegation: `delegation/capability-match`, `delegation/static-router`
- Context: `context/static`, `context/compressor`
- Knowledge (for RAG): `knowledge/core` (`Knowledge` trait), `knowledge/chronicle` (`KnowledgeChronicle`)

**Gaps:**

- **No concrete `ReflectionProvider` / `WorkspaceProvider` impl** (trait-only crates). At
  least a minimal/LLM-backed reflector and a workspace store are needed to run.
- **No `KnowledgeContextProvider`** (Task 2) — the RAG context provider.
- **No host**: nothing constructs `DwRuntime` + the five providers + `DeepLoopCoordinator`
  and drives `run()`; no flow-node invocation seam; no per-tenant wiring.

## 3. Reference architecture — mirror the agentic-worker host

The agentic worker is the template:

- Flow node `dw.agent` → `FlowEngine` dispatches to an `AgentNodeHandler` trait object
  (`greentic-runner-host/src/runner/agent_node.rs`), set via
  `FlowEngine::set_agent_node_handler`.
- `build_agent_node_handler(merged_agents, tenant, secrets)` builds the
  `AgentRuntime` (Redis state, token meter, LLM backend, ext-runtime) and wraps it in a
  `RuntimeAgentNodeHandler`. Long-term memory + knowledge are mounted here
  (`long_term_memory::attach`, `knowledge_mount::attach`).
- `TenantRuntime::from_packs` collects agents from packs and wires the handler.

The deep-worker host should follow the same shape.

## 4. Proposed design

### 4.1 Invocation seam — `dw.deep_agent` flow node

Add a flow node kind `dw.deep_agent` (name TBD — see Open Decisions) dispatched by the
`FlowEngine` to a new `DeepAgentNodeHandler` trait object, analogous to
`AgentNodeHandler`. The node payload carries the task input; the handler runs the
deep-loop and returns the result/output artifacts.

### 4.2 Host home — `greentic-runner-host`

Place the host beside the agentic one (`greentic-runner-host/src/runner/deep_agent.rs`),
gated behind a `deep-worker` cargo feature (off by default; mirrors `agentic-worker`).
It depends on the `greentic-dw-*` crates (runtime/engine/providers) as git deps to
`greentic-dw` + `greentic-dw-providers` (research).

> Alternative considered: a dedicated `greentic-dw-host` crate in `greentic-dw`. Rejected
> for v1 — the runner already owns pack loading, tenancy, secrets, Redis, and the
> FlowEngine dispatch the deep-worker needs to reuse. Revisit if the deep-worker grows a
> standalone (non-flow) entrypoint.

### 4.3 Provider wiring (`build_deep_agent_node_handler`)

Construct, per tenant, from operator config + pack:

- **Planner** — `planning/llm-outline` (LLM-backed) with `planning/static` fallback.
- **Context** — **`KnowledgeContextProvider` (Task 2)** when knowledge env is present,
  else `context/static`. This is the RAG seam.
- **Reflector** — new minimal/LLM reflector (gap, §5).
- **Workspace** — new workspace store impl (gap, §5); back it with the same Redis/state
  store the agentic worker uses, or an artifact store.
- **Delegator** — `delegation/capability-match` (route subtasks to agents/tools).
- **Engine** — the `DwEngine` impl that executes leaf actions; v1 delegates execution to
  the **agentic-worker runtime** (so a deep-loop step that runs an agent reuses the
  existing `AgentRuntime`), keeping one execution substrate.

### 4.4 Task 2 — `KnowledgeContextProvider`

A `ContextProvider` whose `build_context` reads `req.query`, calls
`Knowledge::search(tenant, query, top_k)` and returns a `ContextPackage` of
inline-content fragments (`kind: KnowledgeChunk`, `content: Some(text)`,
`content_ref: ""`) that `render_context` turns into a `<knowledge>` block.

- **Home:** a new crate `greentic-dw-providers/knowledge/context` (`greentic-dw-knowledge-context`)
  depending on `greentic-dw-knowledge` (Knowledge trait) + `greentic-dw-context`
  (ContextProvider trait, git dep to greentic-dw).
- **Tenant:** `BuildContextRequest` carries no tenant, so the provider is constructed
  **per-tenant** (TenantCtx captured at build time) at the host edge.
- **Sync↔async bridge:** `ContextProvider::build_context` is **sync**; `Knowledge::search`
  is **async**, and the deep-loop runs inside a tokio runtime. Use
  `tokio::task::block_in_place(|| Handle::current().block_on(knowledge.search(…)))`
  (requires the multi-threaded runtime the runner already uses). `compress_context` /
  `summarize_context` pass through (no-op v1). Fail-soft: a search error yields an empty
  package so the loop proceeds without knowledge rather than aborting.

### 4.5 Task 5 — host mount

In `TenantRuntime::from_packs` (under the `deep-worker` feature), build the
`DeepAgentNodeHandler` via `build_deep_agent_node_handler(...)` and
`FlowEngine::set_deep_agent_node_handler(handler)`. The `KnowledgeContextProvider` is
wired here with the tenant + a `KnowledgeChronicle` (reuse `knowledge_mount`'s
config/driver builders; note the single-RocksDB-handle constraint — share one knowledge
connection between the agentic knowledge mount and the deep-worker if both are enabled,
or use distinct stores).

## 5. Provider gaps to close (sub-deliverables)

- **ReflectionProvider impl** — minimal rule-based + an LLM-backed `reflection/llm` (mirror `planning/llm-outline`).
- **WorkspaceProvider impl** — `workspace/state` over the runner's state store (scratch artifacts, step outputs).
- **DwEngine impl for production** — delegate-to-agentic-worker execution path.

## 6. Phased plan

1. **P1 — KnowledgeContextProvider (Task 2).** New crate + sync/async bridge + unit tests
   (mock Knowledge). Mergeable independently; no host needed to land.
2. **P2 — Reflection + Workspace impls** (§5) + production `DwEngine`.
3. **P3 — `dw.deep_agent` node + `DeepAgentNodeHandler`** in flow + runner-host (feature `deep-worker`, off by default).
4. **P4 — `build_deep_agent_node_handler` + `from_packs` mount (Task 5)**, wiring all
   providers incl. `KnowledgeContextProvider` per tenant.
5. **P5 — pack authoring**: a deep-agent pack shape (plan/providers/knowledge bindings)
   + designer surface; e2e: author → pack → run a deep task with RAG grounding.

## 7. Open decisions

- **Node name:** `dw.deep_agent` vs `dw.deep` vs reuse `dw.agent` with a `deep: true` flag.
- **Execution substrate:** deep-loop steps execute via the agentic-worker runtime (reuse)
  vs a dedicated `DwEngine` (isolation). Recommend reuse for v1.
- **Knowledge connection sharing:** when both the agentic knowledge mount and the
  deep-worker run on one process with embedded SurrealDB, they must not open two handles
  on one store dir — share a connection or separate stores.
- **Plan source:** who calls `PlanningProvider::create_plan` and persists the
  `PlanDocument` / `TaskEnvelope` across deep-loop iterations (workspace vs Redis).
- **Sync trait vs async:** keep `ContextProvider` sync (block_in_place bridge) vs make the
  deep-worker provider traits async (larger change across all five providers).

## 8. Risks

- `block_in_place` requires the multi-threaded tokio runtime; assert/guard at construction.
- Closing the provider gaps (reflection/workspace/engine) is itself non-trivial — this
  epic is **substantially larger** than the RAG follow-up it unblocks.
- Same `greentic-types` `agents`-field + private-`greentic-biz` CI-credential constraints
  as the agentic knowledge mount apply once chronicle deps are pulled (see the
  greentic-runner long-term/knowledge work and the CI-access follow-up).

## 9. Out of scope

Multi-deep-worker orchestration, durable cross-process deep-loop resumption, and a
non-flow standalone deep-worker entrypoint.
