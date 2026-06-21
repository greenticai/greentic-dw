//! Event bridge: consume `greentic.operala.request.v1` from NATS, invoke the
//! local deep-worker runtime via the [`OperalaDispatchInvoker`] seam, and
//! publish `greentic.operala.response.v1` echoing the correlation id.
//!
//! Runtime-side counterpart of the runner's `operala.call` flow node. greentic-dw
//! has no `greentic-types` dependency, so the wire contract is MIRRORED here
//! (byte-compatible with `greentic_types::runtime_dispatch`), like sorx-event-bridge.

use std::sync::Arc;

use anyhow::Result;
use async_nats::HeaderMap;
use async_trait::async_trait;
use serde_json::Value;

use wire::{DispatchError, RuntimeDispatchRequest, RuntimeDispatchResponse};
pub use wire::{request_topic, response_topic};

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
        .invoke(
            tenant,
            env,
            &req.target,
            &req.operation,
            req.input,
            idempotency_key,
        )
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
        .publish_with_headers(
            response_topic(RUNTIME_NAME),
            out_headers,
            response_bytes.into(),
        )
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

#[cfg(test)]
mod bridge_tests {
    use super::wire::*;
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[allow(clippy::type_complexity)]
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
        let invoker = Arc::new(StubInvoker {
            ok: true,
            seen: Mutex::new(vec![]),
        });
        let resp = build_response(
            invoker.clone(),
            "acme",
            "prod",
            Some("corr-1"),
            sample_request(),
        )
        .await;
        assert!(resp.ok);
        assert_eq!(resp.output, json!({ "reply": "planned" }));
        assert!(resp.error.is_none());
        let seen = invoker.seen.lock().unwrap();
        assert_eq!(seen[0].0, "researcher");
        assert_eq!(seen[0].3.as_deref(), Some("corr-1"));
    }

    #[tokio::test]
    async fn build_response_maps_error() {
        let invoker = Arc::new(StubInvoker {
            ok: false,
            seen: Mutex::new(vec![]),
        });
        let resp = build_response(invoker, "acme", "prod", None, sample_request()).await;
        assert!(!resp.ok);
        let err = resp.error.expect("error present");
        assert_eq!(err.code, "invoke_failed");
        assert!(err.message.contains("deep loop blew up"));
    }
}

#[cfg(test)]
mod wire_tests {
    use super::RUNTIME_NAME;
    use super::wire::*;
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
