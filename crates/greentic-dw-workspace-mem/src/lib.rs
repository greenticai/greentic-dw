//! In-memory WorkspaceProvider for Digital Worker deep-agent flows.

use std::collections::HashMap;
use std::sync::RwLock;

use greentic_dw_workspace::{
    ArtifactContent, ArtifactMetadata, ArtifactRef, ArtifactSummary, ArtifactVersion,
    CreateArtifactRequest, LinkArtifactsRequest, ListArtifactsRequest, ReadArtifactRequest,
    UpdateArtifactRequest, WorkspaceError, WorkspaceProvider, validate_create_artifact_request,
    validate_version_progression,
};

// Outgoing artifact relations are recorded for provenance. No read API
// (`get_links`) exists on the `WorkspaceProvider` contract yet, so the fields
// are write-only for now; `allow(dead_code)` until a read path is added.
#[allow(dead_code)]
struct ArtifactLink {
    to_artifact_id: String,
    relation: String,
}

struct StoredArtifact {
    artifact: ArtifactRef,
    metadata: ArtifactMetadata,
    body: String,
    versions: Vec<ArtifactVersion>,
    links: Vec<ArtifactLink>,
}

/// In-memory, per-process artifact store. First-cut backend behind
/// `WorkspaceProvider`; a persistent backend can replace it behind the trait.
pub struct InMemoryWorkspaceProvider {
    store: RwLock<HashMap<String, StoredArtifact>>,
    clock: Box<dyn Fn() -> String + Send + Sync>,
}

impl InMemoryWorkspaceProvider {
    /// Construct with a real RFC3339-UTC clock.
    pub fn new() -> Self {
        Self::with_clock(default_clock)
    }

    /// Construct with an injectable clock (deterministic in tests).
    pub fn with_clock(clock: impl Fn() -> String + Send + Sync + 'static) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            clock: Box::new(clock),
        }
    }

    fn checksum(body: &str) -> String {
        format!("blake3:{}", blake3::hash(body.as_bytes()).to_hex())
    }
}

impl Default for InMemoryWorkspaceProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn default_clock() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn poisoned() -> WorkspaceError {
    WorkspaceError::Provider("workspace lock poisoned".to_string())
}

impl WorkspaceProvider for InMemoryWorkspaceProvider {
    fn create_artifact(&self, req: CreateArtifactRequest) -> Result<ArtifactRef, WorkspaceError> {
        validate_create_artifact_request(&req)?;
        let mut store = self.store.write().map_err(|_| poisoned())?;
        if store.contains_key(&req.artifact.artifact_id) {
            return Err(WorkspaceError::Provider(format!(
                "artifact already exists: {}",
                req.artifact.artifact_id
            )));
        }
        let version = ArtifactVersion {
            artifact_id: req.artifact.artifact_id.clone(),
            version: 1,
            checksum: Self::checksum(&req.body),
            created_at: (self.clock)(),
            derived_from: vec![],
            provenance: vec![],
        };
        let artifact = req.artifact.clone();
        store.insert(
            req.artifact.artifact_id.clone(),
            StoredArtifact {
                artifact: req.artifact,
                metadata: req.metadata,
                body: req.body,
                versions: vec![version],
                links: vec![],
            },
        );
        Ok(artifact)
    }

    fn read_artifact(&self, req: ReadArtifactRequest) -> Result<ArtifactContent, WorkspaceError> {
        let store = self.store.read().map_err(|_| poisoned())?;
        let entry = store.get(&req.artifact_id).ok_or_else(|| {
            WorkspaceError::Provider(format!("artifact not found: {}", req.artifact_id))
        })?;
        let version = entry
            .versions
            .last()
            .ok_or_else(|| WorkspaceError::Provider("artifact has no versions".to_string()))?
            .clone();
        Ok(ArtifactContent {
            artifact: entry.artifact.clone(),
            metadata: entry.metadata.clone(),
            version,
            body: entry.body.clone(),
        })
    }

    fn update_artifact(
        &self,
        req: UpdateArtifactRequest,
    ) -> Result<ArtifactVersion, WorkspaceError> {
        let mut store = self.store.write().map_err(|_| poisoned())?;
        let entry = store.get_mut(&req.artifact_id).ok_or_else(|| {
            WorkspaceError::Provider(format!("artifact not found: {}", req.artifact_id))
        })?;
        let previous = entry
            .versions
            .last()
            .ok_or_else(|| WorkspaceError::Provider("artifact has no versions".to_string()))?
            .clone();
        let next = ArtifactVersion {
            artifact_id: req.artifact_id.clone(),
            version: previous.version + 1,
            checksum: Self::checksum(&req.body),
            created_at: (self.clock)(),
            derived_from: req.derived_from,
            provenance: req.provenance,
        };
        validate_version_progression(&previous, &next)?;
        entry.body = req.body;
        entry.versions.push(next.clone());
        Ok(next)
    }

    fn list_artifacts(
        &self,
        req: ListArtifactsRequest,
    ) -> Result<Vec<ArtifactSummary>, WorkspaceError> {
        let store = self.store.read().map_err(|_| poisoned())?;
        let mut summaries: Vec<ArtifactSummary> = store
            .values()
            .filter(|e| e.artifact.scope == req.scope)
            .filter_map(|e| {
                e.versions.last().map(|latest| ArtifactSummary {
                    artifact: e.artifact.clone(),
                    latest_version: latest.clone(),
                    metadata: e.metadata.clone(),
                })
            })
            .collect();
        summaries.sort_by(|a, b| a.artifact.artifact_id.cmp(&b.artifact.artifact_id));
        Ok(summaries)
    }

    fn link_artifacts(&self, req: LinkArtifactsRequest) -> Result<(), WorkspaceError> {
        if req.relation.trim().is_empty() {
            return Err(WorkspaceError::Validation(
                "link relation must not be empty".to_string(),
            ));
        }
        let mut store = self.store.write().map_err(|_| poisoned())?;
        if !store.contains_key(&req.to_artifact_id) {
            return Err(WorkspaceError::Provider(format!(
                "artifact not found: {}",
                req.to_artifact_id
            )));
        }
        let from = store.get_mut(&req.from_artifact_id).ok_or_else(|| {
            WorkspaceError::Provider(format!("artifact not found: {}", req.from_artifact_id))
        })?;
        from.links.push(ArtifactLink {
            to_artifact_id: req.to_artifact_id,
            relation: req.relation,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_workspace::*;

    fn scope(tenant: &str, session: &str, run: &str) -> WorkspaceScope {
        WorkspaceScope {
            tenant: tenant.into(),
            team: None,
            session: session.into(),
            agent: None,
            run: run.into(),
        }
    }
    fn create_req(id: &str, sc: WorkspaceScope, title: &str, body: &str) -> CreateArtifactRequest {
        CreateArtifactRequest {
            artifact: ArtifactRef {
                artifact_id: id.into(),
                kind: ArtifactKind::Note,
                scope: sc,
            },
            metadata: ArtifactMetadata {
                title: title.into(),
                tags: vec![],
                mime_type: None,
            },
            body: body.into(),
        }
    }

    #[test]
    fn create_then_read_round_trips() {
        let ws = InMemoryWorkspaceProvider::new();
        let sc = scope("t", "s", "r");
        ws.create_artifact(create_req("a1", sc.clone(), "Title", "hello"))
            .unwrap();
        let content = ws
            .read_artifact(ReadArtifactRequest {
                artifact_id: "a1".into(),
            })
            .unwrap();
        assert_eq!(content.body, "hello");
        assert_eq!(content.version.version, 1);
        assert!(content.version.checksum.starts_with("blake3:"));
        assert_eq!(content.artifact.artifact_id, "a1");
    }

    #[test]
    fn duplicate_create_is_provider_error() {
        let ws = InMemoryWorkspaceProvider::new();
        let sc = scope("t", "s", "r");
        ws.create_artifact(create_req("a1", sc.clone(), "T", "b"))
            .unwrap();
        let err = ws
            .create_artifact(create_req("a1", sc, "T", "b"))
            .unwrap_err();
        assert!(matches!(err, WorkspaceError::Provider(_)));
    }

    #[test]
    fn update_bumps_version_and_carries_provenance() {
        let ws = InMemoryWorkspaceProvider::new();
        let sc = scope("t", "s", "r");
        ws.create_artifact(create_req("a1", sc, "T", "v1body"))
            .unwrap();
        let v = ws
            .update_artifact(UpdateArtifactRequest {
                artifact_id: "a1".into(),
                body: "v2body".into(),
                derived_from: vec![],
                provenance: vec!["edited".into()],
            })
            .unwrap();
        assert_eq!(v.version, 2);
        assert_eq!(v.provenance, vec!["edited".to_string()]);
        let content = ws
            .read_artifact(ReadArtifactRequest {
                artifact_id: "a1".into(),
            })
            .unwrap();
        assert_eq!(content.body, "v2body");
        assert_eq!(content.version.version, 2);
    }

    #[test]
    fn read_missing_is_provider_error() {
        let ws = InMemoryWorkspaceProvider::new();
        assert!(matches!(
            ws.read_artifact(ReadArtifactRequest {
                artifact_id: "nope".into()
            })
            .unwrap_err(),
            WorkspaceError::Provider(_)
        ));
    }

    #[test]
    fn update_missing_is_provider_error() {
        let ws = InMemoryWorkspaceProvider::new();
        let err = ws
            .update_artifact(UpdateArtifactRequest {
                artifact_id: "nope".into(),
                body: "x".into(),
                derived_from: vec![],
                provenance: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, WorkspaceError::Provider(_)));
    }

    #[test]
    fn list_filters_by_scope_sorted_by_id() {
        let ws = InMemoryWorkspaceProvider::new();
        let s1 = scope("t", "s", "r");
        let s2 = scope("t", "s", "other-run");
        ws.create_artifact(create_req("b", s1.clone(), "T", "x"))
            .unwrap();
        ws.create_artifact(create_req("a", s1.clone(), "T", "x"))
            .unwrap();
        ws.create_artifact(create_req("z", s2, "T", "x")).unwrap();
        let listed = ws
            .list_artifacts(ListArtifactsRequest { scope: s1 })
            .unwrap();
        let ids: Vec<_> = listed
            .iter()
            .map(|s| s.artifact.artifact_id.clone())
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn list_scope_matches_optional_team_and_agent() {
        let ws = InMemoryWorkspaceProvider::new();
        let mut with_team = scope("t", "s", "r");
        with_team.team = Some("ops".into());
        let no_team = scope("t", "s", "r");
        ws.create_artifact(create_req("a", with_team.clone(), "T", "x"))
            .unwrap();
        ws.create_artifact(create_req("b", no_team.clone(), "T", "x"))
            .unwrap();
        assert_eq!(
            ws.list_artifacts(ListArtifactsRequest { scope: with_team })
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            ws.list_artifacts(ListArtifactsRequest { scope: no_team })
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn link_existing_ok_missing_or_empty_errors() {
        let ws = InMemoryWorkspaceProvider::new();
        let sc = scope("t", "s", "r");
        ws.create_artifact(create_req("a", sc.clone(), "T", "x"))
            .unwrap();
        ws.create_artifact(create_req("b", sc, "T", "x")).unwrap();
        ws.link_artifacts(LinkArtifactsRequest {
            from_artifact_id: "a".into(),
            to_artifact_id: "b".into(),
            relation: "derived".into(),
        })
        .unwrap();
        assert!(matches!(
            ws.link_artifacts(LinkArtifactsRequest {
                from_artifact_id: "a".into(),
                to_artifact_id: "b".into(),
                relation: "".into()
            })
            .unwrap_err(),
            WorkspaceError::Validation(_)
        ));
        assert!(matches!(
            ws.link_artifacts(LinkArtifactsRequest {
                from_artifact_id: "a".into(),
                to_artifact_id: "missing".into(),
                relation: "x".into()
            })
            .unwrap_err(),
            WorkspaceError::Provider(_)
        ));
    }

    #[test]
    fn validation_passthrough_empty_title() {
        let ws = InMemoryWorkspaceProvider::new();
        let err = ws
            .create_artifact(create_req("a", scope("t", "s", "r"), "", "x"))
            .unwrap_err();
        assert!(matches!(err, WorkspaceError::Validation(_)));
    }

    #[test]
    fn validation_passthrough_empty_artifact_id() {
        let ws = InMemoryWorkspaceProvider::new();
        let err = ws
            .create_artifact(create_req("", scope("t", "s", "r"), "T", "x"))
            .unwrap_err();
        assert!(matches!(err, WorkspaceError::Validation(_)));
    }

    #[test]
    fn validation_passthrough_empty_scope_field() {
        let ws = InMemoryWorkspaceProvider::new();
        let err = ws
            .create_artifact(create_req("a", scope("", "s", "r"), "T", "x"))
            .unwrap_err();
        assert!(matches!(err, WorkspaceError::Validation(_)));
    }

    #[test]
    fn clock_injection_sets_created_at() {
        let ws = InMemoryWorkspaceProvider::with_clock(|| "FIXED".to_string());
        ws.create_artifact(create_req("a", scope("t", "s", "r"), "T", "x"))
            .unwrap();
        let content = ws
            .read_artifact(ReadArtifactRequest {
                artifact_id: "a".into(),
            })
            .unwrap();
        assert_eq!(content.version.created_at, "FIXED");
        let v = ws
            .update_artifact(UpdateArtifactRequest {
                artifact_id: "a".into(),
                body: "y".into(),
                derived_from: vec![],
                provenance: vec![],
            })
            .unwrap();
        assert_eq!(v.created_at, "FIXED");
    }
}
