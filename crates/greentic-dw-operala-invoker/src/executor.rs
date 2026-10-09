//! [`LlmToolStepExecutor`]: carries out a plan step by letting the model call
//! host-injected tools in a bounded loop.
//!
//! Wired by [`crate::DeepWorkerInvoker`] only when tools are present and the
//! model supports tool calling. The coordinator runs on a blocking thread, so
//! every async call here goes through the planning provider's `block_on`
//! bridge, exactly like the planner and reflector.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use greentic_dw_planning_llm::bridge::block_on;
use greentic_dw_runtime::{ExecuteStepRequest, StepExecution, StepExecutionError, StepExecutor};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider, ToolCall, ToolDef};
use serde_json::{Map, Value, json};

use crate::tools::{DeepWorkerTools, ToolSpec};

/// Largest tool result (in bytes) the model is shown; longer ones are cut.
pub(crate) const TOOL_RESULT_LIMIT: usize = 8 * 1024;

/// Rounds allowed beyond the tool-call grant. Cached calls cost no budget, so
/// without this a model repeating one cached call would never stop.
const CACHED_ROUND_ALLOWANCE: usize = 3;

const EXECUTOR_SYSTEM_PROMPT: &str = "You are a deep worker carrying out ONE step of a plan. \
Use the available tools when they help you complete the step; call only the tools you are \
offered, with arguments matching their schemas. When the step is done — or you cannot make \
further progress — answer with a concise plain-text result of the step: the facts you found \
and anything you changed. Do not describe steps other than this one.";

const BUDGET_EXHAUSTED_RESULT: &str =
    "tool call budget exhausted for this step; answer with what you have";

/// Cut `text` to at most `limit` bytes on a char boundary, with a marker.
pub(crate) fn truncate_marked(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut cut = limit;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}…[truncated {} bytes]",
        &text[..cut],
        text.len().saturating_sub(cut)
    )
}

/// JSON with object keys sorted at every depth, so equal arguments produce one
/// cache key whatever order the model wrote them in.
fn canonical_json(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = Map::new();
                for key in keys {
                    if let Some(inner) = map.get(key) {
                        out.insert(key.clone(), sorted(inner));
                    }
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(value).to_string()
}

/// How one requested tool call was resolved.
enum CallOutcome {
    Ran(Value),
    Cached(Value),
    Failed(String),
    OverBudget,
}

impl CallOutcome {
    fn status(&self) -> &'static str {
        match self {
            CallOutcome::Ran(_) => "ok",
            CallOutcome::Cached(_) => "cached",
            CallOutcome::Failed(_) => "error",
            CallOutcome::OverBudget => "skipped_budget",
        }
    }

    /// The payload the model sees as the tool result.
    fn model_payload(&self) -> Value {
        match self {
            CallOutcome::Ran(value) | CallOutcome::Cached(value) => value.clone(),
            CallOutcome::Failed(error) => json!({ "error": error }),
            CallOutcome::OverBudget => json!({ "error": BUDGET_EXHAUSTED_RESULT }),
        }
    }
}

/// Runs a plan step as a bounded tool-calling conversation. One instance
/// serves one invoke: its result cache is what keeps a re-executed step (a
/// planner revisiting it after Retry/Revise) from repeating a side effect.
pub(crate) struct LlmToolStepExecutor {
    llm: Arc<dyn LlmProvider>,
    tools: Arc<dyn DeepWorkerTools>,
    defs: Vec<ToolDef>,
    /// (tool name, canonical args) → successful result.
    cache: Mutex<HashMap<(String, String), Value>>,
}

impl LlmToolStepExecutor {
    pub(crate) fn new(
        llm: Arc<dyn LlmProvider>,
        tools: Arc<dyn DeepWorkerTools>,
        specs: &[ToolSpec],
    ) -> Self {
        let defs = specs
            .iter()
            .map(|spec| ToolDef {
                name: spec.name.clone(),
                description: spec.description.clone(),
                schema: spec.parameters.clone(),
            })
            .collect();
        Self {
            llm,
            tools,
            defs,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, key: &(String, String)) -> Result<Option<Value>, StepExecutionError> {
        let cache = self
            .cache
            .lock()
            .map_err(|_| StepExecutionError::Internal("tool result cache poisoned".into()))?;
        Ok(cache.get(key).cloned())
    }

    fn remember(&self, key: (String, String), value: Value) -> Result<(), StepExecutionError> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| StepExecutionError::Internal("tool result cache poisoned".into()))?;
        cache.insert(key, value);
        Ok(())
    }

    fn resolve_call(
        &self,
        call: &ToolCall,
        used: &mut usize,
        max_tool_calls: usize,
    ) -> Result<CallOutcome, StepExecutionError> {
        let key = (call.name.clone(), canonical_json(&call.arguments));
        if let Some(value) = self.cached(&key)? {
            return Ok(CallOutcome::Cached(value));
        }
        if *used >= max_tool_calls {
            return Ok(CallOutcome::OverBudget);
        }
        *used += 1;
        match block_on(self.tools.call(&call.name, call.arguments.clone())) {
            Ok(value) => {
                self.remember(key, value.clone())?;
                Ok(CallOutcome::Ran(value))
            }
            Err(error) => {
                tracing::warn!(tool = %call.name, %error, "deep-worker tool call failed");
                Ok(CallOutcome::Failed(format!("tool call failed: {error:#}")))
            }
        }
    }

    fn chat(&self, request: ChatRequest) -> Result<greentic_llm::ChatResponse, StepExecutionError> {
        block_on(self.llm.chat(request))
            .map_err(|error| StepExecutionError::Backend(format!("step execution chat: {error}")))
    }
}

/// The user message: goal, step, context, then the earlier steps' outputs.
pub(crate) fn executor_user_prompt(request: &ExecuteStepRequest<'_>) -> String {
    let mut prompt = format!(
        "Goal: {}\nCurrent step ({}): {}",
        request.goal, request.step.step_id, request.step.title
    );
    if let Some(context) = request.context {
        prompt.push_str("\n\nRelevant knowledge:\n");
        prompt.push_str(context);
    }
    if !request.prior_outputs.is_empty() {
        prompt.push_str("\n\nOutputs of earlier steps:");
        for output in request.prior_outputs {
            prompt.push_str("\n- ");
            prompt.push_str(&truncate_marked(&output.to_string(), TOOL_RESULT_LIMIT));
        }
    }
    if request.max_tool_calls == 0 {
        prompt
            .push_str("\n\nNo tool calls are available for this step; answer from what you have.");
    }
    prompt
}

impl StepExecutor for LlmToolStepExecutor {
    fn execute_step(
        &self,
        request: ExecuteStepRequest<'_>,
    ) -> Result<StepExecution, StepExecutionError> {
        let mut messages = vec![
            ChatMessage::system(EXECUTOR_SYSTEM_PROMPT),
            ChatMessage::user(executor_user_prompt(&request)),
        ];
        let mut used = 0usize;
        let mut rounds = 0usize;
        let mut records: Vec<Value> = Vec::new();
        let round_limit = request
            .max_tool_calls
            .saturating_add(CACHED_ROUND_ALLOWANCE);

        loop {
            let offer_tools = used < request.max_tool_calls && rounds < round_limit;
            let has_tool_history = !records.is_empty();
            // Once tools were used, keep their definitions (some providers
            // refuse tool messages without them) and forbid further calls.
            let (tools, tool_choice) = if offer_tools {
                (self.defs.clone(), Some("auto".to_string()))
            } else if has_tool_history {
                (self.defs.clone(), Some("none".to_string()))
            } else {
                (vec![], None)
            };
            let response = self.chat(ChatRequest {
                messages: messages.clone(),
                tools,
                tool_choice,
                max_tokens: Some(2048),
                temperature: Some(0.2),
            })?;
            rounds += 1;

            if !offer_tools || response.tool_calls.is_empty() {
                let mut body = json!({
                    "step_id": request.step.step_id,
                    "output": response.content.trim(),
                });
                if !records.is_empty()
                    && let Some(object) = body.as_object_mut()
                {
                    object.insert("tool_calls".to_string(), Value::Array(records));
                }
                return Ok(StepExecution {
                    body,
                    tool_calls_used: used,
                });
            }

            messages.push(ChatMessage::assistant_with_tool_calls(
                response.content.clone(),
                response.tool_calls.clone(),
            ));
            for call in &response.tool_calls {
                let outcome = self.resolve_call(call, &mut used, request.max_tool_calls)?;
                let shown =
                    truncate_marked(&outcome.model_payload().to_string(), TOOL_RESULT_LIMIT);
                records.push(json!({
                    "name": call.name,
                    "arguments": call.arguments,
                    "status": outcome.status(),
                    "result": shown,
                }));
                messages.push(ChatMessage::tool_result(call.id.clone(), shown));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_ignores_key_order() {
        let a = json!({"b": 1, "a": {"y": [1, {"d": 2, "c": 3}], "x": null}});
        let b = json!({"a": {"x": null, "y": [1, {"c": 3, "d": 2}]}, "b": 1});
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_ne!(canonical_json(&a), canonical_json(&json!({"b": 2})));
    }

    #[test]
    fn truncation_keeps_short_text_and_marks_long_text() {
        assert_eq!(truncate_marked("short", 10), "short");
        let cut = truncate_marked("ééééé", 3);
        assert_eq!(cut, "é…[truncated 8 bytes]");
    }
}
