# Workspace Artifact

## Type

`greentic_dw_workspace::ArtifactContent`

## Purpose

Represents a stored workspace artifact with metadata, body, and a versioned history record.

## Important fields

- `artifact`
- `metadata`
- `version`
- `body`

## Related types

- `ArtifactRef`
- `ArtifactVersion`
- `ArtifactMetadata`
- `WorkspaceScope`

## Validation expectations

- artifact and version IDs must match
- titles must not be empty
- version progression should remain sequential and immutable
