# Deep-worker production polish — Implementation Plan (deep-worker brain, slice 8)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Harden the slice-5/6 deep-worker runtime path: `DeepLoopStatus: Display`, invoker `operation` routing + Display output, and `serve` tracing + graceful shutdown.

**Architecture:** Three small, cohesive changes across `greentic-dw-runtime`, `greentic-dw-operala-invoker`, `greentic-dw-cli` — one branch, one task.

**Tech Stack:** Rust edition 2024; tokio (signal), tracing, tracing-subscriber.

## Global Constraints

- Edition 2024. **No `.unwrap()`/`.expect()` in non-test code.**
- Conventional commit; **NO Claude/AI co-author or attribution**.
- Worktree `.worktrees/dw-polish` (greentic-dw), branch `feat/deep-worker-polish` (off research). greentic-dw pushes via **SSH**. Research-only.
- ALWAYS prefix cargo with `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Scope: `cargo test/clippy -p <crate>`. On "No space left on device": STOP, report BLOCKED.

## Reference (read first)

- `crates/greentic-dw-runtime/src/deep_loop.rs` (the `DeepLoopStatus` enum ~line 28).
- `crates/greentic-dw-operala-invoker/src/lib.rs` (`outcome_from_run` ~line 76, `invoke` ~line 89, the `#[cfg(test)]` module + `ScriptedLlm`).
- `crates/greentic-dw-cli/src/serve.rs` (`serve` ~line 45) + its `Cargo.toml` (tokio features).

---

## Task 1: Display + operation routing + serve hardening

**Files:**
- Modify: `crates/greentic-dw-runtime/src/deep_loop.rs`, `crates/greentic-dw-operala-invoker/src/lib.rs`, `crates/greentic-dw-cli/src/serve.rs`, `crates/greentic-dw-cli/Cargo.toml`

**Interfaces:**
- Produces: `impl Display for DeepLoopStatus`; `validate_operation(&str) -> anyhow::Result<()>`; `invoke` honoring `operation`; serve with tracing + SIGINT shutdown.

- [ ] **Step 1: Read the references** (the three files above; note exact `DeepLoopStatus` variants and the current `invoke`/`outcome_from_run` bodies).

- [ ] **Step 2: `DeepLoopStatus: Display` — failing test (RED)**

In `crates/greentic-dw-runtime/src/deep_loop.rs` tests module (add one if none exists for this), add:
```rust
#[test]
fn deep_loop_status_display_is_stable_lowercase() {
    assert_eq!(DeepLoopStatus::Completed.to_string(), "completed");
    assert_eq!(DeepLoopStatus::Failed.to_string(), "failed");
    assert_eq!(DeepLoopStatus::Planning.to_string(), "planning");
    assert_eq!(DeepLoopStatus::Idle.to_string(), "idle");
}
```
Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-runtime deep_loop_status_display` → FAIL.

- [ ] **Step 3: `DeepLoopStatus: Display` — implement (GREEN)**

Add near the enum:
```rust
impl std::fmt::Display for DeepLoopStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let token = match self {
            DeepLoopStatus::Idle => "idle",
            DeepLoopStatus::Planning => "planning",
            DeepLoopStatus::Executing => "executing",
            DeepLoopStatus::Reflecting => "reflecting",
            DeepLoopStatus::Revising => "revising",
            DeepLoopStatus::Delegating => "delegating",
            DeepLoopStatus::Completed => "completed",
            DeepLoopStatus::Failed => "failed",
        };
        f.write_str(token)
    }
}
```
Run the Step 2 test → PASS.

- [ ] **Step 4: Invoker — failing tests (RED)**

In `crates/greentic-dw-operala-invoker/src/lib.rs` tests module, add:
```rust
#[test]
fn validate_operation_accepts_empty_and_run() {
    assert!(validate_operation("").is_ok());
    assert!(validate_operation("run").is_ok());
    assert!(validate_operation("RUN").is_ok());
    assert!(validate_operation(" run ").is_ok());
}

#[test]
fn validate_operation_rejects_unknown() {
    assert!(validate_operation("delete").is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invoke_rejects_unsupported_operation() {
    let llm = std::sync::Arc::new(ScriptedLlm::new(vec![]));
    let invoker = DeepWorkerInvoker::new(llm);
    let err = invoker
        .invoke("acme", "default", "researcher", "delete", json!({"goal":"x"}), Some("run-1"))
        .await;
    assert!(err.is_err(), "unsupported operation must error before running the loop");
}
```
Also UPDATE the existing `outcome_from_run_maps_completed_and_failed` test: expect `output["status"] == "completed"` / `"failed"` (Display, lowercase) and (if you thread operation into the output) assert the `operation` field. Run: `cargo test -p greentic-dw-operala-invoker` → the new tests FAIL (helper/routing not present).

- [ ] **Step 5: Invoker — implement (GREEN)**

Add the helper near the other free functions:
```rust
/// Validate the dispatch operation. Empty or "run" (case-insensitive) selects
/// the default deep loop; anything else is rejected so callers get feedback
/// instead of a silently-ignored operation.
fn validate_operation(operation: &str) -> anyhow::Result<()> {
    let op = operation.trim();
    if op.is_empty() || op.eq_ignore_ascii_case("run") {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "unsupported operala operation: {operation:?} (use \"\" or \"run\")"
        ))
    }
}
```
Change `outcome_from_run` to take the operation and use Display:
```rust
fn outcome_from_run(run: &DeepLoopRun, operation: &str) -> InvokeOutcome {
    InvokeOutcome {
        ok: matches!(run.status, DeepLoopStatus::Completed),
        output: json!({
            "status": run.status.to_string(),
            "operation": operation,
            "artifact_ids": run.output_artifact_ids,
        }),
        events: vec![],
    }
}
```
In `invoke`: rename `_operation` → `operation`; add `validate_operation(operation)?;` as the FIRST line; capture `let operation = operation.to_string();` before `spawn_blocking` and move it into the closure; change the final closure line to `Ok(outcome_from_run(&run, &operation))`. (Update the existing `outcome_from_run_maps_*` test call sites to pass an operation arg.) Run: `cargo test -p greentic-dw-operala-invoker` → all PASS; `cargo clippy -p greentic-dw-operala-invoker --all-targets -- -D warnings` clean.

- [ ] **Step 6: Serve — Cargo.toml deps**

In `crates/greentic-dw-cli/Cargo.toml`: add `tracing = "0.1"` and `tracing-subscriber = "0.3"`; add `"signal"` to the existing `tokio` `features` array (keep `rt`, `rt-multi-thread`).

- [ ] **Step 7: Serve — tracing + graceful shutdown**

In `crates/greentic-dw-cli/src/serve.rs` `serve`, init the subscriber as the first line:
```rust
    tracing_subscriber::fmt().try_init().ok();
```
Replace the final `run_bridge(client, invoker).await` with:
```rust
    tokio::select! {
        result = run_bridge(client, invoker) => result,
        _ = tokio::signal::ctrl_c() => {
            println!("greentic-dw operala serve: shutdown signal received, stopping");
            Ok(())
        }
    }
```
(Keep the listening banner before the select.)

- [ ] **Step 8: Full scoped verification**

Run, all green/clean:
- `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-runtime -p greentic-dw-operala-invoker -p greentic-dw-cli`
- `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo clippy -p greentic-dw-runtime -p greentic-dw-operala-invoker -p greentic-dw-cli --all-targets -- -D warnings`
- `cargo fmt -p greentic-dw-runtime -p greentic-dw-operala-invoker -p greentic-dw-cli`

- [ ] **Step 9: Commit**

```bash
git add crates/greentic-dw-runtime crates/greentic-dw-operala-invoker crates/greentic-dw-cli Cargo.lock
git commit -m "feat(deep-worker): DeepLoopStatus Display, operation routing, serve tracing + graceful shutdown (deep-worker brain slice 8)"
```

---

## Manual verification

All three crates' tests green; `DeepLoopStatus::Completed.to_string() == "completed"`; `invoke` with an unsupported operation errors; `greentic-dw serve` (with NATS + creds) logs via tracing and stops cleanly on Ctrl-C.

## Self-Review (during planning)

- **Spec coverage:** §2a → Steps 2/3; §2b → Steps 4/5; §2c → Steps 6/7; §4 testing → Steps 2/4 (Display, validate_operation ×2, invoke-reject, updated outcome).
- **Placeholders:** none — full code for every change. Serve tracing/shutdown is compile + manual (signal/global subscriber not unit-testable) — explicitly scoped.
- **Type consistency:** `impl Display for DeepLoopStatus`; `validate_operation(&str) -> anyhow::Result<()>`; `outcome_from_run(&DeepLoopRun, &str)`; serve `tokio::select!` arms both yield `anyhow::Result<()>`. `tokio` gains `signal`; `tracing`/`tracing-subscriber` added.
