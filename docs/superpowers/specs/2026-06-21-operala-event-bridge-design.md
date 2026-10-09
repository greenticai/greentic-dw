# Operala Event Bridge (Design) — SP-3 slice 3.1

- **Date:** 2026-06-21
- **Status:** Design approved, ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-operala-bridge`). greentic-dw only.
- **Part of:** AW-composer roadmap SP-3 (deep-worker via `operala.call`). This is the **transport/seam foundation** — slice 3.1.

## 1. Background

The runner already has the `operala.call` flow node + dispatch contract (`execute_operala_call` → `execute_remote_dispatch("operala")`, subjects `greentic.operala.request.v1` / `greentic.operala.response.v1`, contract `greentic_types::runtime_dispatch`). What's missing is a **runtime-side consumer** in greentic-dw: nothing subscribes to those subjects.

This mirrors the existing pattern: `agentic.call` → `aw-event-bridge` (in the runner workspace), `sorla.call` → `sorx-event-bridge` (in the separate sorx repo, which **mirrors** the wire types by hand because it's a different `greentic-types` lineage).

greentic-dw has **no `greentic-types` dependency** and no `runtime_dispatch` types. So, like `sorx-event-bridge`, this bridge **mirrors** the wire contract locally rather than sharing it — keeping the crate self-contained with no cross-repo `greentic-types` version alignment.

## 2. Scope (slice 3.1 only)

Build the bridge **plumbing + invoker seam + mock**, mirroring `aw-event-bridge`:
- Subscribe `greentic.operala.request.v1`, decode `RuntimeDispatchRequest`, invoke via an `OperalaDispatchInvoker` seam, publish `RuntimeDispatchResponse` on `greentic.operala.response.v1` echoing the correlation id.
- Ship a test stub invoker. The **real** invoker (wrapping `DeepLoopCoordinator` with concrete LLM-backed planner/reflector/delegator providers) is **out of scope** — those production providers don't exist yet and are a separate, larger slice (the deep-worker "brain").

## 3. Non-goals

- No real deep-worker execution (no concrete `PlanningProvider`/`ReflectionProvider`/`DelegationProvider`/`ContextProvider`/`WorkspaceProvider` impls — none exist in greentic-dw today).
- No runner change (the `operala.call` node + in-proc serve spawn are slices 3.2/3.3).
- No designer change (deep-worker authoring is slice 3.4).
- No `greentic-types` dependency (mirror the wire types).

## 4. Design

### 4.1 New crate `crates/greentic-dw-operala-bridge`
Added to the greentic-dw workspace `members`. Deps mirror `aw-event-bridge`: `async-nats`, `async-trait`, `anyhow`, `serde` (derive), `serde_json`, `tokio`, `futures-util`, `tracing`.

### 4.2 Mirrored wire contract (`wire` module)
Self-contained mirror of `greentic_types::runtime_dispatch` (byte-compatible serde so the runner's messages decode):
- `DispatchMode` enum `Await | FireAndForget`, `#[serde(rename_all = "snake_case")]`.
- `RuntimeDispatchRequest { target: String, operation: String, mode: DispatchMode, input: serde_json::Value, deadline_ms: Option<u64> }`.
- `RuntimeDispatchResponse { ok: bool, output: Value, events: Vec<Value> (#[serde(default)]), error: Option<DispatchError> (#[serde(default, skip_serializing_if=Option::is_none)]) }`.
- `DispatchError { code: String, message: String }`.
- `pub fn request_topic(runtime: &str) -> String` = `format!("greentic.{runtime}.request.v1")`; `response_topic` likewise.
- `pub const RUNTIME_NAME: &str = "operala";`

### 4.3 Invoker seam + bridge (mirror `aw-event-bridge`)
- `pub struct InvokeOutcome { ok: bool, output: Value, events: Vec<Value> }`.
- `#[async_trait] pub trait OperalaDispatchInvoker: Send + Sync { async fn invoke(&self, tenant: &str, env: &str, target: &str, operation: &str, input: Value, idempotency_key: Option<&str>) -> Result<InvokeOutcome>; }`
- `pub async fn build_response(invoker, tenant, env, idempotency_key, req) -> RuntimeDispatchResponse` — invoke; on `Err` → `ok:false`, `error: { code: "invoke_failed", message }`.
- `pub async fn handle_message(client, invoker, msg)` — read headers (`Greentic-Correlation-Id`, `Greentic-Idempotency-Key` (fallback to correlation), `Greentic-Tenant`, `Greentic-Env` default "default"), decode request, `build_response`, publish on `response_topic(RUNTIME_NAME)` with `Greentic-Correlation-Id`/`Tenant`/`Env` headers (**echo correlation verbatim**).
- `pub async fn run_bridge(client, invoker)` — subscribe `request_topic(RUNTIME_NAME)`, spawn one task per message.

## 5. Data flow

runner `operala.call` → `greentic.operala.request.v1` (RuntimeDispatchRequest + headers) → bridge `handle_message` → `OperalaDispatchInvoker::invoke` → `RuntimeDispatchResponse` → `greentic.operala.response.v1` (correlation echoed) → runner resumes the paused flow.

## 6. Error handling

- Invoker `Err` → `{ ok:false, error:{ code:"invoke_failed", message } }` (the runner observes it).
- Decode failure / publish failure → `handle_message` returns `Err`, logged in `run_bridge`'s spawned task; the bridge keeps serving.
- No panics; no `unwrap`/`expect` in non-test code.

## 7. Testing

- `wire`: subjects (`request_topic("operala") == "greentic.operala.request.v1"`, response likewise); `RuntimeDispatchRequest`/`Response` JSON round-trip (proves byte-compat with the runner's serde, incl. `DispatchMode` snake_case `"await"`/`"fire_and_forget"`).
- bridge: a `StubInvoker` records calls and returns a canned `InvokeOutcome`; `build_response` maps it to `RuntimeDispatchResponse` (ok + output), and an erroring stub maps to `ok:false` + `invoke_failed`. (Mirror `aw-event-bridge`'s tests; NATS-less unit coverage via `build_response` + a stub.)

## 8. Risks

- Low — self-contained crate, no cross-repo dep, template-driven from `aw-event-bridge`, mirror-types from the canonical `runtime_dispatch` (exact shapes captured). The wire-types round-trip test guards byte-compat with the runner.
- **Explicit limitation (documented):** after 3.1, a deep-worker still does nothing real — the `OperalaDispatchInvoker` has only a stub; the production invoker + LLM-backed deep-worker providers are a separate, larger slice.
