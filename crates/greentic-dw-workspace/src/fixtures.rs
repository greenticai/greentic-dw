use crate::{
    ArtifactContent, ArtifactKind, ArtifactMetadata, ArtifactRef, ArtifactVersion, WorkspaceScope,
};

pub fn workspace_scope_fixture() -> WorkspaceScope {
    WorkspaceScope {
        tenant: "tenant-a".to_string(),
        team: Some("ops".to_string()),
        session: "session-1".to_string(),
        agent: Some("agent-main".to_string()),
        run: "run-1".to_string(),
    }
}

pub fn artifact_content_fixture() -> ArtifactContent {
    ArtifactContent {
        artifact: ArtifactRef {
            artifact_id: "artifact-1".to_string(),
            kind: ArtifactKind::Note,
            scope: workspace_scope_fixture(),
        },
        metadata: ArtifactMetadata {
            title: "Investigation notes".to_string(),
            tags: vec!["incident".to_string()],
            mime_type: Some("text/markdown".to_string()),
        },
        version: ArtifactVersion {
            artifact_id: "artifact-1".to_string(),
            version: 1,
            checksum: "sha256:abc".to_string(),
            created_at: "2026-04-15T00:00:00Z".to_string(),
            derived_from: vec![],
            provenance: vec!["initial capture".to_string()],
        },
        body: "hello".to_string(),
    }
}
