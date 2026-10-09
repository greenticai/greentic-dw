# In-memory WorkspaceProvider (Design) — deep-worker brain, slice 4a

- **Date:** 2026-06-21
- **Status:** Design approved (user chose "Workspace lalu Context"), ready for planning
- **Surface:** greentic-dw (new crate `greentic-dw-workspace-mem`).
- **Part of:** SP-3 deep-worker brain, slice 4a (the artifact store). Prereq for slice 4b (`ContextProvider` writes compressed/summary artifacts here). Unlike the three reasoning providers (planning/reflection/delegation, all on `research`), this is a **stateful store, not an LLM provider** — no `greentic-llm`, no `bridge.rs`.

## 1. Contract (verified)

`greentic_dw_workspace::WorkspaceProvider` — **5 sync methods**:
- `create_artifact(CreateArtifactRequest{artifact: ArtifactRef, metadata, body}) -> ArtifactRef`
- `read_artifact(ReadArtifactRequest{artifact_id}) -> ArtifactContent{artifact, metadata, version, body}`
- `update_artifact(UpdateArtifactRequest{artifact_id, body, derived_from, provenance}) -> ArtifactVersion`
- `list_artifacts(ListArtifactsRequest{scope: WorkspaceScope}) -> Vec<ArtifactSummary>`
- `link_artifacts(LinkArtifactsRequest{from_artifact_id, to_artifact_id, relation}) -> ()`

DTOs (in `greentic-dw-workspace/src/model.rs`): `ArtifactRef{artifact_id, kind: ArtifactKind, scope: WorkspaceScope}`; `WorkspaceScope{tenant, team:Option, session, agent:Option, run}` (derives `PartialEq+Eq`); `ArtifactVersion{artifact_id, version:u32, checksum, created_at, derived_from:Vec<ArtifactRef>, provenance:Vec<String>}`; `ArtifactContent{artifact, metadata, version, body}`; `ArtifactSummary{artifact, latest_version, metadata}`. Errors: `WorkspaceError{Validation(String), Provider(String)}`. Existing helpers (reuse): `validate_create_artifact_request`, `validate_version_progression`, `validate_metadata`.

## 2. Design

New crate `greentic-dw-workspace-mem`:

```rust
pub struct InMemoryWorkspaceProvider {
    store: RwLock<HashMap<String, StoredArtifact>>,    // keyed by artifact_id
    clock: Box<dyn Fn() -> String + Send + Sync>,      // injectable; default = RFC3339 UTC via `time`
}
struct StoredArtifact { artifact: ArtifactRef, metadata: ArtifactMetadata, body: String,
                        versions: Vec<ArtifactVersion>, links: Vec<ArtifactLink> }
struct ArtifactLink { to_artifact_id: String, relation: String }
```

Constructors: `new()` (real clock), `with_clock(impl Fn() -> String + Send + Sync + 'static)` (tests). `Default` = `new()`.

- **checksum**: `format!("blake3:{}", blake3::hash(body.as_bytes()).to_hex())` (blake3 = precedent in `greentic-dw-pack-builder`).
- **created_at**: `clock()` — default `time::OffsetDateTime::now_utc()` formatted RFC3339.
- **Concurrency**: `RwLock`; a poisoned lock maps to `WorkspaceError::Provider("workspace lock poisoned")` — **no `.unwrap()`/`.expect()` in non-test code**.

Method semantics:
- `create_artifact`: `validate_create_artifact_request(&req)?`; if id already present → `Provider("artifact already exists: {id}")`; else store with `version = 1`, computed checksum + `clock()`, empty `derived_from`/`provenance`; return `req.artifact`.
- `read_artifact`: id miss → `Provider("artifact not found: {id}")`; else `ArtifactContent{artifact, metadata, version: latest.clone(), body}`.
- `update_artifact`: id miss → `Provider("artifact not found")`; build next version `prev.version + 1` (checksum from new body, `clock()`, carrying `req.derived_from`/`req.provenance`); `validate_version_progression(prev, &next)?` (defensive); push version, replace body; return the new `ArtifactVersion`.
- `list_artifacts`: return `ArtifactSummary` for every stored artifact whose `artifact.scope == req.scope` (full structural equality, incl. Optional team/agent); **sorted by `artifact_id`** for deterministic output.
- `link_artifacts`: relation empty → `Validation("link relation must not be empty")`; either endpoint missing → `Provider("artifact not found: {id}")`; else append `ArtifactLink` to the `from` artifact; return `()`.

## 3. Error handling

Input-shape problems → `Validation` (delegated to the existing validators + the empty-relation check). State problems (duplicate create, missing read/update/link endpoint, poisoned lock) → `Provider`. No panics; no `unwrap`/`expect` in non-test code.

## 4. Testing

- `create` then `read` → body round-trips; version 1; checksum starts `blake3:`.
- duplicate `create` → `Provider`.
- `update` → version 2, new body, `derived_from`/`provenance` carried on the new version; original body gone.
- `read`/`update`/`link` on missing id → `Provider`.
- `list_artifacts` returns only scope-matching artifacts, sorted by id; a different scope is excluded; Optional team/agent participates in matching.
- `link_artifacts`: existing→existing ok; empty relation → `Validation`; missing endpoint → `Provider`.
- validation passthrough: empty `artifact_id` / empty `metadata.title` / empty scope field → `Validation`.
- clock injection: `with_clock(|| "FIXED".into())` → `created_at == "FIXED"` on create and update.

## 5. Limitations

- In-memory only (per-process, lost on restart) — first cut; a persistent (Redis/SQL) backend is a future enhancement behind the same trait.
- `list_artifacts` matches scope by exact structural equality (no partial/prefix scope queries) — sufficient for the deep-loop’s per-run scope.
- Links are stored but not yet surfaced through a read API (no `get_links`); the contract exposes only `link_artifacts`. Storing them keeps provenance for when a read path is added.
- Completes the artifact store. Next: slice 4b `ContextProvider` (build/compress/summarize) writing artifacts here; then the production `OperalaDispatchInvoker` wiring all 5 providers into `DeepLoopCoordinator`.
