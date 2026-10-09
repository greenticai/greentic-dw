# `greentic-dw serve` operala command — Implementation Plan (deep-worker brain, slice 6 / SP-3.3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add a `serve` subcommand to `greentic-dw-cli` that connects NATS, builds an LLM from env, constructs a `DeepWorkerInvoker`, and runs `greentic_dw_operala_bridge::run_bridge` — making `operala.call` dispatches reach the deep loop.

**Architecture:** Keep the CLI synchronous. The `Serve` arm builds a local multi-thread Tokio runtime and `block_on`s an async `serve`. Two pure helpers (`resolve_nats_url`, `resolve_model`) are unit-tested; LLM/NATS/bridge wiring is compile-checked + manual.

**Tech Stack:** Rust edition 2024; clap 4 derive; `greentic-dw-operala-bridge` + `greentic-dw-operala-invoker` (path), `greentic-llm` (git tag v1.2.6-research), `async-nats = "0.46"`, tokio, anyhow.

## Global Constraints

- Edition 2024. **No `.unwrap()`/`.expect()` in non-test code.**
- Do NOT make `main`/`run`/`run_from_env` async — keep the wizard path synchronous; confine async to the serve module via a local runtime + `block_on`.
- `greentic-llm` git tag `v1.2.6-research` (no path/patch).
- Conventional commits; **NO Claude/AI co-author or attribution**.
- Worktree `.worktrees/operala-serve` (greentic-dw), branch `feat/operala-serve`. greentic-dw pushes via **SSH**.
- ALWAYS prefix cargo with `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Scoped: `cargo build/test/clippy -p greentic-dw-cli`. On "No space left on device": STOP, report BLOCKED.

## Reference (read first)

- `crates/greentic-dw-cli/src/cli_types.rs` (`CliError`, `Cli`, `Command`).
- `crates/greentic-dw-cli/src/wizard.rs` lines 29–43 (the `run`/`run_from_env` dispatch) + `crates/greentic-dw-cli/src/lib.rs` (module list).
- `crates/greentic-dw-operala-bridge/src/lib.rs` (`run_bridge`, `request_topic`, `OperalaDispatchInvoker`).
- `crates/greentic-dw-operala-invoker/src/lib.rs` (`DeepWorkerInvoker::new`).
- greentic-llm exports (git checkout): `EnvCredentialSource`, `CredentialSource` (trait, async `get_credential`), `ProviderKind` (`FromStr`), `RigBackend::new`, `Credential`, `LlmProvider`.

---

## Task 1: `serve` subcommand

**Files:**
- Create: `crates/greentic-dw-cli/src/serve.rs`
- Modify: `crates/greentic-dw-cli/src/cli_types.rs` (Command + CliError), `crates/greentic-dw-cli/src/lib.rs` (mod), `crates/greentic-dw-cli/src/wizard.rs` (dispatch arm), `crates/greentic-dw-cli/Cargo.toml` (deps)

**Interfaces:**
- Produces: `Command::Serve(ServeArgs)` + `pub(crate) fn serve::run_serve(ServeArgs) -> Result<(), CliError>`.

- [ ] **Step 1: Read the references**

Read the four reference files above. Confirm the greentic-llm symbol names by opening the git checkout (find via `cargo metadata` or `~/.cargo/git/checkouts/greentic-llm-*`): `EnvCredentialSource` (unit struct?), `CredentialSource::get_credential` (async, args, return), `ProviderKind: FromStr` (Err type), `RigBackend::new(ProviderKind, &str, &Credential) -> Result<_, LlmError>`. If any name differs, adapt and report.

- [ ] **Step 2: Cargo.toml deps**

Add to `crates/greentic-dw-cli/Cargo.toml` `[dependencies]` (mirror the existing `path`/`git`/`workspace` declaration styles already in the file):
```toml
greentic-dw-operala-bridge = { path = "../greentic-dw-operala-bridge" }
greentic-dw-operala-invoker = { path = "../greentic-dw-operala-invoker" }
greentic-llm = { git = "https://github.com/greenticai/greentic-llm", tag = "v1.2.6-research" }
async-nats = "0.46"
tokio = { version = "1", features = ["rt", "rt-multi-thread"] }
anyhow = "1"
```

- [ ] **Step 3: CliError + Command variants**

In `cli_types.rs`, add to `CliError`:
```rust
    #[error("operala serve failed: {0}")]
    Serve(String),
```
Add to `Command`:
```rust
    /// Serve the operala deep-worker event bridge over NATS.
    Serve(ServeArgs),
```
Add the `ServeArgs` struct (next to `WizardArgs`):
```rust
#[derive(Debug, Clone, Args)]
pub struct ServeArgs {
    /// NATS URL (default: $GREENTIC_EVENTS_NATS_URL or nats://localhost:4222).
    #[arg(long)]
    pub nats_url: Option<String>,
    /// LLM model (default: $GREENTIC_LLM_MODEL or gpt-4o).
    #[arg(long)]
    pub model: Option<String>,
}
```
(`ServeArgs` is re-exported automatically via `pub use cli_types::*` in `lib.rs`.)

- [ ] **Step 4: Write the failing tests (TDD — RED)**

Create `crates/greentic-dw-cli/src/serve.rs` starting with the test module (helpers don't exist yet → RED):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_types::{Cli, Command};
    use clap::Parser;

    #[test]
    fn resolve_nats_url_precedence() {
        assert_eq!(resolve_nats_url(Some("nats://arg:4222"), Some("nats://env:4222".into())), "nats://arg:4222");
        assert_eq!(resolve_nats_url(None, Some("nats://env:4222".into())), "nats://env:4222");
        assert_eq!(resolve_nats_url(None, None), "nats://localhost:4222");
    }

    #[test]
    fn resolve_model_precedence() {
        assert_eq!(resolve_model(Some("m-arg"), Some("m-env".into())), "m-arg");
        assert_eq!(resolve_model(None, Some("m-env".into())), "m-env");
        assert_eq!(resolve_model(None, None), "gpt-4o");
    }

    #[test]
    fn serve_args_parse_from_clap() {
        let cli = Cli::try_parse_from([
            "greentic-dw", "serve", "--nats-url", "nats://x:4222", "--model", "m",
        ])
        .expect("parse serve");
        match cli.command {
            Command::Serve(args) => {
                assert_eq!(args.nats_url.as_deref(), Some("nats://x:4222"));
                assert_eq!(args.model.as_deref(), Some("m"));
            }
            _ => panic!("expected Serve command"),
        }
    }
}
```
NOTE: `Cli`/`Command` fields are `pub(crate)`, so this test (in the same crate) can match them. If `Command` is not importable as written, use `crate::cli_types::Command`.

- [ ] **Step 5: Run tests — verify they FAIL**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-cli serve`
Expected: FAIL (helpers not defined).

- [ ] **Step 6: Implement `serve.rs` (GREEN)**

Above the tests in `serve.rs`:
```rust
//! `greentic-dw serve` — run the operala deep-worker event bridge over NATS.
//!
//! Connects NATS, builds an LLM from env, constructs a [`DeepWorkerInvoker`],
//! and serves `greentic.operala.request.v1` forever via [`run_bridge`]. The CLI
//! is otherwise synchronous; this command owns a local Tokio runtime.

use std::sync::Arc;

use greentic_dw_operala_bridge::{OperalaDispatchInvoker, request_topic, run_bridge};
use greentic_dw_operala_invoker::DeepWorkerInvoker;
use greentic_llm::{CredentialSource, EnvCredentialSource, LlmProvider, ProviderKind, RigBackend};

use crate::cli_types::{CliError, ServeArgs};

const DEFAULT_NATS_URL: &str = "nats://localhost:4222";
const DEFAULT_MODEL: &str = "gpt-4o";
const OPERALA_RUNTIME: &str = "operala";

/// NATS URL: explicit arg, else env, else the localhost default.
fn resolve_nats_url(arg: Option<&str>, env: Option<String>) -> String {
    arg.map(str::to_string)
        .or(env)
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

/// LLM model: explicit arg, else env, else the default.
fn resolve_model(arg: Option<&str>, env: Option<String>) -> String {
    arg.map(str::to_string)
        .or(env)
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// Synchronous entry point invoked from the CLI dispatch: owns a Tokio runtime
/// and blocks on the async serve loop.
pub(crate) fn run_serve(args: ServeArgs) -> Result<(), CliError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::Serve(format!("failed to build tokio runtime: {error}")))?;
    runtime
        .block_on(serve(args))
        .map_err(|error| CliError::Serve(error.to_string()))
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let nats_url = resolve_nats_url(
        args.nats_url.as_deref(),
        std::env::var("GREENTIC_EVENTS_NATS_URL").ok(),
    );
    let model = resolve_model(args.model.as_deref(), std::env::var("GREENTIC_LLM_MODEL").ok());

    let llm = build_llm(&model).await?;
    let invoker: Arc<dyn OperalaDispatchInvoker> = Arc::new(DeepWorkerInvoker::new(llm));

    let client = async_nats::connect(&nats_url).await?;
    println!(
        "greentic-dw operala serve: listening on {} ({nats_url} / {model})",
        request_topic(OPERALA_RUNTIME)
    );
    run_bridge(client, invoker).await
}

/// Build a single LLM provider from process env (provider kind + credential).
async fn build_llm(model: &str) -> anyhow::Result<Arc<dyn LlmProvider>> {
    let kind: ProviderKind = std::env::var("GREENTIC_LLM_PROVIDER")
        .map_err(|_| anyhow::anyhow!("GREENTIC_LLM_PROVIDER is required for operala serve"))?
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid GREENTIC_LLM_PROVIDER: {error}"))?;
    let cred = EnvCredentialSource.get_credential(kind).await?;
    let backend = RigBackend::new(kind, model, &cred)?;
    Ok(Arc::new(backend))
}
```
NOTE (implementer): adapt to the real greentic-llm API confirmed in Step 1:
- If `EnvCredentialSource` is not a unit struct, construct it per its API (e.g. `EnvCredentialSource::new()` / `::default()`).
- If `ProviderKind: FromStr` Err doesn't impl `Display`, format it with `{error:?}`.
- If `get_credential` takes `&ProviderKind` or no arg, adjust. If `RigBackend::new` signature differs, adjust the call and report.
- Keep the two pure helpers and their behavior identical (the tests pin them).

- [ ] **Step 7: Wire the module + dispatch**

In `lib.rs` add `mod serve;` (after `mod cli_types;`). In `wizard.rs` `run()`, extend the match:
```rust
    match cli.command {
        Command::Wizard(wizard) => run_wizard(wizard),
        Command::Serve(args) => crate::serve::run_serve(args),
    }
```

- [ ] **Step 8: Run tests — verify they PASS**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test -p greentic-dw-cli`
Expected: all pass (the 3 new serve tests + existing wizard tests still green).

- [ ] **Step 9: clippy + fmt**

Run: `CARGO_NET_GIT_FETCH_WITH_CLI=true cargo clippy -p greentic-dw-cli --all-targets -- -D warnings` (clean); `cargo fmt -p greentic-dw-cli`.

- [ ] **Step 10: Commit**

```bash
git add crates/greentic-dw-cli Cargo.lock
git commit -m "feat(cli): operala serve subcommand running the deep-worker event bridge (deep-worker brain slice 6)"
```

---

## Manual verification

`cargo test -p greentic-dw-cli` green; `greentic-dw serve --help` lists the command; with `GREENTIC_LLM_PROVIDER`/`_API_KEY` + a NATS server, `greentic-dw serve` prints the listening banner and blocks (manual/e2e — not unit-tested).

## Self-Review (during planning)

- **Spec coverage:** §1 contract → Task 1; §2 design → Steps 2/3/6/7 (sync CLI preserved, local runtime, two pure helpers); §4 testing → Step 4 (url/model precedence + clap parse).
- **Placeholders:** none — full code in Steps 3/4/6/7. The greentic-llm adaptation notes (Step 6) are explicit with fallbacks; the only unknowns are exact greentic-llm symbol shapes (confirmed in Step 1).
- **Type consistency:** `CliError::Serve(String)`; `Command::Serve(ServeArgs)`; `run_serve -> Result<(), CliError>`; `run_bridge(async_nats::Client, Arc<dyn OperalaDispatchInvoker>)`; `DeepWorkerInvoker::new(Arc<dyn LlmProvider>)`. The wizard dispatch + main stay synchronous.
