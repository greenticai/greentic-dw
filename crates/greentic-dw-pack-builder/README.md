# greentic-dw-pack-builder

Build `.gtpack` archives from `DwApplicationPackSpec`.

This crate bridges `greentic-dw-types::DwApplicationPackSpec` (the resolved DW handoff contract) and `greentic-pack-lib::PackBuilder` (the canonical pack writer). Designer / wizard pipelines call `build_dw_pack(&spec, out_path)` to materialize a tenant-installable `.gtpack` ZIP.

## Status

**v0.1 (Phase B.2):** First scaffold. Uses `PackKind::Application` placeholder until `greentic-pack` adds `PackKind::DwApplication` (Phase B.1).

## API

```rust
use greentic_dw_pack_builder::build_dw_pack;
use greentic_dw_types::DwApplicationPackSpec;

let spec: DwApplicationPackSpec = /* from DwCompositionDocument::to_application_pack_spec()? */;
let result = build_dw_pack(&spec, "./out/myworker.gtpack")?;
println!("built {}", result.pack_path.display());
```

## Layout produced

```
<pack>.gtpack/                      (ZIP archive)
├── manifest.cbor                   PackMeta + components index
├── sbom.cbor                       Software bill of materials
├── flows/                          Generated YGTC flows (per agent)
├── components/                     Wasm components (none in v0.1 — DW packs reference upstream)
├── assets/                         Static assets from spec.assets
├── configs/                        Generated configs from spec.generated_configs
├── prompts/                        Generated prompts from spec.generated_prompts
└── signatures/pack.sig             Ed25519 signature (dev key in v0.1)
```

## Plan reference

- Plan: `docs/superpowers/plans/2026-05-08-dw-composer-vertical-slice.md` task B.2 (in greentic-designer repo)
- Architecture: `docs/superpowers/research/2026-05-09-dw-composer-extension-contract.md`

## License

MIT
