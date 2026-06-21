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
    tracing_subscriber::fmt().try_init().ok();

    let nats_url = resolve_nats_url(
        args.nats_url.as_deref(),
        std::env::var("GREENTIC_EVENTS_NATS_URL").ok(),
    );
    let model = resolve_model(
        args.model.as_deref(),
        std::env::var("GREENTIC_LLM_MODEL").ok(),
    );

    let llm = build_llm(&model).await?;
    let invoker: Arc<dyn OperalaDispatchInvoker> = Arc::new(DeepWorkerInvoker::new(llm));

    let client = async_nats::connect(&nats_url).await?;
    println!(
        "greentic-dw operala serve: listening on {} ({nats_url} / {model})",
        request_topic(OPERALA_RUNTIME)
    );
    tokio::select! {
        result = run_bridge(client, invoker) => result,
        _ = tokio::signal::ctrl_c() => {
            println!("greentic-dw operala serve: shutdown signal received, stopping");
            Ok(())
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_types::{Cli, Command};
    use clap::Parser;

    #[test]
    fn resolve_nats_url_precedence() {
        assert_eq!(
            resolve_nats_url(Some("nats://arg:4222"), Some("nats://env:4222".into())),
            "nats://arg:4222"
        );
        assert_eq!(
            resolve_nats_url(None, Some("nats://env:4222".into())),
            "nats://env:4222"
        );
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
            "greentic-dw",
            "serve",
            "--nats-url",
            "nats://x:4222",
            "--model",
            "m",
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
