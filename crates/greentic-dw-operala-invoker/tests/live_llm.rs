//! Live-LLM smoke / prompt-tuning harness for the deep-worker loop.
//!
//! Runs the production `DeepWorkerInvoker` against a REAL provider (built from
//! `GREENTIC_LLM_PROVIDER` / `GREENTIC_LLM_API_KEY` / `GREENTIC_LLM_MODEL`),
//! exercising the full plan -> next_actions -> review loop. Prints the outcome
//! (or the error chain) so prompts can be tuned against real model output.
//!
//! Skipped unless `GREENTIC_LLM_API_KEY` is set, so CI stays green. Run with:
//!   GREENTIC_LLM_PROVIDER=deepseek GREENTIC_LLM_API_KEY=... \
//!     cargo test -p greentic-dw-operala-invoker --test live_llm -- --nocapture

use std::sync::Arc;

use greentic_dw_operala_bridge::OperalaDispatchInvoker;
use greentic_dw_operala_invoker::DeepWorkerInvoker;
use greentic_llm::{CredentialSource, EnvCredentialSource, LlmProvider, ProviderKind, RigBackend};

async fn build_live_llm() -> anyhow::Result<Arc<dyn LlmProvider>> {
    let kind: ProviderKind = std::env::var("GREENTIC_LLM_PROVIDER")
        .map_err(|_| anyhow::anyhow!("GREENTIC_LLM_PROVIDER required"))?
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid provider: {e}"))?;
    let model = std::env::var("GREENTIC_LLM_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string());
    let cred = EnvCredentialSource.get_credential(kind).await?;
    let backend = RigBackend::new(kind, &model, &cred)?;
    eprintln!("live LLM: provider={kind:?} model={model}");
    Ok(Arc::new(backend))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_deep_loop_runs_against_real_llm() {
    if std::env::var("GREENTIC_LLM_API_KEY").is_err() {
        eprintln!("SKIP live_deep_loop_runs_against_real_llm: GREENTIC_LLM_API_KEY not set");
        return;
    }

    let llm = match build_live_llm().await {
        Ok(llm) => llm,
        Err(e) => panic!("failed to build live LLM: {e:#}"),
    };
    let invoker = DeepWorkerInvoker::new(llm);

    let goal = std::env::var("LIVE_GOAL")
        .unwrap_or_else(|_| "List the top 3 benefits of writing unit tests, briefly.".to_string());
    eprintln!("dispatch goal: {goal}");

    let result = invoker
        .invoke(
            "acme",
            "default",
            "researcher",
            "run",
            serde_json::json!({ "goal": goal }),
            Some("live-run-1"),
        )
        .await;

    match result {
        Ok(outcome) => {
            let status = outcome
                .output
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            eprintln!(
                "INVOKE OK: ok={} status={status} output={}",
                outcome.ok, outcome.output
            );
            // `completed` and `delegating` are both healthy terminal states
            // (delegation emits subtasks and returns rather than completing).
            assert!(
                outcome.ok || status == "delegating",
                "deep loop ended in a non-success state: {}",
                outcome.output
            );
        }
        Err(e) => {
            // Print the full anyhow chain so we can see WHICH LLM step failed
            // (create_plan / next_actions / review_final) and tune that prompt.
            panic!("INVOKE ERR (loop failed against real LLM):\n{e:#}");
        }
    }
}
