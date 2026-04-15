use crate::{
    ArtifactContent, ArtifactRef, ArtifactSummary, ArtifactVersion, CreateArtifactRequest,
    LinkArtifactsRequest, ListArtifactsRequest, ReadArtifactRequest, UpdateArtifactRequest,
    WorkspaceError,
};

pub trait WorkspaceProvider: Send + Sync {
    fn create_artifact(&self, req: CreateArtifactRequest) -> Result<ArtifactRef, WorkspaceError>;
    fn read_artifact(&self, req: ReadArtifactRequest) -> Result<ArtifactContent, WorkspaceError>;
    fn update_artifact(
        &self,
        req: UpdateArtifactRequest,
    ) -> Result<ArtifactVersion, WorkspaceError>;
    fn list_artifacts(
        &self,
        req: ListArtifactsRequest,
    ) -> Result<Vec<ArtifactSummary>, WorkspaceError>;
    fn link_artifacts(&self, req: LinkArtifactsRequest) -> Result<(), WorkspaceError>;
}
