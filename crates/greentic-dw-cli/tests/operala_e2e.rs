//! End-to-end smoke for the operala deep-worker dispatch path over real NATS.
//!
//! Spawns a real `nats-server`, runs the bridge (`run_bridge`) with a production
//! [`DeepWorkerInvoker`] backed by a *scripted* LLM, publishes a
//! `greentic.operala.request.v1` message, and asserts a
//! `greentic.operala.response.v1` comes back with `ok:true` (the deep loop ran
//! to completion) and the correlation id echoed.
//!
//! This exercises the FULL wire path — flow node → NATS → serve/bridge →
//! DeepWorkerInvoker → DeepLoopCoordinator (+5 providers) → NATS — without
//! needing a real LLM key (the scripted LLM returns valid plan/review JSON).
//!
//! Skipped gracefully when `nats-server` is not on PATH so CI without it stays
//! green; run locally with `cargo test -p greentic-dw-cli --test operala_e2e`.

use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use greentic_dw_operala_bridge::wire::{
    DispatchMode, RuntimeDispatchRequest, RuntimeDispatchResponse,
};
use greentic_dw_operala_bridge::{
    OperalaDispatchInvoker, RUNTIME_NAME, request_topic, response_topic, run_bridge,
};
use greentic_dw_operala_invoker::DeepWorkerInvoker;
use greentic_dw_planning::{PlanDocument, PlanStatus};
use greentic_dw_reflection::{ReviewOutcome, ReviewVerdict};
use greentic_llm::{
    Capabilities, ChatRequest, ChatResponse, ChatStream, FinishReason, LlmError, LlmProvider,
    StreamEvent,
};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Scripted LLM: returns queued responses in order, one per chat() call.
// Order for one deep-loop run: create_plan -> next_actions -> review_final.
// ---------------------------------------------------------------------------
struct ScriptedLlm {
    responses: Mutex<std::collections::VecDeque<String>>,
}
impl ScriptedLlm {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}
#[async_trait]
impl LlmProvider for ScriptedLlm {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            chat: true,
            tools: false,
            streaming: false,
            vision: false,
            system_prompt: true,
        }
    }
    fn provider_name(&self) -> &'static str {
        "scripted"
    }
    fn model(&self) -> &str {
        "scripted-model"
    }
    async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
        let content = self
            .responses
            .lock()
            .expect("lock")
            .pop_front()
            .unwrap_or_default();
        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
        })
    }
    async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> {
        Ok(stream::iter(vec![Ok(StreamEvent::Done {
            finish_reason: FinishReason::Stop,
        })])
        .boxed())
    }
}

/// Kills the spawned nats-server when dropped (covers panics).
struct NatsGuard(Child);
impl Drop for NatsGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn nats_server_available() -> bool {
    Command::new("nats-server")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

/// Minimal plan whose loop completes deterministically: no steps ->
/// next_actions returns [] -> evaluate_completion is Satisfied (vacuous) ->
/// review_final accepts -> Completed. success_criteria must be non-empty
/// (validate_plan rejects empty).
fn scripted_plan_json() -> String {
    let plan = PlanDocument {
        plan_id: "e2e-plan".into(),
        goal: "e2e smoke".into(),
        status: PlanStatus::Active,
        revision: 1,
        assumptions: vec![],
        constraints: vec![],
        success_criteria: vec!["task completed".into()],
        steps: vec![],
        edges: vec![],
        metadata: BTreeMap::new(),
    };
    serde_json::to_string(&plan).expect("serialize plan")
}

fn scripted_review_json() -> String {
    let review = ReviewOutcome {
        verdict: ReviewVerdict::Accept,
        score: Some(1.0),
        findings: vec![],
        suggested_actions: vec![],
        binding: false,
    };
    serde_json::to_string(&review).expect("serialize review")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn operala_dispatch_round_trips_over_nats() {
    if !nats_server_available() {
        eprintln!("SKIP operala_dispatch_round_trips_over_nats: nats-server not on PATH");
        return;
    }

    // 1. start a real nats-server on an ephemeral port
    let port = free_port();
    let url = format!("nats://127.0.0.1:{port}");
    let child = Command::new("nats-server")
        .args(["-a", "127.0.0.1", "-p", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nats-server");
    let _nats = NatsGuard(child);

    // 2. wait for it to accept connections (the bridge client doubles as readiness)
    let bridge_client = {
        let mut connected = None;
        for _ in 0..100 {
            if let Ok(c) = async_nats::connect(&url).await {
                connected = Some(c);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        connected.expect("nats-server did not accept connections")
    };
    let test_client = async_nats::connect(&url)
        .await
        .expect("test client connect");

    // 3. production invoker backed by a scripted LLM (loop will complete)
    let llm: Arc<dyn LlmProvider> = Arc::new(ScriptedLlm::new(vec![
        scripted_plan_json(),
        "[]".into(),
        scripted_review_json(),
    ]));
    let invoker: Arc<dyn OperalaDispatchInvoker> = Arc::new(DeepWorkerInvoker::new(llm));

    // 4. run the bridge against NATS in the background
    tokio::spawn(async move {
        let _ = run_bridge(bridge_client, invoker).await;
    });

    // 5. subscribe to responses, then give the bridge time to subscribe to requests
    let mut sub = test_client
        .subscribe(response_topic(RUNTIME_NAME))
        .await
        .expect("subscribe response subject");
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // 6. publish one operala.call dispatch with the runtime headers
    let req = RuntimeDispatchRequest {
        target: "researcher".into(),
        operation: "run".into(),
        mode: DispatchMode::Await,
        input: serde_json::json!({ "goal": "summarize the e2e smoke" }),
        deadline_ms: Some(30_000),
    };
    let mut headers = async_nats::HeaderMap::new();
    headers.insert("Greentic-Correlation-Id", "e2e-1");
    headers.insert("Greentic-Idempotency-Key", "e2e-run-1");
    headers.insert("Greentic-Tenant", "acme");
    headers.insert("Greentic-Env", "default");
    test_client
        .publish_with_headers(
            request_topic(RUNTIME_NAME),
            headers,
            serde_json::to_vec(&req).expect("serialize request").into(),
        )
        .await
        .expect("publish request");
    test_client.flush().await.expect("flush");

    // 7. await + assert the response
    let msg = tokio::time::timeout(Duration::from_secs(30), sub.next())
        .await
        .expect("timed out waiting for operala response over NATS")
        .expect("response stream closed without a message");

    let resp: RuntimeDispatchResponse =
        serde_json::from_slice(&msg.payload).expect("decode RuntimeDispatchResponse");
    assert!(
        resp.ok,
        "expected ok:true (deep loop completed), got error={:?} output={}",
        resp.error, resp.output
    );

    // correlation id echoed verbatim by the bridge
    let correlation = msg
        .headers
        .as_ref()
        .and_then(|h| h.get("Greentic-Correlation-Id"))
        .map(|v| v.as_str().to_string());
    assert_eq!(
        correlation.as_deref(),
        Some("e2e-1"),
        "bridge must echo the correlation id"
    );
}
