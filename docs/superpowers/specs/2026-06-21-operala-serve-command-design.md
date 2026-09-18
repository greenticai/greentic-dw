# `greentic-dw serve` operala command (Design) — deep-worker brain, slice 6 (runner serve spawn / SP-3.3)

- **Date:** 2026-06-21
- **Status:** Design approved (user chose "Runner serve spawn (3.3)"), ready for planning
- **Surface:** greentic-dw (`greentic-dw-cli` crate — new `serve` subcommand).
- **Part of:** SP-3 deep-worker brain, slice 6 — the executable entry point. Connects NATS, builds an LLM, constructs the slice-5 `DeepWorkerInvoker`, and runs the operala event bridge so `operala.call` flow nodes (runner side, wire contract from PR #81) actually reach the deep loop.

## 1. Contract (verified)

- `greentic_dw_operala_bridge::run_bridge(client: async_nats::Client, invoker: Arc<dyn OperalaDispatchInvoker>) -> anyhow::Result<()>` — subscribes `greentic.operala.request.v1`, serves forever (one spawned task per message).
- `greentic_dw_operala_invoker::DeepWorkerInvoker::new(llm: Arc<dyn greentic_llm::LlmProvider>) -> Self` (impl `OperalaDispatchInvoker`).
- LLM construction (greentic-llm, verified live source): `EnvCredentialSource` + the `CredentialSource` trait's async `get_credential(ProviderKind) -> Result<Credential, _>`; `RigBackend::new(kind: ProviderKind, model: &str, cred: &Credential) -> Result<RigBackend, LlmError>`; `Arc::new(backend) as Arc<dyn LlmProvider>`. Env: `GREENTIC_LLM_PROVIDER` (→ `ProviderKind::from_str`), `GREENTIC_LLM_API_KEY`, optional `GREENTIC_LLM_BASE_URL`/`_API_VERSION`/`_AWS_PROFILE`. **No** built-in `GREENTIC_LLM_MODEL` — we read it ourselves (default `gpt-4o`).
- `async-nats = "0.46"`; `async_nats::connect(url).await`.
- CLI today: `greentic-dw-cli` is **synchronous** (`fn run_from_env() -> Result<(), CliError>` → `run()` → `match Command`), clap-derive, single `Wizard` subcommand. `CliError` is a thiserror enum (`cli_types.rs`).

## 2. Design

**Blast-radius-minimal:** keep `main`/`run`/`run_from_env` synchronous (do NOT make the whole CLI async — the wizard path stays untouched). The new `Serve` arm builds a local Tokio runtime and `block_on`s the async serve. All async is confined to the serve module.

New file `crates/greentic-dw-cli/src/serve.rs`:
```rust
#[derive(Debug, Clone, clap::Args)]
pub struct ServeArgs {
    /// NATS URL (default: $GREENTIC_EVENTS_NATS_URL or nats://localhost:4222).
    #[arg(long)] pub nats_url: Option<String>,
    /// LLM model (default: $GREENTIC_LLM_MODEL or gpt-4o).
    #[arg(long)] pub model: Option<String>,
}

// pure, unit-tested helpers
fn resolve_nats_url(arg: Option<&str>, env: Option<String>) -> String   // arg > env > "nats://localhost:4222"
fn resolve_model(arg: Option<&str>, env: Option<String>) -> String      // arg > env > "gpt-4o"

// sync wrapper called from the dispatch match
pub(crate) fn run_serve(args: ServeArgs) -> Result<(), CliError> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()
        .map_err(|e| CliError::Serve(e.to_string()))?;
    runtime.block_on(serve(args)).map_err(|e| CliError::Serve(e.to_string()))
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let nats_url = resolve_nats_url(args.nats_url.as_deref(), std::env::var("GREENTIC_EVENTS_NATS_URL").ok());
    let model = resolve_model(args.model.as_deref(), std::env::var("GREENTIC_LLM_MODEL").ok());
    let llm = build_llm(&model).await?;
    let invoker: Arc<dyn OperalaDispatchInvoker> = Arc::new(DeepWorkerInvoker::new(llm));
    let client = async_nats::connect(&nats_url).await?;
    println!("greentic-dw operala serve: listening on {} ({} / {})", request_topic("operala"), nats_url, model);
    run_bridge(client, invoker).await
}

async fn build_llm(model: &str) -> anyhow::Result<Arc<dyn LlmProvider>> {
    let kind: ProviderKind = std::env::var("GREENTIC_LLM_PROVIDER")
        .map_err(|_| anyhow::anyhow!("GREENTIC_LLM_PROVIDER is required for operala serve"))?
        .parse().map_err(|e| anyhow::anyhow!("invalid GREENTIC_LLM_PROVIDER: {e}"))?;
    let cred = EnvCredentialSource.get_credential(kind).await?;
    let backend = RigBackend::new(kind, model, &cred)?;
    Ok(Arc::new(backend))
}
```

Wiring:
- `cli_types.rs`: add `Serve(ServeArgs)` to `Command`; add `#[error("operala serve failed: {0}")] Serve(String)` to `CliError`. Re-export `ServeArgs` (the module is `pub use cli_types::*`).
- `lib.rs`: `mod serve;`.
- `wizard.rs` `run()` dispatch: add `Command::Serve(args) => crate::serve::run_serve(args),`.
- `Cargo.toml` (`greentic-dw-cli`): add deps `greentic-dw-operala-bridge`/`-invoker` (path), `greentic-llm` (git tag v1.2.6-research), `async-nats = "0.46"`, `tokio = { features = ["rt","rt-multi-thread"] }`, `anyhow`.

## 3. Error handling

`run_serve` is the boundary: runtime-build, connect, LLM-build, and `run_bridge` errors all map to `CliError::Serve(String)`. Inside `serve`/`build_llm`, errors propagate via `anyhow` `?`. No `unwrap`/`expect` in non-test code. `GREENTIC_LLM_PROVIDER` missing → a clear `Serve` error (the operator must configure it).

## 4. Testing

Unit (pure helpers, no infra):
- `resolve_nats_url`: arg wins over env; env wins over default; none → `nats://localhost:4222`.
- `resolve_model`: arg > env > `gpt-4o`.
- `ServeArgs` parses via clap (`Cli::try_parse_from(["greentic-dw","serve","--nats-url","nats://x:4222","--model","m"])` → the Serve variant with those values).

`serve`/`build_llm`/`run_bridge` need a live NATS + LLM creds, so they are **manual/e2e** (documented), not unit-tested. The slice-5 invoker already has the loop e2e-tested; this slice is thin wiring whose correctness is compile-time + the helper units + manual run.

## 5. Limitations

- One concrete provider from env (`RigBackend`); no multi-provider routing or per-tenant keys (operator sets process env). Future: per-dispatch LLM selection.
- No tracing-subscriber init — `run_bridge`'s per-message `tracing::error!` is dropped without a subscriber; the startup banner is a plain `println!`. A subscriber is a future enhancement.
- `serve` blocks until NATS closes (intended for a long-running process); no graceful-shutdown signal handling yet.
- Runner side (`operala.call` node → NATS) is already contracted (PR #81); this slice is the greentic-dw responder. Remaining SP-3: designer deep-worker authoring (3.4), live-LLM prompt tuning.
