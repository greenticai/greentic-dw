use crate::{ContextBudget, ContextFragment, ContextFragmentKind, ContextPackage};

pub fn context_package_fixture() -> ContextPackage {
    ContextPackage {
        package_id: "context-1".to_string(),
        fragments: vec![
            ContextFragment {
                fragment_id: "fragment-1".to_string(),
                kind: ContextFragmentKind::WorkspaceArtifact,
                content_ref: "artifact://notes-1".to_string(),
                provenance: "workspace".to_string(),
                ordinal: 1,
            },
            ContextFragment {
                fragment_id: "fragment-2".to_string(),
                kind: ContextFragmentKind::PlanStep,
                content_ref: "plan-step://review".to_string(),
                provenance: "plan".to_string(),
                ordinal: 2,
            },
        ],
        budget: ContextBudget {
            max_fragments: 8,
            max_bytes: 4096,
        },
    }
}
