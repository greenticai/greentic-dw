# Operala Event Bridge Implementation Plan (SP-3 slice 3.1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `greentic-dw-operala-bridge` crate that consumes `greentic.operala.request.v1` from NATS, invokes a deep-worker via an `OperalaDispatchInvoker` seam, and publishes `greentic.operala.response.v1` (correlation echoed) — mirroring `aw-event-bridge`, with the wire contract mirrored locally (greentic-dw has no `greentic-types`).

**Architecture:** Self-contained crate: a `wire` module mirroring `greentic_types::runtime_dispatch` (byte-compatible serde) + an invoker seam + NATS plumbing. Ships a stub invoker only; the real `DeepLoopCoordinator`-backed invoker is a separate later slice.

**Tech Stack:** Rust edition 2024 (workspace), async-nats 0.46, async-trait, tokio, serde/serde_json, futures-util, anyhow, tracing.

## Global Constraints

- Rust edition 2024, rust-version 1.94 (greentic-dw workspace). License MIT.
- **No `greentic-types` dependency** — mirror the wire types locally (greentic-dw is a separate lineage, like sorx-event-bridge).
- Wire types must be **byte-compatible** with `greentic_types::runtime_dispatch`: `DispatchMode` serde `rename_all = "snake_case"` (`"await"` / `"fire_and_forget"`); `RuntimeDispatchResponse.events` `#[serde(default)]`; `.error` `#[serde(default, skip_serializing_if = "Option::is_none")]`. Subjects: `greentic.operala.request.v1` / `greentic.operala.response.v1`. `RUNTIME_NAME = "operala"`.
- No `.unwrap()`/`.expect()` in non-test code.
- Conventional commits; NO Claude/AI co-author or attribution.
- Work in worktree `.worktrees/operala-bridge` (greentic-dw) on branch `feat/operala-event-bridge`. Do NOT touch other worktrees.
- greentic-dw pushes via **SSH** (`git@github.com:greenticai/greentic-dw`) — HTTPS is read-only.
- Build/test scoped to the new crate: `cargo build -p greentic-dw-operala-bridge`, `cargo test -p greentic-dw-operala-bridge`, `cargo clippy -p greentic-dw-operala-bridge --all-targets -- -D warnings`, `cargo fmt -p greentic-dw-operala-bridge`. If a cargo op needs to fetch a private git dep and libgit2 auth fails, prefix with `CARGO_NET_GIT_FETCH_WITH_CLI=true`.

## Reference facts (verified)

- Template: greentic-runner `crates/aw-event-bridge/src/lib.rs` (structure: `InvokeOutcome`, `AgentDispatchInvoker` trait, `build_response`, `handle_message`, `run_bridge`, StubInvoker tests) and its `Cargo.toml` (deps).
- Canonical wire types being mirrored: `greentic_types::runtime_dispatch` — `DispatchMode { Await, FireAndForget }` (snake_case), `RuntimeDispatchRequest { target, operation, mode, input: Value, deadline_ms: Option<u64> }`, `RuntimeDispatchResponse { ok, output: Value, events: Vec<Value>, error: Option<DispatchError> }`, `DispatchError { code, message }`, `request_topic(rt)="greentic.{rt}.request.v1"`, `response_topic(rt)="greentic.{rt}.response.v1"`.
- greentic-dw workspace `Cargo.toml`: `[workspace] members = [...]` (add the new crate path); `workspace.package { version="1.1.0-dev.0", edition="2024", rust-version="1.94", license="MIT" }`; `workspace.dependencies` has `serde_json`, `thiserror`, `tempfile` but NOT async-nats/tokio/async-trait/futures-util/anyhow/tracing/serde → declare those inline in the crate.

---

## Task 1: Scaffold crate + mirrored `wire` module

**Files:**
- Create: `crates/greentic-dw-operala-bridge/Cargo.toml`
- Create: `crates/greentic-dw-operala-bridge/src/lib.rs` (wire module + re-exports; bridge added in Task 2)
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Produces: `wire::{DispatchMode, RuntimeDispatchRequest, RuntimeDispatchResponse, DispatchError, request_topic, response_topic}`, `RUNTIME_NAME`.

- [ ] **Step 1: Create the crate Cargo.toml**

`crates/greentic-dw-operala-bridge/Cargo.toml`:

```toml
[package]
name        = "greentic-dw-operala-bridge"
version     = { workspace = true }
edition     = { workspace = true }
license     = { workspace = true }
description = "NATS event bridge for the Greentic deep-worker runtime: consumes greentic.operala.request.v1 and dispatches to OperalaDispatchInvoker."
publish     = false

[dependencies]
anyhow       = "1"
async-nats   = "0.46"
async-trait  = "0.1"
futures-util = "0.3"
serde        = { version = "1", features = ["derive"] }
serde_json   = { workspace = true }
tokio        = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
tracing      = "0.1"

[dev-dependencies]
serde_json = { workspace = true }
tokio      = { version = "1", features = ["rt", "rt-multi-thread", "macros", "time"] }
```

- [ ] **Step 2: Register in the workspace**

In greentic-dw `Cargo.toml`, add to `[workspace] members` (next to the other `crates/...` entries):

```toml
    "crates/greentic-dw-operala-bridge",
```

- [ ] **Step 3: Write the failing wire tests**

Create `crates/greentic-dw-operala-bridge/src/lib.rs` with the wire module + tests (bridge comes in Task 2):

```rust
//! Event bridge: consume `greentic.operala.request.v1` from NATS, invoke the
//! local deep-worker runtime via the [`OperalaDispatchInvoker`] seam, and
//! publish `greentic.operala.response.v1` echoing the correlation id.
//!
//! Runtime-side counterpart of the runner's `operala.call` flow node. greentic-dw
//! has no `greentic-types` dependency, so the wire contract is MIRRORED here
//! (byte-compatible with `greentic_types::runtime_dispatch`), like sorx-event-bridge.

/// Runtime name selecting the operala subjects.
pub const RUNTIME_NAME: &str = "operala";

/// Mirror of `greentic_types::runtime_dispatch` (byte-compatible serde).
pub mod wire {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    /// Await vs fire-and-forget dispatch semantics.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum DispatchMode {
        Await,
        FireAndForget,
    }

    /// Payload of a `greentic.<runtime>.request.v1` message.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct RuntimeDispatchRequest {
        pub target: String,
        pub operation: String,
        pub mode: DispatchMode,
        pub input: Value,
        pub deadline_ms: Option<u64>,
    }

    /// Payload of a `greentic.<runtime>.response.v1` message.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct RuntimeDispatchResponse {
        pub ok: bool,
        pub output: Value,
        #[serde(default)]
        pub events: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub error: Option<DispatchError>,
    }

    /// Structured error returned when a dispatch fails.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct DispatchError {
        pub code: String,
        pub message: String,
    }

    /// Request subject for a runtime, e.g. `greentic.operala.request.v1`.
    pub fn request_topic(runtime: &str) -> String {
        format!("greentic.{runtime}.request.v1")
    }

    /// Response subject for a runtime, e.g. `greentic.operala.response.v1`.
    pub fn response_topic(runtime: &str) -> String {
        format!("greentic.{runtime}.response.v1")
    }
}

#[cfg(test)]
mod wire_tests {
    use super::wire::*;
    use super::RUNTIME_NAME;
    use serde_json::json;

    #[test]
    fn subjects_use_operala_runtime_name() {
        assert_eq!(request_topic(RUNTIME_NAME), "greentic.operala.request.v1");
        assert_eq!(response_topic(RUNTIME_NAME), "greentic.operala.response.v1");
    }

    #[test]
    fn dispatch_mode_serializes_snake_case() {
        assert_eq!(serde_json::to_value(DispatchMode::Await).unwrap(), "await");
        assert_eq!(
            serde_json::to_value(DispatchMode::FireAndForget).unwrap(),
            "fire_and_forget"
        );
    }

    #[test]
    fn request_round_trips_through_json() {
        let req = RuntimeDispatchRequest {
            target: "researcher".into(),
            operation: String::new(),
            mode: DispatchMode::Await,
            input: json!({ "user_text": "plan X" }),
            deadline_ms: Some(30_000),
        };
        let back: RuntimeDispatchRequest =
            serde_json::from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn response_omits_error_when_none_and_defaults_events() {
        let resp = RuntimeDispatchResponse {
            ok: true,
            output: json!({ "reply": "done" }),
            events: vec![],
            error: None,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("error").is_none());
        // events present but empty round-trips; a payload without events still decodes.
        let decoded: RuntimeDispatchResponse =
            serde_json::from_value(json!({ "ok": true, "output": {} })).unwrap();
        assert!(decoded.events.is_empty() && decoded.error.is_none());
    }
}
```

- [ ] **Step 4: Run wire tests (RED→GREEN)**

Run: `cargo test -p greentic-dw-operala-bridge 2>&1 | tail -15`
Expected: the crate compiles and the 4 wire tests pass. (If a private-dep fetch fails, prefix `CARGO_NET_GIT_FETCH_WITH_CLI=true`.)

- [ ] **Step 5: Commit**

```bash
git add crates/greentic-dw-operala-bridge/Cargo.toml crates/greentic-dw-operala-bridge/src/lib.rs Cargo.toml
git commit -m "feat(operala-bridge): scaffold crate + mirrored runtime-dispatch wire types"
```

---

## Task 2: Invoker seam + NATS bridge

**Files:**
- Modify: `crates/greentic-dw-operala-bridge/src/lib.rs`

**Interfaces:**
- Consumes: `wire::*`, `RUNTIME_NAME` (Task 1).
- Produces: `InvokeOutcome`, `OperalaDispatchInvoker`, `build_response`, `handle_message`, `run_bridge`.

- [ ] **Step 1: Write the failing bridge tests**

Add to `lib.rs` (new `#[cfg(test)] mod bridge_tests`):

```rust
#[cfg(test)]
mod bridge_tests {
    use super::wire::*;
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    struct StubInvoker {
        ok: bool,
        seen: Mutex<Vec<(String, String, serde_json::Value, Option<String>)>>,
    }

    #[async_trait::async_trait]
    impl OperalaDispatchInvoker for StubInvoker {
        async fn invoke(
            &self,
            _tenant: &str,
            _env: &str,
            target: &str,
            operation: &str,
            input: serde_json::Value,
            idempotency_key: Option<&str>,
        ) -> anyhow::Result<InvokeOutcome> {
            self.seen.lock().unwrap().push((
                target.to_string(),
                operation.to_string(),
                input,
                idempotency_key.map(str::to_string),
            ));
            if self.ok {
                Ok(InvokeOutcome {
                    ok: true,
                    output: json!({ "reply": "planned" }),
                    events: vec![],
                })
            } else {
                anyhow::bail!("deep loop blew up")
            }
        }
    }

    fn sample_request() -> RuntimeDispatchRequest {
        RuntimeDispatchRequest {
            target: "researcher".into(),
            operation: String::new(),
            mode: DispatchMode::Await,
            input: json!({ "user_text": "go" }),
            deadline_ms: Some(30_000),
        }
    }

    #[tokio::test]
    async fn build_response_maps_success() {
        let invoker = Arc::new(StubInvoker { ok: true, seen: Mutex::new(vec![]) });
        let resp = build_response(invoker.clone(), "acme", "prod", Some("corr-1"), sample_request()).await;
        assert!(resp.ok);
        assert_eq!(resp.output, json!({ "reply": "planned" }));
        assert!(resp.error.is_none());
        let seen = invoker.seen.lock().unwrap();
        assert_eq!(seen[0].0, "researcher");
        assert_eq!(seen[0].3.as_deref(), Some("corr-1"));
    }

    #[tokio::test]
    async fn build_response_maps_error() {
        let invoker = Arc::new(StubInvoker { ok: false, seen: Mutex::new(vec![]) });
        let resp = build_response(invoker, "acme", "prod", None, sample_request()).await;
        assert!(!resp.ok);
        let err = resp.error.expect("error present");
        assert_eq!(err.code, "invoke_failed");
        assert!(err.message.contains("deep loop blew up"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p greentic-dw-operala-bridge bridge_tests 2>&1 | tail -15`
Expected: FAIL — `OperalaDispatchInvoker`/`InvokeOutcome`/`build_response` not found.

- [ ] **Step 3: Implement the seam + bridge**

Add to `lib.rs` (after the `wire` module, before the test modules; add the needed `use`s at the top of the file):

```rust
use std::sync::Arc;

use anyhow::Result;
use async_nats::HeaderMap;
use async_trait::async_trait;
use serde_json::Value;

pub use wire::{request_topic, response_topic};
use wire::{DispatchError, RuntimeDispatchRequest, RuntimeDispatchResponse};

/// Result of invoking the local deep-worker runtime for one dispatch.
pub struct InvokeOutcome {
    pub ok: bool,
    pub output: Value,
    pub events: Vec<Value>,
}

/// Seam over the actual deep-worker invocation. The production impl will wrap a
/// `DeepLoopCoordinator` with concrete planner/reflector/delegator/context/
/// workspace providers (a separate slice); tests use a stub.
///
/// `target` is the deep-worker id; `operation` is reserved and may be empty;
/// `input` is the opaque node input; `idempotency_key` doubles as the session
/// hint when the input carries no explicit session id.
#[async_trait]
pub trait OperalaDispatchInvoker: Send + Sync {
    async fn invoke(
        &self,
        tenant: &str,
        env: &str,
        target: &str,
        operation: &str,
        input: Value,
        idempotency_key: Option<&str>,
    ) -> Result<InvokeOutcome>;
}

/// Invoke and build the response (no NATS I/O). Errors map to an error response.
pub async fn build_response(
    invoker: Arc<dyn OperalaDispatchInvoker>,
    tenant: &str,
    env: &str,
    idempotency_key: Option<&str>,
    req: RuntimeDispatchRequest,
) -> RuntimeDispatchResponse {
    match invoker
        .invoke(tenant, env, &req.target, &req.operation, req.input, idempotency_key)
        .await
    {
        Ok(outcome) => RuntimeDispatchResponse {
            ok: outcome.ok,
            output: outcome.output,
            events: outcome.events,
            error: None,
        },
        Err(error) => RuntimeDispatchResponse {
            ok: false,
            output: Value::Null,
            events: vec![],
            error: Some(DispatchError {
                code: "invoke_failed".into(),
                message: error.to_string(),
            }),
        },
    }
}

/// Handle one request message end-to-end: decode, invoke, publish response.
/// The correlation id is echoed VERBATIM (the runner encodes resume markers there).
pub async fn handle_message(
    client: &async_nats::Client,
    invoker: Arc<dyn OperalaDispatchInvoker>,
    msg: async_nats::Message,
) -> Result<()> {
    let headers = msg.headers.as_ref();
    let get_header = |name: &str| -> Option<String> {
        headers
            .and_then(|header_map| header_map.get(name))
            .map(|value| value.as_str().to_string())
    };

    let correlation = get_header("Greentic-Correlation-Id");
    let idempotency = get_header("Greentic-Idempotency-Key").or_else(|| correlation.clone());
    let tenant = get_header("Greentic-Tenant").unwrap_or_default();
    let env = get_header("Greentic-Env").unwrap_or_else(|| "default".to_string());

    let req: RuntimeDispatchRequest = serde_json::from_slice(&msg.payload)?;
    let resp = build_response(invoker, &tenant, &env, idempotency.as_deref(), req).await;

    let mut out_headers = HeaderMap::new();
    if let Some(correlation_value) = correlation.as_deref() {
        out_headers.insert("Greentic-Correlation-Id", correlation_value);
    }
    out_headers.insert("Greentic-Tenant", tenant.as_str());
    out_headers.insert("Greentic-Env", env.as_str());

    let response_bytes = serde_json::to_vec(&resp)?;
    client
        .publish_with_headers(response_topic(RUNTIME_NAME), out_headers, response_bytes.into())
        .await?;
    Ok(())
}

/// Subscribe to `greentic.operala.request.v1` and serve forever (one spawned
/// task per message).
pub async fn run_bridge(
    client: async_nats::Client,
    invoker: Arc<dyn OperalaDispatchInvoker>,
) -> Result<()> {
    use futures_util::StreamExt;
    let mut subscriber = client.subscribe(request_topic(RUNTIME_NAME)).await?;
    while let Some(msg) = subscriber.next().await {
        let client = client.clone();
        let invoker = invoker.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_message(&client, invoker, msg).await {
                tracing::error!(%error, "operala event bridge failed to handle request");
            }
        });
    }
    Ok(())
}
```

- [ ] **Step 4: Run bridge tests (GREEN)**

Run: `cargo test -p greentic-dw-operala-bridge 2>&1 | tail -15`
Expected: all wire + bridge tests pass.

- [ ] **Step 5: Clippy + fmt**

Run: `cargo clippy -p greentic-dw-operala-bridge --all-targets -- -D warnings 2>&1 | tail -6` → clean.
Run: `cargo fmt -p greentic-dw-operala-bridge` (no diff after).

- [ ] **Step 6: Commit**

```bash
git add crates/greentic-dw-operala-bridge/src/lib.rs
git commit -m "feat(operala-bridge): OperalaDispatchInvoker seam + NATS request/response bridge"
```

---

## Manual verification (after Task 2)

`cargo test -p greentic-dw-operala-bridge` is green; the crate exposes `run_bridge(client, invoker)` ready for a future production invoker. (No live NATS / deep-worker execution yet — the stub invoker is the only impl; the `DeepLoopCoordinator`-backed invoker + LLM providers are a separate slice.)

## Self-Review (completed during planning)

- **Spec coverage:** §4.1 crate/Cargo + §4.2 wire → Task 1; §4.3 seam + bridge → Task 2; §7 testing folded in.
- **Placeholder scan:** complete code for Cargo.toml, wire module, seam, bridge, and tests — pure transcription. The only lookup is the exact spot in `members` to insert the crate path.
- **Type consistency:** `OperalaDispatchInvoker`/`InvokeOutcome`/`build_response`/`handle_message`/`run_bridge` and the mirrored `wire::*` types are consistent across tasks and byte-compatible with `greentic_types::runtime_dispatch` (snake_case `DispatchMode`, defaulted `events`, skipped-none `error`).
