# PR-02: Workspace Core Contracts

## Title
feat(dw): add workspace artifact contracts and versioned artifact model

## Why
Deep agents need a typed working area for notes, drafts, evidence, and intermediate outputs.
LangChain’s deep-agent guidance treats the filesystem / virtual filesystem as a shared workspace
for long-running tasks and subagents.

## Scope
Create a workspace core crate with:
- `WorkspaceProvider`
- `ArtifactRef`
- `ArtifactVersion`
- `ArtifactMetadata`
- `ArtifactKind`
- `WorkspaceScope`

## Target file tree
```text
crates/
  greentic-dw-workspace/
    Cargo.toml
    src/
      lib.rs
      error.rs
      traits.rs
      model.rs
      validate.rs
      fixtures.rs
```

## Concrete work

### 1) Create trait
```rust
pub trait WorkspaceProvider: Send + Sync {
    fn create_artifact(&self, req: CreateArtifactRequest) -> Result<ArtifactRef, WorkspaceError>;
    fn read_artifact(&self, req: ReadArtifactRequest) -> Result<ArtifactContent, WorkspaceError>;
    fn update_artifact(&self, req: UpdateArtifactRequest) -> Result<ArtifactVersion, WorkspaceError>;
    fn list_artifacts(&self, req: ListArtifactsRequest) -> Result<Vec<ArtifactSummary>, WorkspaceError>;
    fn link_artifacts(&self, req: LinkArtifactsRequest) -> Result<(), WorkspaceError>;
}
```

### 2) Artifact kinds
Support:
- `Note`
- `Draft`
- `Evidence`
- `ToolOutput`
- `PromptFragment`
- `Table`
- `ReportSection`
- `Custom(String)`

### 3) Version model
Every update produces a new immutable `ArtifactVersion` with:
- `artifact_id`
- `version`
- `checksum`
- `created_at`
- `derived_from: Vec<ArtifactRef>`
- `provenance: Vec<String>`

### 4) Scope model
Support:
- tenant
- team
- session
- agent
- run

## Tests
- artifact create/read/update round-trip
- immutable version history preserved
- typed artifact validation
- provenance linkage retained across updates

## Acceptance criteria
- Typed artifacts exist as a first-class contract.
- No concrete storage backend yet.
- Models are ready for context/reflection consumption.
