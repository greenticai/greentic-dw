//! Translation: [`DwApplicationPackSpec`] → [`PackMeta`].
//!
//! Mechanical mapping. Field-level decisions documented inline.

use greentic_dw_types::DwApplicationPackSpec;
use greentic_pack::builder::PackMeta;
use greentic_pack::kind::PackKind;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const DEFAULT_VERSION: &str = "0.1.0";

/// Options that adjust [`build_pack_meta`] output for testing /
/// deterministic builds.
#[derive(Debug, Clone, Default)]
pub struct DwPackBuildOptions {
    /// Override `created_at_utc`. `None` uses [`OffsetDateTime::now_utc`].
    pub fixed_now: Option<OffsetDateTime>,
    /// Override `kind`. Defaults to [`PackKind::Application`] in v0.1.
    /// Phase B.1 swaps default to `PackKind::DwApplication`.
    pub kind: Option<PackKind>,
}

/// Errors produced while translating spec → [`PackMeta`].
#[derive(Debug, Error)]
pub enum PackMetaBuildError {
    /// Spec must declare at least one agent.
    #[error("spec must include at least one agent (got 0)")]
    NoAgents,
    /// `version` failed semver parsing.
    #[error("version '{0}' is not valid semver: {1}")]
    InvalidVersion(String, String),
    /// `created_at_utc` couldn't be formatted.
    #[error("failed to format created_at_utc: {0}")]
    InvalidTimestamp(String),
}

/// Build a [`PackMeta`] from a [`DwApplicationPackSpec`].
pub(crate) fn build_pack_meta(
    spec: &DwApplicationPackSpec,
    options: &DwPackBuildOptions,
) -> Result<PackMeta, PackMetaBuildError> {
    if spec.agents.is_empty() {
        return Err(PackMetaBuildError::NoAgents);
    }

    let version_str = spec
        .metadata
        .version
        .clone()
        .unwrap_or_else(|| DEFAULT_VERSION.to_string());
    let version = version_str.parse().map_err(|e: semver::Error| {
        PackMetaBuildError::InvalidVersion(version_str.clone(), e.to_string())
    })?;

    let now = options.fixed_now.unwrap_or_else(OffsetDateTime::now_utc);
    let created_at_utc = now
        .format(&Rfc3339)
        .map_err(|e| PackMetaBuildError::InvalidTimestamp(e.to_string()))?;

    // v0.1: default to PackKind::Application as placeholder.
    // B.1 will introduce PackKind::DwApplication; swap default then.
    let kind = options.kind.clone().or(Some(PackKind::Application));

    // Entry flows: derive from generated_flows when present (use asset_id),
    // else fall back to deterministic `<agent_id>.entry` per agent.
    let entry_flows: Vec<String> = if spec.generated_flows.is_empty() {
        spec.agents
            .iter()
            .map(|agent| format!("{}.entry", agent.agent_id))
            .collect()
    } else {
        spec.generated_flows
            .iter()
            .map(|flow| flow.asset_id.clone())
            .collect()
    };

    Ok(PackMeta {
        pack_version: greentic_pack::builder::PACK_VERSION,
        pack_id: spec.metadata.pack_id.clone(),
        version,
        name: spec.metadata.display_name.clone(),
        kind,
        description: None,
        authors: Vec::new(),
        license: None,
        homepage: None,
        support: None,
        vendor: None,
        imports: Vec::new(),
        entry_flows,
        created_at_utc,
        events: None,
        repo: None,
        messaging: None,
        interfaces: Vec::new(),
        annotations: serde_json::Map::new(),
        distribution: None,
        components: Vec::new(),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use greentic_dw_types::{
        DwApplicationPackAgent, DwApplicationPackLayout, DwApplicationPackMetadata,
        DwApplicationPackSpec,
    };

    fn sample_spec() -> DwApplicationPackSpec {
        DwApplicationPackSpec {
            metadata: DwApplicationPackMetadata {
                pack_id: "pack.demo".to_string(),
                application_id: "app.demo".to_string(),
                display_name: "Demo".to_string(),
                version: Some("1.2.3".to_string()),
                multi_agent: false,
            },
            agents: vec![DwApplicationPackAgent {
                agent_id: "primary".to_string(),
                display_name: "Primary Agent".to_string(),
                template_id: "orchestrator".to_string(),
                asset_root: "agents/primary".to_string(),
            }],
            assets: Vec::new(),
            generated_configs: Vec::new(),
            generated_flows: Vec::new(),
            generated_prompts: Vec::new(),
            requirements: Vec::new(),
            dependency_pack_refs: Vec::new(),
            setup_requirements: Vec::new(),
            routing: None,
            layout: DwApplicationPackLayout {
                app_root: "app".to_string(),
                shared_asset_roots: Vec::new(),
                layout_hint: None,
            },
        }
    }

    #[test]
    fn build_pack_meta_uses_application_kind_by_default() {
        let spec = sample_spec();
        let meta = build_pack_meta(&spec, &DwPackBuildOptions::default()).expect("ok");
        assert_eq!(meta.kind, Some(PackKind::Application));
        assert_eq!(meta.pack_id, "pack.demo");
        assert_eq!(meta.name, "Demo");
    }

    #[test]
    fn build_pack_meta_derives_entry_flows_from_agents_when_no_flows() {
        let spec = sample_spec();
        let meta = build_pack_meta(&spec, &DwPackBuildOptions::default()).expect("ok");
        assert_eq!(meta.entry_flows, vec!["primary.entry"]);
    }

    #[test]
    fn build_pack_meta_uses_default_version_when_missing() {
        let mut spec = sample_spec();
        spec.metadata.version = None;
        let meta = build_pack_meta(&spec, &DwPackBuildOptions::default()).expect("ok");
        assert_eq!(meta.version.to_string(), DEFAULT_VERSION);
    }

    #[test]
    fn build_pack_meta_rejects_invalid_version() {
        let mut spec = sample_spec();
        spec.metadata.version = Some("not-semver".to_string());
        let err = build_pack_meta(&spec, &DwPackBuildOptions::default()).expect_err("must reject");
        match err {
            PackMetaBuildError::InvalidVersion(v, _) => assert_eq!(v, "not-semver"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn build_pack_meta_uses_fixed_now_when_provided() {
        let spec = sample_spec();
        let fixed = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid epoch");
        let opts = DwPackBuildOptions {
            fixed_now: Some(fixed),
            ..Default::default()
        };
        let meta = build_pack_meta(&spec, &opts).expect("ok");
        assert!(meta.created_at_utc.starts_with("2023-"));
    }

    #[test]
    fn build_pack_meta_rejects_empty_agents() {
        let mut spec = sample_spec();
        spec.agents.clear();
        let err = build_pack_meta(&spec, &DwPackBuildOptions::default()).expect_err("must reject");
        assert!(matches!(err, PackMetaBuildError::NoAgents));
    }
}
