#[cfg(test)]
mod tests {
    use crate::{
        DwProviderCatalog, DwProviderCatalogError, DwProviderEnvironmentSuitability, SourceRefKind,
    };
    use schemars::schema_for;

    #[test]
    fn provider_catalog_lists_by_family_and_recommendation() {
        let catalog = DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.openai",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat",
                  "summary": "Managed LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"],
                    "pack_capability_ids": ["pack.llm.chat"]
                  },
                  "default_profile": {
                    "is_default_choice": true,
                    "recommended_for_families": ["llm"]
                  },
                  "suitability": ["dev", "enterprise", "prod"]
                },
                {
                  "provider_id": "provider.memory.redis",
                  "family": "memory",
                  "category": "short_term",
                  "display_name": "Redis Memory",
                  "summary": "Short-term memory provider",
                  "source_ref": {
                    "raw_ref": "repo://providers/memory/redis",
                    "kind": "repo"
                  },
                  "maturity": "beta",
                  "default_profile": {
                    "is_recommended_choice": true
                  },
                  "suitability": ["local", "dev", "demo"]
                }
              ]
            }"#,
        )
        .unwrap();

        let llm = catalog.list_by_family("llm");
        assert_eq!(llm.len(), 1);
        assert_eq!(llm[0].provider_id, "provider.llm.openai");

        let recommended = catalog.recommended_for_family("llm");
        assert_eq!(recommended.len(), 1);
        assert_eq!(recommended[0].provider_id, "provider.llm.openai");
    }

    #[test]
    fn provider_catalog_filters_by_environment_and_resolves_source_refs() {
        let catalog = DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.tool.local-shell",
                  "family": "tool",
                  "category": "shell",
                  "display_name": "Local Shell",
                  "summary": "Local dev shell tool",
                  "source_ref": {
                    "raw_ref": "./packs/tool/local-shell.pack",
                    "kind": "local_path"
                  },
                  "maturity": "experimental",
                  "suitability": ["local", "dev"]
                }
              ]
            }"#,
        )
        .unwrap();

        let dev_entries = catalog.list_by_suitability(DwProviderEnvironmentSuitability::Dev);
        assert_eq!(dev_entries.len(), 1);
        assert_eq!(dev_entries[0].provider_id, "provider.tool.local-shell");

        let source = catalog
            .resolve_source_ref("provider.tool.local-shell")
            .unwrap();
        assert_eq!(source.source.source.kind, SourceRefKind::LocalPath);
    }

    #[test]
    fn provider_catalog_rejects_missing_provider_lookup() {
        let catalog = DwProviderCatalog::default();
        let err = catalog.resolve_source_ref("provider.missing").unwrap_err();
        assert!(matches!(err, DwProviderCatalogError::NotFound { .. }));
    }

    #[test]
    fn provider_catalog_schema_is_exportable() {
        let schema = schema_for!(DwProviderCatalog);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("provider_id"));
        assert!(schema_text.contains("capability_profile"));
        assert!(schema_text.contains("suitability"));
    }
}
