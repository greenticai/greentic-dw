use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextFragmentKind {
    MemoryItem,
    WorkspaceArtifact,
    PlanStep,
    RuntimeMetadata,
    PromptFragment,
    /// A passage retrieved from the worker's knowledge corpus (RAG).
    KnowledgeChunk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextFragment {
    pub fragment_id: String,
    pub kind: ContextFragmentKind,
    pub content_ref: String,
    /// Inline renderable text for retrieval results (e.g. knowledge chunks).
    /// Reference-only fragments (memory / artifact / plan-step) leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub provenance: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextBudget {
    pub max_fragments: u32,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextPackage {
    pub package_id: String,
    #[serde(default)]
    pub fragments: Vec<ContextFragment>,
    pub budget: ContextBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompressedContext {
    pub source_package_id: String,
    pub compressed_artifact_ref: String,
    pub fragment_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SummaryArtifactRef {
    pub artifact_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BuildContextRequest {
    #[serde(default)]
    pub fragment_refs: Vec<String>,
    /// Semantic retrieval query. When `Some`, a knowledge-aware
    /// `ContextProvider` performs RAG; `None` preserves fragment-ref-only
    /// behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub budget: ContextBudget,
}

/// Render the inline-content fragments of a context package into a delimited
/// `<knowledge>` block, in ascending `ordinal` order. Returns an empty string
/// when no fragment carries inline content. Mirrors the aw-runtime
/// `knowledge::augment_system_prompt` shape for cross-runtime consistency.
pub fn render_context(package: &ContextPackage) -> String {
    let mut frags: Vec<&ContextFragment> = package
        .fragments
        .iter()
        .filter(|f| f.content.is_some())
        .collect();
    if frags.is_empty() {
        return String::new();
    }
    frags.sort_by_key(|f| f.ordinal);
    let mut out = String::from(
        "<knowledge>\nRelevant passages retrieved from the worker's knowledge base:\n",
    );
    for f in frags {
        if let Some(text) = &f.content {
            out.push_str("- ");
            out.push_str(text.trim());
            out.push('\n');
        }
    }
    out.push_str("</knowledge>");
    out
}

#[cfg(test)]
mod rag_tests {
    use super::*;

    fn knowledge_fragment(text: &str, ordinal: u32) -> ContextFragment {
        ContextFragment {
            fragment_id: format!("k{ordinal}"),
            kind: ContextFragmentKind::KnowledgeChunk,
            content_ref: String::new(),
            content: Some(text.to_string()),
            provenance: "knowledge".into(),
            ordinal,
        }
    }

    #[test]
    fn build_context_request_query_defaults_none() {
        let req: BuildContextRequest = serde_json::from_str(
            r#"{"fragment_refs":[],"budget":{"max_fragments":8,"max_bytes":16384}}"#,
        )
        .unwrap();
        assert!(req.query.is_none());
    }

    #[test]
    fn render_context_emits_knowledge_block_in_ordinal_order() {
        let pkg = ContextPackage {
            package_id: "p1".into(),
            fragments: vec![
                knowledge_fragment("second", 1),
                knowledge_fragment("first", 0),
            ],
            budget: ContextBudget {
                max_fragments: 8,
                max_bytes: 16384,
            },
        };
        let out = render_context(&pkg);
        assert!(out.contains("<knowledge>"));
        assert!(out.contains("</knowledge>"));
        let first = out.find("first").unwrap();
        let second = out.find("second").unwrap();
        assert!(first < second, "fragments must render in ordinal order");
    }

    #[test]
    fn render_context_empty_when_no_inline_content() {
        let pkg = ContextPackage {
            package_id: "p1".into(),
            fragments: vec![ContextFragment {
                fragment_id: "a".into(),
                kind: ContextFragmentKind::WorkspaceArtifact,
                content_ref: "artifact://x".into(),
                content: None,
                provenance: "ws".into(),
                ordinal: 0,
            }],
            budget: ContextBudget {
                max_fragments: 8,
                max_bytes: 16384,
            },
        };
        assert_eq!(render_context(&pkg), "");
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CompressContextRequest {
    pub package: ContextPackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SummarizeContextRequest {
    pub package: ContextPackage,
}
