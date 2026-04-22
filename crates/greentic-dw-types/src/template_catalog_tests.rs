#[cfg(test)]
mod tests {
    use crate::{
        DigitalWorkerTemplate, TemplateCatalog, TemplateCatalogError, TemplateModeSuitability,
        TemplateSourceRef,
    };
    use schemars::schema_for;
    use std::fs;
    use tempfile::{NamedTempFile, tempdir};

    #[test]
    fn template_can_generate_catalog_entry() {
        let template = DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "beta"
              },
              "capability_plan": {
                "required_capabilities": ["cap://llm/chat"]
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();

        let entry = template.to_catalog_entry(
            TemplateSourceRef::from_raw("./examples/templates/support-assistant.json").unwrap(),
            Some("0.5.0".to_string()),
            TemplateModeSuitability::BothModes,
        );

        assert_eq!(entry.template_id, "dw.support-assistant");
        assert_eq!(entry.capability_summary.required_capabilities.len(), 1);
        assert!(entry.supports_multi_agent_app_pack);
    }

    #[test]
    fn template_catalog_loads_and_resolves_local_templates() {
        let template_file = NamedTempFile::new().unwrap();
        fs::write(
            template_file.path(),
            r#"{
              "metadata": {
                "id": "dw.local-template",
                "name": "Local Template",
                "summary": "Local template file.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();

        let catalog = TemplateCatalog::from_json_str(&format!(
            r#"{{
              "entries": [
                {{
                  "template_id": "dw.local-template",
                  "display_name": "Local Template",
                  "summary": "Local template file.",
                  "source_ref": {{
                    "raw_ref": "{}",
                    "kind": "local_path"
                  }},
                  "maturity": "stable",
                  "mode_suitability": "both_modes"
                }}
              ]
            }}"#,
            template_file.path().display()
        ))
        .unwrap();

        let resolved = catalog.resolve_template("dw.local-template").unwrap();
        assert_eq!(resolved.metadata.name, "Local Template");
    }

    #[test]
    fn template_catalog_rejects_remote_local_resolution() {
        let catalog = TemplateCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "template_id": "dw.remote-template",
                  "display_name": "Remote Template",
                  "summary": "Remote descriptor",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/templates/support:latest",
                    "kind": "oci"
                  },
                  "maturity": "beta",
                  "mode_suitability": "both_modes"
                }
              ]
            }"#,
        )
        .unwrap();

        let err = catalog.resolve_template("dw.remote-template").unwrap_err();
        assert!(matches!(
            err,
            TemplateCatalogError::UnsupportedLocalResolution { .. }
        ));
    }

    #[test]
    fn template_catalog_schema_is_exportable() {
        let schema = schema_for!(TemplateCatalog);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("mode_suitability"));
        assert!(schema_text.contains("capability_summary"));
        assert!(schema_text.contains("source_ref"));
    }

    #[test]
    fn template_catalog_rejects_paths_outside_catalog_root() {
        let temp_dir = tempdir().unwrap();
        let catalog_dir = temp_dir.path().join("catalog");
        let outside_dir = temp_dir.path().join("outside");
        fs::create_dir_all(&catalog_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();

        let outside_template = outside_dir.join("template.json");
        fs::write(
            &outside_template,
            r#"{
              "metadata": {
                "id": "dw.outside-template",
                "name": "Outside Template",
                "summary": "Outside the catalog root.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();

        let catalog_file = catalog_dir.join("catalog.json");
        fs::write(
            &catalog_file,
            r#"{
              "entries": [
                {
                  "template_id": "dw.outside-template",
                  "display_name": "Outside Template",
                  "summary": "Outside the catalog root.",
                  "source_ref": {
                    "raw_ref": "../outside/template.json",
                    "kind": "local_path"
                  },
                  "maturity": "stable",
                  "mode_suitability": "both_modes"
                }
              ]
            }"#,
        )
        .unwrap();

        let catalog = TemplateCatalog::from_json_path(&catalog_file).unwrap();
        let err = catalog.resolve_template("dw.outside-template").unwrap_err();
        assert!(matches!(
            err,
            TemplateCatalogError::EscapesCatalogRoot { .. }
        ));
    }

    #[test]
    fn template_catalog_rejects_missing_paths_that_lexically_escape_catalog_root() {
        let temp_dir = tempdir().unwrap();
        let catalog_dir = temp_dir.path().join("catalog");
        fs::create_dir_all(&catalog_dir).unwrap();

        let catalog_file = catalog_dir.join("catalog.json");
        fs::write(
            &catalog_file,
            r#"{
              "entries": [
                {
                  "template_id": "dw.outside-template",
                  "display_name": "Outside Template",
                  "summary": "Outside the catalog root.",
                  "source_ref": {
                    "raw_ref": "../outside/missing-template.json",
                    "kind": "local_path"
                  },
                  "maturity": "stable",
                  "mode_suitability": "both_modes"
                }
              ]
            }"#,
        )
        .unwrap();

        let catalog = TemplateCatalog::from_json_path(&catalog_file).unwrap();
        let err = catalog.resolve_template("dw.outside-template").unwrap_err();
        assert!(matches!(
            err,
            TemplateCatalogError::EscapesCatalogRoot { .. }
        ));
    }
}
