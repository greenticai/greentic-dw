//! LLM-backed [`ContextProvider`] for greentic-dw.
//!
//! `build_context` assembles a package from fragment refs deterministically.
//! `compress_context` / `summarize_context` call the LLM and store the reply
//! text as an artifact in a [`WorkspaceProvider`], returning the artifact ref.

pub mod bridge;
mod prompt;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use greentic_dw_context::{
    BuildContextRequest, CompressContextRequest, CompressedContext, ContextError, ContextFragment,
    ContextFragmentKind, ContextPackage, ContextProvider, SummarizeContextRequest,
    SummaryArtifactRef, validate_context_package,
};
use greentic_dw_workspace::{
    ArtifactKind, ArtifactMetadata, ArtifactRef, CreateArtifactRequest, WorkspaceProvider,
    WorkspaceScope,
};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider};

/// An LLM-backed [`ContextProvider`]. Compression/summarization results are
/// persisted to `workspace` under `scope`.
pub struct LlmContextProvider {
    llm: Arc<dyn LlmProvider>,
    workspace: Arc<dyn WorkspaceProvider>,
    scope: WorkspaceScope,
    counter: AtomicU64,
}

impl LlmContextProvider {
    /// Create a provider over the given LLM, workspace store, and run scope.
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        workspace: Arc<dyn WorkspaceProvider>,
        scope: WorkspaceScope,
    ) -> Self {
        Self {
            llm,
            workspace,
            scope,
            counter: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }

    /// One-shot LLM call returning the reply text verbatim (no JSON parse).
    fn complete_text(&self, system: &str, user: String) -> Result<String, ContextError> {
        let request = ChatRequest {
            messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
            tools: vec![],
            tool_choice: None,
            max_tokens: Some(2048),
            temperature: Some(0.2),
        };
        let response = bridge::block_on(self.llm.chat(request))
            .map_err(|llm_error| ContextError::Provider(llm_error.to_string()))?;
        Ok(response.content)
    }

    /// Persist `body` as a new artifact under the provider's scope.
    fn write_artifact(
        &self,
        artifact_id: &str,
        kind: ArtifactKind,
        title: &str,
        body: String,
    ) -> Result<(), ContextError> {
        let request = CreateArtifactRequest {
            artifact: ArtifactRef {
                artifact_id: artifact_id.to_string(),
                kind,
                scope: self.scope.clone(),
            },
            metadata: ArtifactMetadata {
                title: title.to_string(),
                tags: vec![],
                mime_type: Some("text/plain".to_string()),
            },
            body,
        };
        self.workspace
            .create_artifact(request)
            .map_err(|e| ContextError::Provider(format!("workspace write failed: {e}")))?;
        Ok(())
    }
}

impl ContextProvider for LlmContextProvider {
    fn build_context(&self, req: BuildContextRequest) -> Result<ContextPackage, ContextError> {
        let take = (req.budget.max_fragments as usize).min(req.fragment_refs.len());
        let fragments: Vec<ContextFragment> = req
            .fragment_refs
            .iter()
            .take(take)
            .enumerate()
            .map(|(i, content_ref)| ContextFragment {
                fragment_id: format!("frag-{i}"),
                kind: ContextFragmentKind::WorkspaceArtifact,
                content_ref: content_ref.clone(),
                content: None,
                provenance: "build_context".to_string(),
                ordinal: i as u32,
            })
            .collect();
        let package = ContextPackage {
            package_id: format!("context-{}", self.next_id()),
            fragments,
            budget: req.budget,
        };
        validate_context_package(&package)?;
        Ok(package)
    }

    fn compress_context(
        &self,
        req: CompressContextRequest,
    ) -> Result<CompressedContext, ContextError> {
        validate_context_package(&req.package)?;
        let rendered = prompt::render_package(&req.package);
        let text = self.complete_text(
            &prompt::system_for_compress(),
            prompt::user_for_compress(&rendered),
        )?;
        let artifact_id = format!("{}::compressed::{}", req.package.package_id, self.next_id());
        self.write_artifact(
            &artifact_id,
            ArtifactKind::PromptFragment,
            "compressed context",
            text,
        )?;
        Ok(CompressedContext {
            source_package_id: req.package.package_id.clone(),
            compressed_artifact_ref: artifact_id,
            fragment_count: req.package.fragments.len() as u32,
        })
    }

    fn summarize_context(
        &self,
        req: SummarizeContextRequest,
    ) -> Result<SummaryArtifactRef, ContextError> {
        validate_context_package(&req.package)?;
        let rendered = prompt::render_package(&req.package);
        let text = self.complete_text(
            &prompt::system_for_summarize(),
            prompt::user_for_summarize(&rendered),
        )?;
        let artifact_id = format!("{}::summary::{}", req.package.package_id, self.next_id());
        self.write_artifact(
            &artifact_id,
            ArtifactKind::ReportSection,
            "context summary",
            text,
        )?;
        Ok(SummaryArtifactRef {
            artifact_ref: artifact_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::stream;
    use greentic_dw_context::*;
    use greentic_dw_workspace::{ReadArtifactRequest, WorkspaceProvider, WorkspaceScope};
    use greentic_dw_workspace_mem::InMemoryWorkspaceProvider;
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
    // FailingLlm — always returns an error from chat()
    // ---------------------------------------------------------------------------

    struct FailingLlm;

    #[async_trait]
    impl LlmProvider for FailingLlm {
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
            "failing"
        }

        fn model(&self) -> &str {
            "failing-model"
        }

        async fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, LlmError> {
            Err(LlmError::Other(anyhow::anyhow!("boom")))
        }

        async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, LlmError> {
            Err(LlmError::Other(anyhow::anyhow!("boom")))
        }
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn scope() -> WorkspaceScope {
        WorkspaceScope {
            tenant: "t".into(),
            team: None,
            session: "s".into(),
            agent: None,
            run: "r".into(),
        }
    }

    fn budget(max_fragments: u32) -> ContextBudget {
        ContextBudget {
            max_fragments,
            max_bytes: 4096,
        }
    }

    fn provider_with(
        content: &str,
    ) -> (
        LlmContextProvider,
        std::sync::Arc<InMemoryWorkspaceProvider>,
    ) {
        let ws = std::sync::Arc::new(InMemoryWorkspaceProvider::new());
        let llm = std::sync::Arc::new(StubLlm::with_response(content));
        (LlmContextProvider::new(llm, ws.clone(), scope()), ws)
    }

    fn package_for_test() -> ContextPackage {
        ContextPackage {
            package_id: "pkg-1".into(),
            fragments: vec![ContextFragment {
                fragment_id: "f0".into(),
                kind: ContextFragmentKind::MemoryItem,
                content_ref: "ref-a".into(),
                content: Some("body".into()),
                provenance: "test".into(),
                ordinal: 0,
            }],
            budget: budget(8),
        }
    }

    // ---------------------------------------------------------------------------
    // build_context tests
    // ---------------------------------------------------------------------------

    #[test]
    fn build_context_assembles_fragments_within_budget() {
        let (cx, _ws) = provider_with("");
        let pkg = cx
            .build_context(BuildContextRequest {
                fragment_refs: vec!["a".into(), "b".into(), "c".into()],
                query: None,
                budget: budget(8),
            })
            .unwrap();
        assert_eq!(pkg.fragments.len(), 3);
        assert_eq!(pkg.fragments[0].ordinal, 0);
        assert!(!pkg.package_id.is_empty());
    }

    #[test]
    fn build_context_caps_to_max_fragments() {
        let (cx, _ws) = provider_with("");
        let pkg = cx
            .build_context(BuildContextRequest {
                fragment_refs: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
                query: None,
                budget: budget(2),
            })
            .unwrap();
        assert_eq!(pkg.fragments.len(), 2);
    }

    #[test]
    fn build_context_zero_budget_is_validation_error() {
        let (cx, _ws) = provider_with("");
        let err = cx
            .build_context(BuildContextRequest {
                fragment_refs: vec![],
                query: None,
                budget: budget(0),
            })
            .unwrap_err();
        assert!(matches!(err, ContextError::Validation(_)));
    }

    // ---------------------------------------------------------------------------
    // compress_context tests
    // ---------------------------------------------------------------------------

    #[test]
    fn compress_writes_artifact_and_returns_ref() {
        let (cx, ws) = provider_with("COMPRESSED");
        let out = cx
            .compress_context(CompressContextRequest {
                package: package_for_test(),
            })
            .unwrap();
        assert!(out.compressed_artifact_ref.contains("::compressed::"));
        assert_eq!(out.source_package_id, "pkg-1");
        assert_eq!(out.fragment_count, 1);
        let stored = ws
            .read_artifact(ReadArtifactRequest {
                artifact_id: out.compressed_artifact_ref.clone(),
            })
            .unwrap();
        assert_eq!(stored.body, "COMPRESSED");
    }

    #[test]
    fn compress_invalid_package_is_validation_error() {
        let (cx, _ws) = provider_with("X");
        let mut bad = package_for_test();
        bad.package_id = "".into();
        let err = cx
            .compress_context(CompressContextRequest { package: bad })
            .unwrap_err();
        assert!(matches!(err, ContextError::Validation(_)));
    }

    #[test]
    fn compress_llm_failure_is_provider_error() {
        let ws = std::sync::Arc::new(InMemoryWorkspaceProvider::new());
        let cx = LlmContextProvider::new(std::sync::Arc::new(FailingLlm), ws, scope());
        let err = cx
            .compress_context(CompressContextRequest {
                package: package_for_test(),
            })
            .unwrap_err();
        assert!(matches!(err, ContextError::Provider(_)));
    }

    // ---------------------------------------------------------------------------
    // summarize_context tests
    // ---------------------------------------------------------------------------

    #[test]
    fn summarize_writes_artifact_and_returns_ref() {
        let (cx, ws) = provider_with("SUMMARY");
        let out = cx
            .summarize_context(SummarizeContextRequest {
                package: package_for_test(),
            })
            .unwrap();
        assert!(out.artifact_ref.contains("::summary::"));
        let stored = ws
            .read_artifact(ReadArtifactRequest {
                artifact_id: out.artifact_ref.clone(),
            })
            .unwrap();
        assert_eq!(stored.body, "SUMMARY");
    }
}
