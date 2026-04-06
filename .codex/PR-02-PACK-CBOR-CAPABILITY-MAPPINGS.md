# PR-02: Add `pack.cbor` capability mapping support to greentic-dw

## Objective
Make DW runtime/schema aware of capability declarations and operation mappings coming from `pack.cbor`.

## Scope
- Define the pack-facing capability declaration shape expected by DW-related assets
- Support capability declarations with:
  - `offers`
  - `requires`
  - `consumes`
- For offered capabilities, support mapping to provider component operations

## Example direction
```yaml
capabilities:
  offers:
    - id: cap://dw.memory.short-term
      provider_component: memory.redis
      operation_map:
        get: memory.get
        put: memory.put
        delete: memory.delete
        clear: memory.clear
```

## Validation goals
- capability id exists / is syntactically valid
- provider component exists
- mapped operations exist in self-description
- operation signatures are compatible with the capability contract

## Deliverables
- Rust types/parsers for capability declarations relevant to DW
- validation helpers
- docs/examples
- keep any crate/example version references aligned with the workspace version source of truth in the root `Cargo.toml`
- prefer `../greentic-cap` path dependencies for the shared capability types, schema, profile, and resolver crates
