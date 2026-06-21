# Deep-worker production polish (Design) — deep-worker brain, slice 8

- **Date:** 2026-06-21
- **Status:** Design approved (user chose "Invoker/serve polish"), ready for planning
- **Surface:** greentic-dw (3 crates: `greentic-dw-runtime`, `greentic-dw-operala-invoker`, `greentic-dw-cli`).
- **Part of:** SP-3 deep-worker brain — production hardening of the slice-5/6 runtime path. Closes three scoped gaps the slice-5/6 reviews flagged: ignored `operation`, `{:?}` status string, and `serve` lacking tracing + graceful shutdown.

## 1. The three gaps (verified)

1. `DeepWorkerInvoker::invoke` ignores `_operation` (`lib.rs:94`) — the dispatch operation has no effect, silently.
2. `outcome_from_run` serializes status via `format!("{:?}", run.status)` (`lib.rs:80`) — a `Debug` impl leaking into the output contract; no stable `Display`.
3. `serve` calls `run_bridge` directly with no tracing subscriber (so `run_bridge`'s `tracing::error!` per-message failures are dropped) and no shutdown signal (the process can only be killed).

## 2. Design

### 2a. `DeepLoopStatus: Display` (greentic-dw-runtime, `src/deep_loop.rs`)
Add `impl std::fmt::Display for DeepLoopStatus` mapping each variant to a stable lowercase token: `idle`/`planning`/`executing`/`reflecting`/`revising`/`delegating`/`completed`/`failed`. (`Debug` stays for diagnostics; `Display` is the stable surface.)

### 2b. Invoker: route `operation` + use `Display` (greentic-dw-operala-invoker, `src/lib.rs`)
- New pure helper `validate_operation(operation: &str) -> anyhow::Result<()>`: accept empty or `"run"` (case-insensitive, trimmed); any other value → `Err(anyhow!("unsupported operala operation: {operation:?} (use \"\" or \"run\")"))`. This gives `operation` a real, observable effect (rejection) without inventing divergent execution.
- `invoke`: rename `_operation` → `operation`; call `validate_operation(operation)?` BEFORE `spawn_blocking` (cheap fail-fast; the bridge maps the error to a `{code:"invoke_failed"}` response).
- `outcome_from_run`: `"status": run.status.to_string()` (Display) instead of `format!("{:?}", ...)`; also add `"operation": operation` to the output for traceability — thread the operation in by changing the signature to `outcome_from_run(run: &DeepLoopRun, operation: &str)` (or build the json inline in `invoke`). Existing unit test `outcome_from_run_maps_completed_and_failed` updates to expect `"completed"`/`"failed"`.

### 2c. Serve: tracing + graceful shutdown (greentic-dw-cli, `src/serve.rs` + `Cargo.toml`)
- Init a tracing subscriber once at the top of `serve`: `tracing_subscriber::fmt().try_init().ok();` (idempotent; ignore the "already set" error).
- Wrap the bridge in a shutdown-aware select:
```rust
tokio::select! {
    result = run_bridge(client, invoker) => result,
    _ = tokio::signal::ctrl_c() => {
        println!("greentic-dw operala serve: shutdown signal received, stopping");
        Ok(())
    }
}
```
- Deps: add `tracing = "0.1"`, `tracing-subscriber = "0.3"`; add `"signal"` to the `tokio` features.

## 3. Error handling

`validate_operation` returns `anyhow::Error` (→ bridge `invoke_failed`). `tracing_subscriber` init uses `try_init().ok()` (never panics). `ctrl_c()` errors propagate via the select arm's `Result` only on the bridge arm; the signal arm returns `Ok(())`. No `unwrap`/`expect` in non-test code.

## 4. Testing

- `greentic-dw-runtime`: `deep_loop_status_display_is_stable_lowercase` — assert `Completed.to_string() == "completed"`, `Planning.to_string() == "planning"`, etc. (a couple of representative variants + Failed).
- `greentic-dw-operala-invoker`:
  - `validate_operation_accepts_empty_and_run` (`""`, `"run"`, `"RUN"`, `" run "` → Ok) and `validate_operation_rejects_unknown` (`"delete"` → Err).
  - `invoke_rejects_unsupported_operation` (async): a `DeepWorkerInvoker` with any stub LLM, `invoke(..., operation="nope", ...)` → `Err` (fails before the loop runs).
  - update `outcome_from_run_maps_completed_and_failed` to expect Display strings + the `operation` field.
  - the existing e2e `invoke_runs_loop_to_terminal_status` passes `operation=""` (already does) and still completes.
- `greentic-dw-cli`: the tracing/shutdown changes are compile-checked + the existing helper tests; signal/subscriber behavior is manual (documented) — no new unit test (it needs a real signal/global subscriber).

## 5. Limitations

- `operation` routing is validate-and-reject (empty/`run`); richer operation verbs (e.g. `plan-only`, `dry-run`) are a future enhancement once the deep loop supports modes.
- `Display` strings are a new stable output surface; if any future external consumer keys off the old `{:?}` capitalized form, it must switch to the lowercase tokens (no such consumer exists today per the slice-5 review).
- Graceful shutdown drops the bridge future on SIGINT (no in-flight-request draining); a drain phase is a future enhancement.
- Research-only (do not forward-port to develop).
