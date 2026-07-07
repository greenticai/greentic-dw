//! LLM-backed [`DelegationProvider`] implementation for greentic-dw.
//!
//! [`LlmDelegationProvider`] wraps any [`greentic_llm::LlmProvider`] and exposes
//! it as a synchronous [`DelegationProvider`] via the [`bridge::block_on`]
//! helper.
//!
//! # Method status
//!
//! | Method            | Status                         |
//! |-------------------|--------------------------------|
//! | `choose_delegate` | Implemented (LLM-backed)       |
//! | `start_subtask`   | Implemented (deterministic)    |
//! | `merge_result`    | Implemented (deterministic)    |

pub mod bridge;
mod prompt;

use std::sync::Arc;

use greentic_dw_delegation::{
    DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult, DelegationMode,
    DelegationProvider, DelegationRequest, MergePolicy, MergeSubtaskResultRequest,
    StartSubtaskRequest, SubtaskResultEnvelope,
};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider};

/// An [`LlmDelegationProvider`] backed by a [`LlmProvider`].
///
/// `choose_delegate` calls the LLM with a structured schema prompt and
/// validates the returned [`DelegationDecision`]. `start_subtask` and
/// `merge_result` are fully deterministic — no LLM call is made.
pub struct LlmDelegationProvider {
    /// The underlying LLM provider used for delegation decision operations.
    llm: Arc<dyn LlmProvider>,
}

impl LlmDelegationProvider {
    /// Create a new provider wrapping the given `llm`.
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self { llm }
    }

    /// Return a reference to the underlying LLM provider.
    pub fn llm(&self) -> &dyn LlmProvider {
        self.llm.as_ref()
    }

    /// Send a one-shot LLM request and deserialize the JSON response into `T`.
    ///
    /// Builds a minimal `ChatRequest` with a system prompt and a single user
    /// message, calls the provider synchronously via [`bridge::block_on`], then
    /// extracts and parses the JSON from the model reply.
    ///
    /// # Errors
    ///
    /// Returns [`DelegationError::Provider`] when the LLM call fails or when the
    /// response cannot be parsed as valid JSON for `T`.
    fn complete_json<T: serde::de::DeserializeOwned>(
        &self,
        system: &str,
        user: String,
    ) -> Result<T, DelegationError> {
        let request = ChatRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            tools: vec![],
            tool_choice: None,
            max_tokens: Some(4096),
            temperature: Some(0.2),
        };

        let response = bridge::block_on(self.llm.chat(request))
            .map_err(|llm_error| DelegationError::Provider(llm_error.to_string()))?;

        let raw_json = prompt::extract_json(&response.content);

        serde_json::from_str::<T>(raw_json).map_err(|parse_error| {
            DelegationError::Provider(format!(
                "delegation LLM output not valid JSON: {parse_error}"
            ))
        })
    }

    /// Return true if `status` represents a success outcome (case-insensitive).
    fn is_success_status(status: &str) -> bool {
        let trimmed = status.trim();
        trimmed.eq_ignore_ascii_case("success")
            || trimmed.eq_ignore_ascii_case("succeeded")
            || trimmed.eq_ignore_ascii_case("completed")
    }
}

impl DelegationProvider for LlmDelegationProvider {
    /// Call the LLM to decide whether and how to delegate the goal, returning
    /// a [`DelegationDecision`].
    ///
    /// After parsing the model response, validates that any non-`None` mode
    /// names at least one target agent.
    ///
    /// # Errors
    ///
    /// - [`DelegationError::Provider`] — LLM call failed or response is not
    ///   valid JSON for [`DelegationDecision`].
    /// - [`DelegationError::Validation`] — `mode != None` but `target_agents`
    ///   is empty.
    fn choose_delegate(
        &self,
        req: DelegationRequest,
    ) -> Result<DelegationDecision, DelegationError> {
        let decision: DelegationDecision = self.complete_json(
            &prompt::system_for_choose_delegate(),
            prompt::user_for_choose_delegate(&req),
        )?;

        if decision.mode != DelegationMode::None && decision.target_agents.is_empty() {
            return Err(DelegationError::Validation(
                "delegation decision selects a mode but names no target agents".into(),
            ));
        }

        Ok(decision)
    }

    /// Deterministically construct a [`DelegationHandle`] from the subtask
    /// envelope — no LLM call is made.
    fn start_subtask(&self, req: StartSubtaskRequest) -> Result<DelegationHandle, DelegationError> {
        Ok(DelegationHandle {
            subtask_id: req.envelope.subtask_id,
            target_agent: req.envelope.target_agent,
        })
    }

    /// Merge subtask results according to `req.merge_policy` — no LLM call is
    /// made.
    ///
    /// - [`MergePolicy::FirstSuccess`]: returns the first result whose `status`
    ///   is `success`/`succeeded`/`completed` (case-insensitive) and whose
    ///   `output_artifact_ref` is non-empty. If none qualifies, returns an
    ///   empty accepted list with a descriptive summary.
    /// - All other policies: collect every result with a non-empty
    ///   `output_artifact_ref`.
    fn merge_result(
        &self,
        req: MergeSubtaskResultRequest,
    ) -> Result<DelegationMergeResult, DelegationError> {
        let has_non_empty_ref =
            |r: &SubtaskResultEnvelope| !r.output_artifact_ref.trim().is_empty();

        match req.merge_policy {
            MergePolicy::FirstSuccess => {
                match req
                    .results
                    .iter()
                    .find(|r| Self::is_success_status(&r.status) && has_non_empty_ref(r))
                {
                    Some(winner) => Ok(DelegationMergeResult {
                        accepted_artifact_refs: vec![winner.output_artifact_ref.clone()],
                        summary: format!("first successful subtask: {}", winner.subtask_id),
                    }),
                    None => Ok(DelegationMergeResult {
                        accepted_artifact_refs: vec![],
                        summary: "no successful subtask result".into(),
                    }),
                }
            }
            other => {
                let refs: Vec<String> = req
                    .results
                    .iter()
                    .filter(|r| has_non_empty_ref(r))
                    .map(|r| r.output_artifact_ref.clone())
                    .collect();
                let count = refs.len();
                Ok(DelegationMergeResult {
                    accepted_artifact_refs: refs,
                    summary: format!("merged {count} subtask result(s) under {other:?}"),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::stream;
    use greentic_llm::{
        Capabilities, ChatRequest, ChatResponse, ChatStream, FinishReason, LlmError, StreamEvent,
    };

    // ---------------------------------------------------------------------------
    // StubLlm — minimal LlmProvider for unit tests
    // ---------------------------------------------------------------------------

    struct StubLlm {
        canned_response: ChatResponse,
    }

    impl StubLlm {
        fn with_response(content: impl Into<String>) -> Self {
            Self {
                canned_response: ChatResponse {
                    content: content.into(),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                },
            }
        }
    }

    #[async_trait]
    impl LlmProvider for StubLlm {
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
            "stub"
        }

        fn model(&self) -> &str {
            "stub-model"
        }

        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                content: self.canned_response.content.clone(),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
            })
        }

        async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> {
            use futures_util::StreamExt;
            let events: Vec<Result<StreamEvent, LlmError>> = vec![
                Ok(StreamEvent::TextChunk(self.canned_response.content.clone())),
                Ok(StreamEvent::Done {
                    finish_reason: FinishReason::Stop,
                }),
            ];
            Ok(stream::iter(events).boxed())
        }
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn stub_with(content: impl Into<String>) -> LlmDelegationProvider {
        LlmDelegationProvider::new(Arc::new(StubLlm::with_response(content)))
    }

    fn delegation_request() -> DelegationRequest {
        DelegationRequest {
            goal: "Summarize the quarterly report".into(),
            candidate_agents: vec!["summarizer".into(), "analyst".into()],
        }
    }

    fn valid_decision_json() -> &'static str {
        r#"{"mode":"single","target_agents":["summarizer"],"merge_policy":"first_success","rationale":"Only one agent needed."}"#
    }

    fn subtask_envelope() -> greentic_dw_delegation::SubtaskEnvelope {
        greentic_dw_delegation::SubtaskEnvelope {
            subtask_id: "sub-001".into(),
            parent_run_id: "run-42".into(),
            correlation_id: String::new(),
            source_agent_id: String::new(),
            target_agent: "summarizer".into(),
            tool_id: String::new(),
            goal: "Summarize section 3".into(),
            context_package_ref: "ctx://run-42/context".into(),
            context_scope: Default::default(),
            expected_output_schema: "{}".into(),
            permissions_profile: "read-only".into(),
            deadline: "2026-12-31T00:00:00Z".into(),
            return_policy: Default::default(),
        }
    }

    fn result_envelope(subtask_id: &str, status: &str, artifact: &str) -> SubtaskResultEnvelope {
        SubtaskResultEnvelope {
            subtask_id: subtask_id.into(),
            correlation_id: String::new(),
            source_agent_id: String::new(),
            target_agent_id: String::new(),
            tool_id: String::new(),
            status: status.into(),
            output_artifact_ref: artifact.into(),
            output_schema_ref: String::new(),
            notes: vec![],
        }
    }

    // ---------------------------------------------------------------------------
    // choose_delegate tests
    // ---------------------------------------------------------------------------

    #[test]
    fn choose_delegate_parses_valid_decision() {
        let provider = stub_with(valid_decision_json());
        let decision = provider
            .choose_delegate(delegation_request())
            .expect("should succeed");
        assert_eq!(decision.mode, DelegationMode::Single);
        assert_eq!(decision.target_agents, vec!["summarizer"]);
    }

    #[test]
    fn choose_delegate_mode_with_empty_agents_is_validation_error() {
        let json = r#"{"mode":"single","target_agents":[],"merge_policy":"first_success","rationale":"oops"}"#;
        let provider = stub_with(json);
        let err = provider.choose_delegate(delegation_request()).unwrap_err();
        assert!(
            matches!(err, DelegationError::Validation(_)),
            "expected Validation error, got {err:?}"
        );
    }

    #[test]
    fn choose_delegate_none_mode_with_empty_agents_is_ok() {
        let json = r#"{"mode":"none","target_agents":[],"merge_policy":"collect_all","rationale":"no delegation needed"}"#;
        let provider = stub_with(json);
        let decision = provider
            .choose_delegate(delegation_request())
            .expect("None mode with empty agents should be valid");
        assert_eq!(decision.mode, DelegationMode::None);
    }

    #[test]
    fn choose_delegate_bad_json_is_provider_error() {
        let provider = stub_with("not valid json at all");
        let err = provider.choose_delegate(delegation_request()).unwrap_err();
        assert!(
            matches!(err, DelegationError::Provider(_)),
            "expected Provider error, got {err:?}"
        );
    }

    #[test]
    fn choose_delegate_handles_fenced_json() {
        let fenced = "```json\n{\"mode\":\"parallel\",\"target_agents\":[\"a\",\"b\"],\"merge_policy\":\"collect_all\",\"rationale\":\"parallel run\"}\n```";
        let provider = stub_with(fenced);
        let decision = provider
            .choose_delegate(delegation_request())
            .expect("should parse fenced JSON");
        assert_eq!(decision.mode, DelegationMode::Parallel);
    }

    // ---------------------------------------------------------------------------
    // start_subtask tests
    // ---------------------------------------------------------------------------

    #[test]
    fn start_subtask_echoes_envelope_ids() {
        let provider = stub_with("unused");
        let req = StartSubtaskRequest {
            envelope: subtask_envelope(),
        };
        let handle = provider.start_subtask(req).expect("should succeed");
        assert_eq!(handle.subtask_id, "sub-001");
        assert_eq!(handle.target_agent, "summarizer");
    }

    // ---------------------------------------------------------------------------
    // merge_result tests
    // ---------------------------------------------------------------------------

    #[test]
    fn merge_result_first_success_picks_first_success() {
        let provider = stub_with("unused");
        let req = MergeSubtaskResultRequest {
            merge_policy: MergePolicy::FirstSuccess,
            results: vec![
                result_envelope("sub-001", "failed", "artifact://sub-001/out"),
                result_envelope("sub-002", "success", "artifact://sub-002/out"),
            ],
        };
        let merged = provider.merge_result(req).expect("should succeed");
        assert_eq!(
            merged.accepted_artifact_refs,
            vec!["artifact://sub-002/out"]
        );
    }

    #[test]
    fn merge_result_first_success_skips_failed_only() {
        let provider = stub_with("unused");
        let req = MergeSubtaskResultRequest {
            merge_policy: MergePolicy::FirstSuccess,
            results: vec![result_envelope(
                "sub-001",
                "failed",
                "artifact://sub-001/out",
            )],
        };
        let merged = provider.merge_result(req).expect("should succeed");
        assert!(merged.accepted_artifact_refs.is_empty());
    }

    #[test]
    fn merge_result_first_success_accepts_succeeded_status() {
        let provider = stub_with("unused");
        let req = MergeSubtaskResultRequest {
            merge_policy: MergePolicy::FirstSuccess,
            results: vec![result_envelope(
                "sub-001",
                "succeeded",
                "artifact://sub-001/out",
            )],
        };
        let merged = provider.merge_result(req).expect("should succeed");
        assert_eq!(
            merged.accepted_artifact_refs,
            vec!["artifact://sub-001/out"]
        );
    }

    #[test]
    fn merge_result_first_success_accepts_completed_status() {
        let provider = stub_with("unused");
        let req = MergeSubtaskResultRequest {
            merge_policy: MergePolicy::FirstSuccess,
            results: vec![result_envelope(
                "sub-001",
                "COMPLETED",
                "artifact://sub-001/out",
            )],
        };
        let merged = provider.merge_result(req).expect("should succeed");
        assert_eq!(
            merged.accepted_artifact_refs,
            vec!["artifact://sub-001/out"]
        );
    }

    #[test]
    fn merge_result_collect_all_returns_all_non_empty_refs() {
        let provider = stub_with("unused");
        let req = MergeSubtaskResultRequest {
            merge_policy: MergePolicy::CollectAll,
            results: vec![
                result_envelope("sub-001", "success", "artifact://sub-001/out"),
                result_envelope("sub-002", "failed", "artifact://sub-002/out"),
                result_envelope("sub-003", "success", ""),
            ],
        };
        let merged = provider.merge_result(req).expect("should succeed");
        assert_eq!(merged.accepted_artifact_refs.len(), 2);
        assert!(
            merged
                .accepted_artifact_refs
                .contains(&"artifact://sub-001/out".to_string())
        );
        assert!(
            merged
                .accepted_artifact_refs
                .contains(&"artifact://sub-002/out".to_string())
        );
    }

    #[test]
    fn merge_result_empty_results_returns_empty() {
        let provider = stub_with("unused");
        let req = MergeSubtaskResultRequest {
            merge_policy: MergePolicy::CollectAll,
            results: vec![],
        };
        let merged = provider.merge_result(req).expect("should succeed");
        assert!(merged.accepted_artifact_refs.is_empty());
    }

    // ---------------------------------------------------------------------------
    // extract_json tests (via prompt module bridge smoke)
    // ---------------------------------------------------------------------------

    #[test]
    fn choose_delegate_handles_prose_wrapped_json() {
        let input = format!(
            "Here is my analysis:\n{}\nHope that helps.",
            valid_decision_json()
        );
        let provider = stub_with(input);
        let decision = provider
            .choose_delegate(delegation_request())
            .expect("should strip prose and parse JSON");
        assert_eq!(decision.mode, DelegationMode::Single);
    }
}
