#[cfg(test)]
mod tests {
    use crate::{
        DigitalWorkerTemplate, DwAgentResolveRequest, DwCompositionResolveRequest,
        DwProviderCatalog, DwResolutionMode,
    };
    use greentic_cap_types::CapabilityId;
    use std::collections::BTreeMap;

    #[test]
    fn composition_resolver_uses_template_defaults_when_sufficient() {
        let template = DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {
                "required_capabilities": ["cap://llm/chat"],
                "default_provider_ids": {
                  "cap://llm/chat": "provider.llm.openai"
                }
              },
              "defaults": {
                "values": {
                  "display_name": "Support Assistant",
                  "temperature": 0.2
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {
                  "question_block_ids": ["dw.core.identity"]
                },
                "personalised_mode_behavior": {
                  "question_block_ids": ["dw.core.identity", "provider.llm.advanced"],
                  "include_optional_sections": true
                }
              }
            }"#,
        )
        .unwrap();

        let provider_catalog = DwProviderCatalog::from_json_str(
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
                  "version": "0.5.0",
                  "channel": "stable",
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"],
                    "pack_capability_ids": ["pack.llm.chat"]
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let request = DwCompositionResolveRequest {
            application_id: "dw.app.support".to_string(),
            display_name: "Support App".to_string(),
            version: Some("0.5.0".to_string()),
            tenant: Some("tenant-a".to_string()),
            tags: vec!["support".to_string()],
            agents: vec![DwAgentResolveRequest {
                agent_id: "support-assistant".to_string(),
                display_name: None,
                template,
                selected_template: None,
                answers: BTreeMap::new(),
                provider_overrides: BTreeMap::new(),
            }],
            shared_provider_overrides: BTreeMap::new(),
            mode: Some(DwResolutionMode::Recommended),
        };

        let document = request.resolve(&provider_catalog).unwrap();
        assert_eq!(document.agents.len(), 1);
        assert_eq!(
            document.agents[0].capability_bindings[0]
                .provider_binding
                .provider_id,
            "provider.llm.openai"
        );
        assert!(document.unresolved_setup_items.is_empty());
        assert_eq!(
            document.agents[0]
                .behavior_config
                .enabled_question_block_ids,
            vec!["dw.core.identity".to_string()]
        );
    }

    #[test]
    fn composition_resolver_allows_review_all_provider_overrides() {
        let template = DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.approval-worker",
                "name": "Approval Worker",
                "summary": "Routes approvals.",
                "maturity": "beta"
              },
              "capability_plan": {
                "required_capabilities": ["cap://llm/chat"],
                "default_provider_ids": {
                  "cap://llm/chat": "provider.llm.default"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {
                  "question_block_ids": ["provider.override"],
                  "include_optional_sections": true,
                  "allow_provider_overrides": true
                }
              }
            }"#,
        )
        .unwrap();

        let provider_catalog = DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.default",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "Default LLM",
                  "summary": "Default provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/default:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable"
                },
                {
                  "provider_id": "provider.llm.mini",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "Mini LLM",
                  "summary": "Override provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/mini:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable"
                }
              ]
            }"#,
        )
        .unwrap();

        let request = DwCompositionResolveRequest {
            application_id: "dw.app.approval".to_string(),
            display_name: "Approval App".to_string(),
            version: None,
            tenant: None,
            tags: Vec::new(),
            agents: vec![DwAgentResolveRequest {
                agent_id: "approval-worker".to_string(),
                display_name: None,
                template,
                selected_template: None,
                answers: BTreeMap::new(),
                provider_overrides: BTreeMap::from([(
                    CapabilityId::new("cap://llm/chat").unwrap(),
                    "provider.llm.mini".to_string(),
                )]),
            }],
            shared_provider_overrides: BTreeMap::new(),
            mode: Some(DwResolutionMode::ReviewAll),
        };

        let document = request.resolve(&provider_catalog).unwrap();
        assert_eq!(
            document.agents[0].capability_bindings[0]
                .provider_binding
                .provider_id,
            "provider.llm.mini"
        );
        assert_eq!(
            document.agents[0]
                .behavior_config
                .enabled_question_block_ids,
            vec!["provider.override".to_string()]
        );
    }

    #[test]
    fn composition_resolver_surfaces_unresolved_items() {
        let template = DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.memory-worker",
                "name": "Memory Worker",
                "summary": "Needs memory.",
                "maturity": "stable"
              },
              "capability_plan": {
                "required_capabilities": ["cap://memory/short-term"]
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();

        let request = DwCompositionResolveRequest {
            application_id: "dw.app.memory".to_string(),
            display_name: "Memory App".to_string(),
            version: None,
            tenant: None,
            tags: Vec::new(),
            agents: vec![DwAgentResolveRequest {
                agent_id: "memory-worker".to_string(),
                display_name: None,
                template,
                selected_template: None,
                answers: BTreeMap::new(),
                provider_overrides: BTreeMap::new(),
            }],
            shared_provider_overrides: BTreeMap::new(),
            mode: Some(DwResolutionMode::Recommended),
        };

        let document = request.resolve(&DwProviderCatalog::default()).unwrap();
        assert_eq!(document.agents[0].capability_bindings.len(), 0);
        assert_eq!(document.unresolved_setup_items.len(), 1);
        assert!(
            document.unresolved_setup_items[0]
                .summary
                .contains("No provider selected")
        );
    }

    #[test]
    fn composition_resolver_supports_shared_provider_defaults_across_agents() {
        let template = DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Shared llm.",
                "maturity": "stable"
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

        let provider_catalog = DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.shared",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "Shared LLM",
                  "summary": "Shared provider",
                  "source_ref": {
                    "raw_ref": "repo://providers/llm/shared",
                    "kind": "repo"
                  },
                  "maturity": "stable"
                }
              ]
            }"#,
        )
        .unwrap();

        let request = DwCompositionResolveRequest {
            application_id: "dw.app.shared".to_string(),
            display_name: "Shared App".to_string(),
            version: None,
            tenant: None,
            tags: Vec::new(),
            agents: vec![
                DwAgentResolveRequest {
                    agent_id: "agent-a".to_string(),
                    display_name: None,
                    template: template.clone(),
                    selected_template: None,
                    answers: BTreeMap::new(),
                    provider_overrides: BTreeMap::new(),
                },
                DwAgentResolveRequest {
                    agent_id: "agent-b".to_string(),
                    display_name: None,
                    template,
                    selected_template: None,
                    answers: BTreeMap::new(),
                    provider_overrides: BTreeMap::new(),
                },
            ],
            shared_provider_overrides: BTreeMap::from([(
                CapabilityId::new("cap://llm/chat").unwrap(),
                "provider.llm.shared".to_string(),
            )]),
            mode: Some(DwResolutionMode::Recommended),
        };

        let document = request.resolve(&provider_catalog).unwrap();
        assert_eq!(document.agents.len(), 2);
        assert_eq!(document.shared_pack_dependencies.len(), 1);
        assert_eq!(
            document.shared_pack_dependencies[0].applies_to_agents,
            vec!["agent-a".to_string(), "agent-b".to_string()]
        );
        assert!(document.output_plan.supports_multi_agent_app_pack);
    }

    #[test]
    fn resolution_mode_deserializes_legacy_names() {
        let recommended: DwResolutionMode = serde_json::from_str("\"default\"").unwrap();
        let review_all: DwResolutionMode = serde_json::from_str("\"personalised\"").unwrap();

        assert_eq!(recommended, DwResolutionMode::Recommended);
        assert_eq!(review_all, DwResolutionMode::ReviewAll);
        assert_eq!(
            serde_json::to_string(&DwResolutionMode::Recommended).unwrap(),
            "\"recommended\""
        );
        assert_eq!(
            serde_json::to_string(&DwResolutionMode::ReviewAll).unwrap(),
            "\"review_all\""
        );
    }
}
