//! Tools a host injects into a deep worker.
//!
//! The deep worker never discovers tools itself: the host (greentic-runner)
//! adapts whatever tool sources its agentic workers use to [`DeepWorkerTools`]
//! and hands the invoker one through [`crate::DeepWorkerInvoker::with_tools`].

use async_trait::async_trait;
use serde_json::Value;

/// One tool as advertised to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    /// Name the model calls the tool by. Unique within one [`DeepWorkerTools`].
    pub name: String,
    /// What the tool does, in words the model reads.
    pub description: String,
    /// JSON Schema of the tool's arguments.
    pub parameters: Value,
}

/// A host-provided tool source.
///
/// `call` returns `Ok` for every outcome the MODEL should see — including a
/// tool-level failure, which is reported as `Ok(json!({"error": "…"}))` so the
/// model can react to it. `Err` means the call could not be made at all
/// (transport, host misconfiguration); the executor records it in the step
/// body as an error and never fails the run over it.
#[async_trait]
pub trait DeepWorkerTools: Send + Sync {
    /// The tools on offer. An empty list leaves the deep worker tool-less.
    fn list(&self) -> Vec<ToolSpec>;

    /// Run the tool `name` with `args`.
    async fn call(&self, name: &str, args: Value) -> anyhow::Result<Value>;
}
