#[cfg(test)]
mod tests {
    use crate::{
        AgentLocalBindingOverride, ApplicationPackLayoutHints, BehaviorConfig, BundleInclusionPlan,
        CapabilityBinding, CompositionSourceProvenance, DwAgentComposition, DwApplication,
        DwApplicationAgentRef, DwCompositionApplicationMetadata, DwCompositionDocument,
        DwCompositionOutputPlan, DwProviderSourceRef, InterAgentRoutingConfig, PackDependencyRef,
        PackSourceRef, ProviderBinding, SetupRequirement, SetupRequirementStatus,
        SharedCapabilityBinding, TemplateSourceRef,
    };
    use greentic_cap_types::CapabilityId;
    use schemars::schema_for;
    use std::collections::BTreeMap;

    #[test]
    fn composition_document_supports_multiple_agents() {
        let llm_provider = ProviderBinding {
            provider_id: "provider.llm.openai".to_string(),
            source_ref: DwProviderSourceRef::from_raw(
                "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
            )
            .unwrap(),
            version: Some("0.5.0".to_string()),
            channel: Some("stable".to_string()),
            provider_family: Some("llm".to_string()),
            provider_category: Some("chat".to_string()),
        };
        let observer_provider = ProviderBinding {
            provider_id: "provider.observer.audit".to_string(),
            source_ref: DwProviderSourceRef::from_raw("repo://providers/observer/audit").unwrap(),
            version: None,
            channel: None,
            provider_family: Some("observer".to_string()),
            provider_category: Some("audit".to_string()),
        };

        let document = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.support-suite".to_string(),
                display_name: "Support Suite".to_string(),
                version: Some("0.5.0".to_string()),
                tenant: Some("tenant-a".to_string()),
                tags: vec!["support".to_string(), "multi-agent".to_string()],
            },
            agents: vec![
                DwAgentComposition {
                    agent_id: "support-assistant".to_string(),
                    display_name: "Support Assistant".to_string(),
                    template_id: "dw.support-assistant".to_string(),
                    selected_template: None,
                    capability_bindings: vec![CapabilityBinding {
                        capability_id: CapabilityId::new("cap://llm/chat").unwrap(),
                        provider_binding: llm_provider.clone(),
                        optional: false,
                        pack_capability_id: Some("pack.llm.chat".to_string()),
                    }],
                    behavior_config: BehaviorConfig {
                        enabled_question_block_ids: vec!["dw.core.identity".to_string()],
                        values: BTreeMap::from([(
                            "temperature".to_string(),
                            serde_json::json!(0.2),
                        )]),
                    },
                    source_provenance: CompositionSourceProvenance {
                        template_source_ref: Some(
                            TemplateSourceRef::from_raw("repo://templates/support-assistant")
                                .unwrap(),
                        ),
                        provider_source_refs: vec![llm_provider.source_ref.clone()],
                    },
                },
                DwAgentComposition {
                    agent_id: "approval-worker".to_string(),
                    display_name: "Approval Worker".to_string(),
                    template_id: "dw.approval-worker".to_string(),
                    selected_template: None,
                    capability_bindings: vec![CapabilityBinding {
                        capability_id: CapabilityId::new("cap://observer/audit").unwrap(),
                        provider_binding: observer_provider.clone(),
                        optional: true,
                        pack_capability_id: Some("pack.observer.audit".to_string()),
                    }],
                    behavior_config: BehaviorConfig::default(),
                    source_provenance: CompositionSourceProvenance {
                        template_source_ref: Some(
                            TemplateSourceRef::from_raw("repo://templates/approval-worker")
                                .unwrap(),
                        ),
                        provider_source_refs: vec![observer_provider.source_ref.clone()],
                    },
                },
            ],
            shared_pack_dependencies: vec![PackDependencyRef {
                pack_id: "pack.shared.audit".to_string(),
                source_ref: PackSourceRef::from_raw("repo://packs/shared/audit").unwrap(),
                version: Some("0.5.0".to_string()),
                provider_id: Some("provider.observer.audit".to_string()),
                applies_to_agents: vec![
                    "support-assistant".to_string(),
                    "approval-worker".to_string(),
                ],
            }],
            bundle_plan: vec![BundleInclusionPlan {
                pack_id: "pack.generated.support-suite".to_string(),
                source_ref: PackSourceRef::from_raw("./dist/support-suite.pack").unwrap(),
                applies_to_agents: vec![
                    "support-assistant".to_string(),
                    "approval-worker".to_string(),
                ],
                rationale: Some("Generated application pack for all agents".to_string()),
            }],
            unresolved_setup_items: vec![SetupRequirement {
                requirement_id: "setup.llm.api-key".to_string(),
                status: SetupRequirementStatus::Required,
                summary: "Provide an API key for the LLM provider".to_string(),
                provider_id: Some("provider.llm.openai".to_string()),
                setup_schema_ref: Some(
                    TemplateSourceRef::from_raw("repo://setup/llm/openai-api-key").unwrap(),
                ),
                question_block_id: Some("provider.llm.credentials".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            output_plan: DwCompositionOutputPlan {
                generated_pack_id: Some("pack.generated.support-suite".to_string()),
                generated_bundle_id: Some("bundle.support-suite".to_string()),
                supports_multi_agent_app_pack: true,
            },
            source_provenance: Some(CompositionSourceProvenance {
                template_source_ref: None,
                provider_source_refs: vec![llm_provider.source_ref, observer_provider.source_ref],
            }),
        };

        assert_eq!(document.agents.len(), 2);
        assert_eq!(document.agents[0].capability_bindings.len(), 1);
        assert_eq!(document.shared_pack_dependencies.len(), 1);
        assert_eq!(document.unresolved_setup_items.len(), 1);
        assert!(document.output_plan.supports_multi_agent_app_pack);
    }

    #[test]
    fn composition_document_serializes_selected_provider_and_source_refs() {
        let document = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.single".to_string(),
                display_name: "Single Agent App".to_string(),
                version: None,
                tenant: None,
                tags: Vec::new(),
            },
            agents: vec![DwAgentComposition {
                agent_id: "agent-1".to_string(),
                display_name: "Agent 1".to_string(),
                template_id: "dw.template".to_string(),
                selected_template: None,
                capability_bindings: vec![CapabilityBinding {
                    capability_id: CapabilityId::new("cap://llm/chat").unwrap(),
                    provider_binding: ProviderBinding {
                        provider_id: "provider.llm.openai".to_string(),
                        source_ref: DwProviderSourceRef::from_raw(
                            "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                        )
                        .unwrap(),
                        version: None,
                        channel: None,
                        provider_family: None,
                        provider_category: None,
                    },
                    optional: false,
                    pack_capability_id: None,
                }],
                behavior_config: BehaviorConfig::default(),
                source_provenance: CompositionSourceProvenance {
                    template_source_ref: Some(
                        TemplateSourceRef::from_raw("repo://templates/template").unwrap(),
                    ),
                    provider_source_refs: vec![
                        DwProviderSourceRef::from_raw(
                            "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                        )
                        .unwrap(),
                    ],
                },
            }],
            shared_pack_dependencies: Vec::new(),
            bundle_plan: Vec::new(),
            unresolved_setup_items: Vec::new(),
            output_plan: DwCompositionOutputPlan::default(),
            source_provenance: None,
        };

        let text = serde_json::to_value(&document).unwrap().to_string();
        assert!(text.contains("provider.llm.openai"));
        assert!(text.contains("oci://ghcr.io/greenticai/packs/providers/llm/openai:latest"));
        assert!(text.contains("repo://templates/template"));
    }

    #[test]
    fn composition_schema_is_exportable() {
        let schema = schema_for!(DwCompositionDocument);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("application_id"));
        assert!(schema_text.contains("capability_bindings"));
        assert!(schema_text.contains("unresolved_setup_items"));
        assert!(schema_text.contains("bundle_plan"));
    }

    #[test]
    fn application_model_supports_shared_and_local_bindings() {
        let shared_llm = SharedCapabilityBinding {
            binding_id: "shared.llm".to_string(),
            capability_id: CapabilityId::new("cap://llm/chat").unwrap(),
            provider_binding: ProviderBinding {
                provider_id: "provider.llm.openai".to_string(),
                source_ref: DwProviderSourceRef::from_raw(
                    "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                )
                .unwrap(),
                version: Some("0.5.0".to_string()),
                channel: Some("stable".to_string()),
                provider_family: Some("llm".to_string()),
                provider_category: Some("chat".to_string()),
            },
            pack_capability_id: Some("pack.llm.chat".to_string()),
            applies_to_agents: vec![
                "support-assistant".to_string(),
                "approval-worker".to_string(),
            ],
        };

        let app = DwApplication {
            application_id: "dw.app.support-suite".to_string(),
            display_name: "Support Suite".to_string(),
            agents: vec![
                DwApplicationAgentRef {
                    agent_id: "support-assistant".to_string(),
                    display_name: "Support Assistant".to_string(),
                    template_id: "dw.support-assistant".to_string(),
                    local_binding_overrides: Vec::new(),
                    behavior_config: BehaviorConfig::default(),
                    asset_roots: vec!["agents/support-assistant".to_string()],
                },
                DwApplicationAgentRef {
                    agent_id: "approval-worker".to_string(),
                    display_name: "Approval Worker".to_string(),
                    template_id: "dw.approval-worker".to_string(),
                    local_binding_overrides: vec![AgentLocalBindingOverride {
                        shared_binding_id: "shared.llm".to_string(),
                        provider_binding: ProviderBinding {
                            provider_id: "provider.llm.openai.gpt5-mini".to_string(),
                            source_ref: DwProviderSourceRef::from_raw(
                                "oci://ghcr.io/greenticai/packs/providers/llm/openai-mini:latest",
                            )
                            .unwrap(),
                            version: Some("0.5.0".to_string()),
                            channel: Some("stable".to_string()),
                            provider_family: Some("llm".to_string()),
                            provider_category: Some("chat".to_string()),
                        },
                        pack_capability_id: Some("pack.llm.chat.mini".to_string()),
                    }],
                    behavior_config: BehaviorConfig {
                        enabled_question_block_ids: vec!["approval.routing".to_string()],
                        values: BTreeMap::new(),
                    },
                    asset_roots: vec!["agents/approval-worker".to_string()],
                },
            ],
            shared_bindings: vec![shared_llm],
            shared_support_pack_refs: vec![
                PackSourceRef::from_raw("repo://packs/shared/observer-audit").unwrap(),
            ],
            routing: Some(InterAgentRoutingConfig {
                allowed_routes: vec!["support-assistant->approval-worker".to_string()],
                coordinator_agent_id: Some("support-assistant".to_string()),
            }),
            layout_hint: Some(ApplicationPackLayoutHints::MultiAgentSharedProviders),
        };

        assert_eq!(app.agents.len(), 2);
        assert_eq!(app.shared_bindings.len(), 1);
        assert_eq!(app.agents[1].local_binding_overrides.len(), 1);
        assert_eq!(
            app.layout_hint,
            Some(ApplicationPackLayoutHints::MultiAgentSharedProviders)
        );
    }

    #[test]
    fn application_model_serializes_routing_and_asset_layout() {
        let app = DwApplication {
            application_id: "dw.app.single".to_string(),
            display_name: "Single Agent".to_string(),
            agents: vec![DwApplicationAgentRef {
                agent_id: "agent-1".to_string(),
                display_name: "Agent 1".to_string(),
                template_id: "dw.template".to_string(),
                local_binding_overrides: Vec::new(),
                behavior_config: BehaviorConfig::default(),
                asset_roots: vec!["agents/agent-1".to_string()],
            }],
            shared_bindings: Vec::new(),
            shared_support_pack_refs: Vec::new(),
            routing: Some(InterAgentRoutingConfig {
                allowed_routes: vec!["agent-1->agent-1".to_string()],
                coordinator_agent_id: Some("agent-1".to_string()),
            }),
            layout_hint: Some(ApplicationPackLayoutHints::SingleAgentPack),
        };

        let text = serde_json::to_value(&app).unwrap().to_string();
        assert!(text.contains("allowed_routes"));
        assert!(text.contains("agents/agent-1"));
        assert!(text.contains("single_agent_pack"));
    }

    #[test]
    fn application_schema_is_exportable() {
        let schema = schema_for!(DwApplication);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("shared_bindings"));
        assert!(schema_text.contains("local_binding_overrides"));
        assert!(schema_text.contains("layout_hint"));
        assert!(schema_text.contains("routing"));
    }
}
