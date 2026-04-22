use crate::{
    ArtifactContent, ArtifactMetadata, ArtifactVersion, CreateArtifactRequest, WorkspaceError,
};

pub fn validate_create_artifact_request(req: &CreateArtifactRequest) -> Result<(), WorkspaceError> {
    if req.artifact.artifact_id.trim().is_empty() {
        return Err(WorkspaceError::Validation(
            "artifact_id must not be empty".to_string(),
        ));
    }
    if req.artifact.scope.tenant.trim().is_empty() {
        return Err(WorkspaceError::Validation(
            "scope tenant must not be empty".to_string(),
        ));
    }
    if req.artifact.scope.session.trim().is_empty() {
        return Err(WorkspaceError::Validation(
            "scope session must not be empty".to_string(),
        ));
    }
    if req.artifact.scope.run.trim().is_empty() {
        return Err(WorkspaceError::Validation(
            "scope run must not be empty".to_string(),
        ));
    }
    validate_metadata(&req.metadata)?;
    Ok(())
}

pub fn validate_metadata(metadata: &ArtifactMetadata) -> Result<(), WorkspaceError> {
    if metadata.title.trim().is_empty() {
        return Err(WorkspaceError::Validation(
            "artifact title must not be empty".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_version_progression(
    previous: &ArtifactVersion,
    next: &ArtifactVersion,
) -> Result<(), WorkspaceError> {
    if previous.artifact_id != next.artifact_id {
        return Err(WorkspaceError::Validation(
            "artifact versions must reference the same artifact_id".to_string(),
        ));
    }
    if next.version != previous.version + 1 {
        return Err(WorkspaceError::Validation(
            "artifact version increments must be immutable and sequential".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_content(content: &ArtifactContent) -> Result<(), WorkspaceError> {
    validate_metadata(&content.metadata)?;
    if content.version.artifact_id != content.artifact.artifact_id {
        return Err(WorkspaceError::Validation(
            "content version artifact_id must match artifact ref".to_string(),
        ));
    }
    Ok(())
}
