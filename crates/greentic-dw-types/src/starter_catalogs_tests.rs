#[cfg(test)]
mod tests {
    use crate::{
        DwAgentResolveRequest, DwCompositionResolveRequest, DwProviderCatalog, DwResolutionMode,
        TemplateCatalog,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn workspace_examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .canonicalize()
            .expect("workspace examples dir")
    }

    #[test]
    fn starter_template_catalog_loads_expected_entries() {
        let catalog_path = workspace_examples_dir().join("templates/catalog.json");
        let catalog = TemplateCatalog::from_json_path(&catalog_path).expect("load starter catalog");

        assert_eq!(catalog.entries.len(), 3);
        assert!(catalog.find("dw.support-assistant").is_some());
        assert!(catalog.find("dw.approval-worker").is_some());
        assert!(catalog.find("dw.workflow-executor").is_some());
    }

    #[test]
    fn starter_provider_catalog_loads_expected_families() {
        let catalog_path = workspace_examples_dir().join("providers/catalog.json");
        let catalog =
            DwProviderCatalog::from_json_path(&catalog_path).expect("load provider catalog");

        assert_eq!(catalog.entries.len(), 8);
        assert_eq!(catalog.list_by_family("engine").len(), 1);
        assert_eq!(catalog.list_by_family("llm").len(), 2);
        assert_eq!(catalog.list_by_family("memory").len(), 1);
        assert_eq!(catalog.list_by_family("control").len(), 1);
        assert_eq!(catalog.list_by_family("observer").len(), 1);
        assert_eq!(catalog.list_by_family("tool").len(), 1);
        assert_eq!(catalog.list_by_family("task-store").len(), 1);
    }

    #[test]
    fn starter_catalogs_drive_resolve_pack_and_bundle_flow() {
        let examples_dir = workspace_examples_dir();
        let template_catalog =
            TemplateCatalog::from_json_path(examples_dir.join("templates/catalog.json"))
                .expect("load starter template catalog");
        let provider_catalog =
            DwProviderCatalog::from_json_path(examples_dir.join("providers/catalog.json"))
                .expect("load starter provider catalog");

        let selected_template = template_catalog
            .find("dw.support-assistant")
            .cloned()
            .expect("starter support template entry");
        let template = template_catalog
            .resolve_template("dw.support-assistant")
            .expect("resolve support template");

        let request = DwCompositionResolveRequest {
            application_id: "dw.app.support-starter".to_string(),
            display_name: "Starter Support App".to_string(),
            version: Some("0.5.0".to_string()),
            tenant: Some("tenant-starter".to_string()),
            tags: vec!["starter".to_string(), "support".to_string()],
            agents: vec![DwAgentResolveRequest {
                agent_id: "support-assistant".to_string(),
                display_name: None,
                template,
                selected_template: Some(selected_template),
                answers: BTreeMap::new(),
                provider_overrides: BTreeMap::new(),
            }],
            shared_provider_overrides: BTreeMap::new(),
            mode: Some(DwResolutionMode::Default),
        };

        let composition = request
            .resolve(&provider_catalog)
            .expect("resolve composition");
        assert!(!composition.agents.is_empty());
        assert!(composition.unresolved_setup_items.is_empty());
        assert!(composition.shared_pack_dependencies.len() >= 3);

        let pack_spec = composition
            .to_application_pack_spec()
            .expect("materialize app pack");
        assert_eq!(pack_spec.agents.len(), 1);
        assert!(!pack_spec.dependency_pack_refs.is_empty());

        let bundle_plan = composition.to_bundle_plan().expect("generate bundle plan");
        assert_eq!(
            bundle_plan.generated_app_pack.pack_id,
            pack_spec.metadata.pack_id
        );
        assert!(!bundle_plan.provider_packs.is_empty());
        assert!(!bundle_plan.inclusions.is_empty());
    }
}
