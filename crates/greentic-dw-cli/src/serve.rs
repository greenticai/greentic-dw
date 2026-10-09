//! `greentic-dw serve` — run the operala deep-worker event bridge over NATS.
//!
//! Connects NATS, builds an LLM from env, constructs a [`DeepWorkerInvoker`],
//! and serves `greentic.operala.request.v1` forever via [`run_bridge`]. The CLI
//! is otherwise synchronous; this command owns a local Tokio runtime.
//!
//! When `--port <p>` is supplied a minimal HTTP `/healthz` probe is started on
//! `127.0.0.1:<p>` *before* the bridge loop so the caller can poll for
//! readiness. The probe returns `200 ok` for `GET /healthz` and `404` for
//! everything else. No additional dependencies are required — the server is
//! hand-rolled over a [`tokio::net::TcpListener`].

use std::sync::Arc;

use greentic_dw_operala_bridge::{OperalaDispatchInvoker, request_topic, run_bridge};
use greentic_dw_operala_invoker::DeepWorkerInvoker;
use greentic_llm::{CredentialSource, EnvCredentialSource, LlmProvider, ProviderKind, RigBackend};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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

    // Start the /healthz probe before the bridge loop so callers can poll for
    // readiness as soon as NATS is connected and the LLM is built.
    if let Some(port) = args.port {
        let addr = format!("127.0.0.1:{port}");
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to bind healthz listener on {addr}: {e}"))?;
        tracing::info!("greentic-dw healthz probe listening on http://{addr}/healthz");
        tokio::spawn(serve_healthz(listener));
    }

    tokio::select! {
        result = run_bridge(client, invoker) => result,
        _ = tokio::signal::ctrl_c() => {
            println!("greentic-dw operala serve: shutdown signal received, stopping");
            Ok(())
        }
    }
}

/// Accept-loop for the minimal /healthz HTTP probe.
///
/// Runs until the future is dropped (process exit or ctrl_c). Each accepted
/// connection is dispatched to [`handle_healthz_conn`] in its own task so a
/// slow client never blocks the probe.
async fn serve_healthz(listener: TcpListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                tokio::spawn(handle_healthz_conn(stream));
            }
            Err(error) => {
                tracing::warn!("healthz accept error: {error}");
            }
        }
    }
}

/// Handle one HTTP connection for the /healthz probe.
///
/// Reads up to 1 KiB of the request, inspects the request-line, and writes
/// either a `200 ok` or `404 Not Found` response. Uses `Connection: close` and
/// an explicit `Content-Length` so standard HTTP clients parse the response
/// without relying on connection close for framing.
async fn handle_healthz_conn(mut stream: TcpStream) {
    let mut buf = [0u8; 1024];
    let n = match stream.read(&mut buf).await {
        Ok(n) => n,
        Err(error) => {
            tracing::warn!("healthz connection read error: {error}");
            return;
        }
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let response: &[u8] = if first_line.starts_with("GET /healthz ") {
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
    } else {
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    };
    if let Err(error) = stream.write_all(response).await {
        tracing::warn!("healthz connection write error: {error}");
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

    /// Clap parses --port 8123 into ServeArgs.port == Some(8123).
    #[test]
    fn serve_args_parse_port() {
        let cli = Cli::try_parse_from(["greentic-dw", "serve", "--port", "8123"])
            .expect("parse serve with --port");
        match cli.command {
            Command::Serve(args) => {
                assert_eq!(args.port, Some(8123_u16));
            }
            _ => panic!("expected Serve command"),
        }
    }

    /// When --port is absent, ServeArgs.port is None (backward-compatible).
    #[test]
    fn serve_args_port_absent_is_none() {
        let cli =
            Cli::try_parse_from(["greentic-dw", "serve"]).expect("parse serve without --port");
        match cli.command {
            Command::Serve(args) => {
                assert_eq!(args.port, None);
            }
            _ => panic!("expected Serve command"),
        }
    }

    /// Starting the healthz server and sending GET /healthz returns 200.
    #[tokio::test]
    async fn healthz_returns_200_for_healthz_path() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        tokio::spawn(serve_healthz(listener));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("connect to healthz server");
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .expect("send request");
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf).await.expect("read response");
        let response = std::str::from_utf8(&buf[..n]).expect("utf8 response");
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "expected 200, got: {response}"
        );
        assert!(response.contains("ok"), "body must contain \"ok\"");
    }

    /// Sending a non-/healthz path returns 404.
    #[tokio::test]
    async fn healthz_returns_404_for_unknown_path() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        tokio::spawn(serve_healthz(listener));
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("connect to healthz server");
        stream
            .write_all(b"GET /status HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .expect("send request");
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf).await.expect("read response");
        let response = std::str::from_utf8(&buf[..n]).expect("utf8 response");
        assert!(
            response.starts_with("HTTP/1.1 404"),
            "expected 404, got: {response}"
        );
    }
}
