//! Prompt builders for the LLM-backed context methods.
//!
//! `render_package` flattens a `ContextPackage` into readable text; the
//! `system_for_*` / `user_for_*` builders instruct the model to return PLAIN
//! TEXT (no JSON, no fences) — the reply is stored verbatim as an artifact.

use greentic_dw_context::ContextPackage;

/// Flatten a context package into a stable, human-readable block: one line per
/// fragment in `ordinal` order, including inline `content` when present.
pub fn render_package(package: &ContextPackage) -> String {
    let mut fragments: Vec<&greentic_dw_context::ContextFragment> =
        package.fragments.iter().collect();
    fragments.sort_by_key(|f| f.ordinal);
    let mut out = format!(
        "Context package {} ({} fragments):\n",
        package.package_id,
        package.fragments.len()
    );
    for fragment in fragments {
        out.push_str(&format!(
            "[{}] {:?} {}",
            fragment.ordinal, fragment.kind, fragment.content_ref
        ));
        if let Some(text) = &fragment.content {
            out.push_str(" :: ");
            out.push_str(text.trim());
        }
        out.push('\n');
    }
    out
}

/// System prompt for `compress_context`.
pub fn system_for_compress() -> String {
    "You are a context-compression assistant in a deep-worker system. Condense the supplied \
context into the smallest faithful form that preserves every fact needed for downstream \
reasoning. Respond with ONLY the compressed text — no preamble, no JSON, no markdown fences."
        .to_string()
}

/// System prompt for `summarize_context`.
pub fn system_for_summarize() -> String {
    "You are a context-summarization assistant in a deep-worker system. Produce a concise \
prose summary of the supplied context. Respond with ONLY the summary text — no preamble, no \
JSON, no markdown fences."
        .to_string()
}

/// User prompt wrapping the rendered package for compression.
pub fn user_for_compress(rendered: &str) -> String {
    format!("Compress the following context:\n\n{rendered}")
}

/// User prompt wrapping the rendered package for summarization.
pub fn user_for_summarize(rendered: &str) -> String {
    format!("Summarize the following context:\n\n{rendered}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_context::{ContextBudget, ContextFragment, ContextFragmentKind};

    fn pkg() -> ContextPackage {
        ContextPackage {
            package_id: "p1".into(),
            fragments: vec![
                ContextFragment {
                    fragment_id: "f1".into(),
                    kind: ContextFragmentKind::KnowledgeChunk,
                    content_ref: "ref-b".into(),
                    content: Some("second".into()),
                    provenance: "x".into(),
                    ordinal: 1,
                },
                ContextFragment {
                    fragment_id: "f0".into(),
                    kind: ContextFragmentKind::MemoryItem,
                    content_ref: "ref-a".into(),
                    content: None,
                    provenance: "x".into(),
                    ordinal: 0,
                },
            ],
            budget: ContextBudget {
                max_fragments: 8,
                max_bytes: 4096,
            },
        }
    }

    #[test]
    fn render_package_orders_by_ordinal_and_includes_inline_content() {
        let out = render_package(&pkg());
        let first = out.find("ref-a").unwrap();
        let second = out.find("ref-b").unwrap();
        assert!(first < second, "fragments must render in ordinal order");
        assert!(out.contains("second"), "inline content must be included");
        assert!(out.contains("p1"));
    }
}
