use crate::{
    ArtifactVersion, CreateArtifactRequest, WorkspaceError, artifact_content_fixture,
    validate_content, validate_create_artifact_request, validate_version_progression,
};

#[test]
fn validates_create_request() {
    let content = artifact_content_fixture();
    let req = CreateArtifactRequest {
        artifact: content.artifact,
        metadata: content.metadata,
        body: content.body,
    };
    validate_create_artifact_request(&req).expect("request should validate");
}

#[test]
fn validates_content_fixture() {
    validate_content(&artifact_content_fixture()).expect("content should validate");
}

#[test]
fn enforces_immutable_version_progression() {
    let previous = artifact_content_fixture().version;
    let next = ArtifactVersion {
        artifact_id: previous.artifact_id.clone(),
        version: previous.version + 1,
        checksum: "sha256:def".to_string(),
        created_at: "2026-04-15T00:01:00Z".to_string(),
        derived_from: vec![],
        provenance: vec!["updated".to_string()],
    };
    validate_version_progression(&previous, &next).expect("sequential version should pass");
    let bad = ArtifactVersion { version: 9, ..next };
    let err = validate_version_progression(&previous, &bad)
        .expect_err("non-sequential version should fail");
    assert_eq!(
        err,
        WorkspaceError::Validation(
            "artifact version increments must be immutable and sequential".to_string()
        )
    );
}
