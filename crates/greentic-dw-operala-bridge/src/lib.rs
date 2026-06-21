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
