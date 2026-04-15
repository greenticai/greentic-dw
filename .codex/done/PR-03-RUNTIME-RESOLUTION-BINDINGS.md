# PR-03: Add runtime support for resolved capability bindings

## Objective
Teach `greentic-dw` runtime to execute using resolved capability bindings rather than hard-coded provider assumptions.

## Scope
- Accept resolution output from bundle/setup/runtime context
- Bind consumer-facing capability names to provider components/operations
- Route runtime calls through capability bindings
- Keep task state/resume provider-agnostic

## Important concept
Resolution chooses a concrete provider ref using real Greentic refs such as:
- `oci://...`
- `store://...`
- `repo://...`
- `file://...`
- `./...`

DW runtime should consume the already-resolved binding result.

## Example binding output
```yaml
bindings:
  - consumer: component://dw-runtime
    binding: short_term_memory
    capability: cap://dw.memory.short-term
    provider_ref: oci://ghcr.io/greenticai/packs/dw-providers/memory/redis-short-term:latest
    provider_component: memory.redis
    operation_map:
      get: memory.get
      put: memory.put
```

## Deliverables
- runtime binding structures
- capability dispatch layer
- provider-agnostic resume/state access path
- tests with mock providers
- preserve workspace-managed versioning in all touched manifests, tests, and examples
- consume resolved capability output from the shared capability workspace rather than reimplementing the resolver here
