//! LLM-backed [`ReflectionProvider`] implementation for greentic-dw.
//!
//! [`LlmReflectionProvider`] wraps any [`greentic_llm::LlmProvider`] and exposes
//! it as a synchronous [`ReflectionProvider`] via the [`bridge::block_on`]
//! helper.
//!
//! # Method status
//!
//! | Method         | Status                   |
//! |----------------|--------------------------|
//! | `review_step`  | Implemented (LLM-backed) |
//! | `review_plan`  | Implemented (LLM-backed) |
//! | `review_final` | Implemented (LLM-backed) |

pub mod bridge;
mod prompt;

use std::sync::Arc;

use greentic_dw_reflection::{
    ReflectionError, ReflectionProvider, ReviewFinalRequest, ReviewOutcome, ReviewPlanRequest,
    ReviewStepRequest,
};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider};

/// An [`LlmReflectionProvider`] backed by a [`LlmProvider`].
///
/// All three review methods call the LLM with a structured schema prompt and
/// validate the returned [`ReviewOutcome`] before returning it to the caller.
pub struct LlmReflectionProvider {
    /// The underlying LLM provider used for generative review operations.
    llm: Arc<dyn LlmProvider>,
}

impl LlmReflectionProvider {
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
    /// Returns [`ReflectionError::Provider`] when the LLM call fails or when the
    /// response cannot be parsed as valid JSON for `T`.
    fn complete_json<T: serde::de::DeserializeOwned>(
        &self,
        system: &str,
        user: String,
    ) -> Result<T, ReflectionError> {
        let request = ChatRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            tools: vec![],
            tool_choice: None,
            max_tokens: Some(4096),
            temperature: Some(0.2),
        };

        let response = bridge::block_on(self.llm.chat(request))
            .map_err(|llm_error| ReflectionError::Provider(llm_error.to_string()))?;

        let raw_json = prompt::extract_json(&response.content);

        serde_json::from_str::<T>(raw_json).map_err(|parse_error| {
            ReflectionError::Provider(format!(
                "reflection LLM output not valid JSON: {parse_error}"
            ))
        })
    }
}

impl ReflectionProvider for LlmReflectionProvider {
    /// Call the LLM to review a plan step's output, returning a [`ReviewOutcome`].
    ///
    /// The outcome is validated with [`ReviewOutcome::validate`] before being
    /// returned so callers always receive a structurally sound assessment.
    fn review_step(&self, req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
        let outcome: ReviewOutcome = self.complete_json(
            &prompt::system_for_review_step(),
            prompt::user_for_review_step(&req),
        )?;
        outcome.validate()?;
        Ok(outcome)
    }

    /// Call the LLM to review a plan revision, returning a [`ReviewOutcome`].
    ///
    /// The outcome is validated with [`ReviewOutcome::validate`] before being
    /// returned so callers always receive a structurally sound assessment.
    fn review_plan(&self, req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> {
        let outcome: ReviewOutcome = self.complete_json(
            &prompt::system_for_review_plan(),
            prompt::user_for_review_plan(&req),
        )?;
        outcome.validate()?;
        Ok(outcome)
    }

    /// Call the LLM to review the final output, returning a [`ReviewOutcome`].
    ///
    /// The outcome is validated with [`ReviewOutcome::validate`] before being
    /// returned so callers always receive a structurally sound assessment.
    fn review_final(&self, req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> {
        let outcome: ReviewOutcome = self.complete_json(
            &prompt::system_for_review_final(),
            prompt::user_for_review_final(&req),
        )?;
        outcome.validate()?;
        Ok(outcome)
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

    fn stub_with(content: impl Into<String>) -> LlmReflectionProvider {
        LlmReflectionProvider::new(Arc::new(StubLlm::with_response(content)))
    }

    fn review_step_request() -> ReviewStepRequest {
        ReviewStepRequest {
            plan_step_id: "step-1".into(),
            output_artifact_ref: "artifact://step-1/output".into(),
            context: None,
        }
    }

    fn review_plan_request() -> ReviewPlanRequest {
        ReviewPlanRequest {
            plan_id: "plan-42".into(),
            revision: 1,
        }
    }

    fn review_final_request() -> ReviewFinalRequest {
        ReviewFinalRequest {
            run_id: "run-99".into(),
            output_artifact_ref: "artifact://run-99/final".into(),
            context: None,
        }
    }

    fn valid_accept_json() -> &'static str {
        r#"{"verdict":"accept","findings":[],"suggested_actions":[],"binding":false}"#
    }

    // ---------------------------------------------------------------------------
    // review_step tests
    // ---------------------------------------------------------------------------

    #[test]
    fn review_step_parses_valid_outcome() {
        let provider = stub_with(valid_accept_json());
        let outcome = provider
            .review_step(review_step_request())
            .expect("should succeed");
        use greentic_dw_reflection::ReviewVerdict;
        assert_eq!(outcome.verdict, ReviewVerdict::Accept);
    }

    #[test]
    fn review_step_invalid_score_is_validation_error() {
        // score 1.5 is outside [0.0, 1.0] — ReviewOutcome::validate rejects it
        let json = r#"{"verdict":"revise","score":1.5,"findings":[],"suggested_actions":[],"binding":false}"#;
        let provider = stub_with(json);
        let err = provider.review_step(review_step_request()).unwrap_err();
        assert!(
            matches!(err, ReflectionError::Validation(_)),
            "expected Validation error, got {err:?}"
        );
    }

    #[test]
    fn review_step_bad_json_is_provider_error() {
        let provider = stub_with("nope");
        let err = provider.review_step(review_step_request()).unwrap_err();
        assert!(
            matches!(err, ReflectionError::Provider(_)),
            "expected Provider error, got {err:?}"
        );
    }

    #[test]
    fn review_step_handles_fenced_json() {
        let fenced = "```json\n{\"verdict\":\"fail\",\"findings\":[],\"suggested_actions\":[],\"binding\":false}\n```";
        let provider = stub_with(fenced);
        let outcome = provider
            .review_step(review_step_request())
            .expect("should parse fenced JSON");
        use greentic_dw_reflection::ReviewVerdict;
        assert_eq!(outcome.verdict, ReviewVerdict::Fail);
    }

    // ---------------------------------------------------------------------------
    // review_plan tests
    // ---------------------------------------------------------------------------

    #[test]
    fn review_plan_parses_valid_outcome() {
        let provider = stub_with(valid_accept_json());
        let outcome = provider
            .review_plan(review_plan_request())
            .expect("should succeed");
        use greentic_dw_reflection::ReviewVerdict;
        assert_eq!(outcome.verdict, ReviewVerdict::Accept);
    }

    // ---------------------------------------------------------------------------
    // review_final tests
    // ---------------------------------------------------------------------------

    #[test]
    fn review_final_parses_valid_outcome() {
        let provider = stub_with(valid_accept_json());
        let outcome = provider
            .review_final(review_final_request())
            .expect("should succeed");
        use greentic_dw_reflection::ReviewVerdict;
        assert_eq!(outcome.verdict, ReviewVerdict::Accept);
    }

    // ---------------------------------------------------------------------------
    // extract_json tests (via the public prompt module)
    // ---------------------------------------------------------------------------

    #[test]
    fn review_step_handles_prose_wrapped_json() {
        let input = "Here is the assessment:\n{\"verdict\":\"retry\",\"findings\":[],\"suggested_actions\":[],\"binding\":false}\nDone.";
        let provider = stub_with(input);
        let outcome = provider
            .review_step(review_step_request())
            .expect("should strip prose and parse JSON");
        use greentic_dw_reflection::ReviewVerdict;
        assert_eq!(outcome.verdict, ReviewVerdict::Retry);
    }
}
