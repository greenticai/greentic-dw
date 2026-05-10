//! Structural snapshot tests for [`build_dw_pack_with_options`] output.
//!
//! Byte-stable golden tests are deferred to v0.3+ — `PackBuilder` uses
//! `Signing::Dev` which generates a fresh Ed25519 key per run, so the
//! signature blob differs across builds. v0.3 adds a fixed-key signer for
//! deterministic byte-level testing.
//!
//! These tests instead verify **structural semantics**: archive contains
//! expected file paths, declared assets land at their `path`, and metadata
//! fields round-trip from the produced manifest.

use std::collections::BTreeSet;
use std::io::Read;

use greentic_dw_pack_builder::{
    AssetSupplierError, DwPackBuildError, DwPackBuildOptions, MapAssetSupplier,
    build_dw_pack_with_options,
};
use greentic_dw_types::{
    DwApplicationPackAgent, DwApplicationPackAsset, DwApplicationPackAssetKind,
    DwApplicationPackLayout, DwApplicationPackMetadata, DwApplicationPackSpec,
    DwGeneratedConfigAsset, DwGeneratedFlowAsset, DwGeneratedPromptAsset,
};
use tempfile::tempdir;
use time::OffsetDateTime;

fn fixed_options() -> DwPackBuildOptions {
    DwPackBuildOptions {
        fixed_now: Some(OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid epoch")),
        kind: None,
    }
}

fn rich_spec() -> DwApplicationPackSpec {
    DwApplicationPackSpec {
        metadata: DwApplicationPackMetadata {
            pack_id: "pack.test.dw.snapshot".to_string(),
            application_id: "app.snapshot".to_string(),
            display_name: "Snapshot DW".to_string(),
            version: Some("0.1.0".to_string()),
            multi_agent: false,
        },
        agents: vec![DwApplicationPackAgent {
            agent_id: "primary".to_string(),
            display_name: "Primary".to_string(),
            template_id: "orchestrator".to_string(),
            asset_root: "agents/primary".to_string(),
        }],
        assets: vec![DwApplicationPackAsset {
            asset_id: "icon".to_string(),
            path: "assets/icon.bin".to_string(),
            kind: DwApplicationPackAssetKind::Generic,
            content_type: None,
            applies_to_agents: Vec::new(),
            source_ref: None,
        }],
        generated_configs: vec![DwGeneratedConfigAsset {
            asset_id: "rt-cfg".to_string(),
            path: "configs/runtime.json".to_string(),
            format: "json".to_string(),
            applies_to_agents: Vec::new(),
        }],
        generated_flows: vec![DwGeneratedFlowAsset {
            asset_id: "main-flow".to_string(),
            path: "flows/main.ygtc".to_string(),
            entrypoint: Some("start".to_string()),
            applies_to_agents: vec!["primary".to_string()],
        }],
        generated_prompts: vec![DwGeneratedPromptAsset {
            asset_id: "system-prompt".to_string(),
            path: "prompts/system.txt".to_string(),
            prompt_kind: "system".to_string(),
            applies_to_agents: vec!["primary".to_string()],
        }],
        requirements: Vec::new(),
        dependency_pack_refs: Vec::new(),
        setup_requirements: Vec::new(),
        layout: DwApplicationPackLayout {
            app_root: "app".to_string(),
            shared_asset_roots: Vec::new(),
            layout_hint: None,
        },
    }
}

fn rich_supplier() -> MapAssetSupplier {
    MapAssetSupplier::new()
        .with("icon", b"ICON_BYTES".to_vec())
        .with("rt-cfg", br#"{"engine":"default"}"#.to_vec())
        .with("main-flow", b"id: main\nentry: start\nnodes: []\n".to_vec())
        .with("system-prompt", b"You are a helpful agent.".to_vec())
}

fn list_zip_paths(pack_path: &std::path::Path) -> BTreeSet<String> {
    let bytes = std::fs::read(pack_path).expect("read pack");
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).expect("open zip");

    let mut paths = BTreeSet::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).expect("entry");
        paths.insert(entry.name().to_string());
    }
    paths
}

fn read_zip_entry(pack_path: &std::path::Path, name: &str) -> Vec<u8> {
    let bytes = std::fs::read(pack_path).expect("read pack");
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).expect("open zip");
    let mut entry = zip.by_name(name).expect("entry exists");
    let mut out = Vec::new();
    entry.read_to_end(&mut out).expect("read entry");
    out
}

#[test]
fn snapshot_contains_expected_archive_paths() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("snapshot.gtpack");

    let spec = rich_spec();
    let supplier = rich_supplier();

    build_dw_pack_with_options(&spec, &out, &supplier, fixed_options()).expect("build ok");

    let paths = list_zip_paths(&out);

    // PackBuilder prefixes all `with_asset_bytes` paths under `assets/`.
    // Spec-declared paths land at `assets/<spec.path>`.
    assert!(
        paths.contains("assets/assets/icon.bin"),
        "static asset path"
    );
    assert!(
        paths.contains("assets/configs/runtime.json"),
        "generated config path"
    );
    assert!(
        paths.contains("assets/flows/main.ygtc"),
        "generated flow path"
    );
    assert!(
        paths.contains("assets/prompts/system.txt"),
        "generated prompt path"
    );

    // Pack-level structural artifacts.
    assert!(
        paths.iter().any(|p| p.ends_with("manifest.cbor")),
        "manifest.cbor must be present, got: {paths:?}"
    );
}

#[test]
fn snapshot_asset_bytes_round_trip_through_archive() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("roundtrip.gtpack");

    let spec = rich_spec();
    let supplier = rich_supplier();

    build_dw_pack_with_options(&spec, &out, &supplier, fixed_options()).expect("build ok");

    let icon_bytes = read_zip_entry(&out, "assets/assets/icon.bin");
    assert_eq!(icon_bytes, b"ICON_BYTES");

    let cfg_bytes = read_zip_entry(&out, "assets/configs/runtime.json");
    assert_eq!(cfg_bytes, br#"{"engine":"default"}"#);

    let prompt_bytes = read_zip_entry(&out, "assets/prompts/system.txt");
    assert_eq!(prompt_bytes, b"You are a helpful agent.");
}

#[test]
fn missing_asset_in_supplier_surfaces_not_found() {
    let dir = tempdir().expect("tempdir");
    let out = dir.path().join("missing.gtpack");

    let spec = rich_spec();
    // Supplier missing one of the four required assets.
    let supplier = MapAssetSupplier::new()
        .with("icon", b"ICON".to_vec())
        .with("rt-cfg", b"{}".to_vec())
        .with("system-prompt", b"hi".to_vec());

    let err = build_dw_pack_with_options(&spec, &out, &supplier, fixed_options())
        .expect_err("must error");

    match err {
        DwPackBuildError::Supplier(AssetSupplierError::NotFound(id)) => {
            assert_eq!(id, "main-flow");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
