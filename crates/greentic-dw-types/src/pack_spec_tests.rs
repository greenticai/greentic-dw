#[cfg(test)]
mod tests {
    use crate::{
        ApplicationPackLayoutHints, DwApplicationPackAgent, DwApplicationPackAsset,
        DwApplicationPackAssetKind, DwApplicationPackLayout, DwApplicationPackMaterializationError,
        DwApplicationPackMetadata, DwApplicationPackRequirement, DwApplicationPackSpec,
        DwCompositionApplicationMetadata, DwCompositionDocument, DwCompositionOutputPlan,
        DwGeneratedConfigAsset, DwGeneratedFlowAsset, DwGeneratedPromptAsset, PackDependencyRef,
        PackSourceRef, SetupRequirement, SetupRequirementStatus, TemplateSourceRef,
    };
    use greentic_cap_types::CapabilityId;
    use schemars::schema_for;

    #[test]
    fn app_pack_spec_supports_single_agent_shape() {
        let spec = DwApplicationPackSpec {
            metadata: DwApplicationPackMetadata {
                pack_id: "pack.dw.support".to_string(),
                application_id: "dw.app.support".to_string(),
                display_name: "Support App".to_string(),
                version: Some("0.5.0".to_string()),
                multi_agent: false,
            },
            agents: vec![DwApplicationPackAgent {
                agent_id: "support-assistant".to_string(),
                display_name: "Support Assistant".to_string(),
                template_id: "dw.support-assistant".to_string(),
                asset_root: "agents/support-assistant".to_string(),
            }],
            assets: vec![DwApplicationPackAsset {
                asset_id: "asset.config.agent".to_string(),
                path: "agents/support-assistant/config.json".to_string(),
                kind: DwApplicationPackAssetKind::Config,
                content_type: Some("application/json".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
                source_ref: Some(
                    TemplateSourceRef::from_raw("repo://templates/support-assistant").unwrap(),
                ),
            }],
            generated_configs: vec![DwGeneratedConfigAsset {
                asset_id: "cfg.agent".to_string(),
                path: "agents/support-assistant/config.json".to_string(),
                format: "json".to_string(),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            generated_flows: Vec::new(),
            generated_prompts: Vec::new(),
            requirements: vec![DwApplicationPackRequirement {
                requirement_id: "req.llm".to_string(),
                summary: "Chat capability required".to_string(),
                capability_id: Some(CapabilityId::new("cap://llm/chat").unwrap()),
                provider_id: Some("provider.llm.openai".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            dependency_pack_refs: vec![PackDependencyRef {
                pack_id: "provider.llm.openai".to_string(),
                source_ref: PackSourceRef::from_raw(
                    "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                )
                .unwrap(),
                version: Some("0.5.0".to_string()),
                provider_id: Some("provider.llm.openai".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            setup_requirements: vec![SetupRequirement {
                requirement_id: "setup.llm.key".to_string(),
                status: SetupRequirementStatus::Required,
                summary: "Provide API key".to_string(),
                provider_id: Some("provider.llm.openai".to_string()),
                setup_schema_ref: Some(
                    TemplateSourceRef::from_raw("repo://setup/llm/openai-api-key").unwrap(),
                ),
                question_block_id: Some("provider.llm.credentials".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            layout: DwApplicationPackLayout {
                app_root: "support.pack".to_string(),
                shared_asset_roots: Vec::new(),
                layout_hint: Some(ApplicationPackLayoutHints::SingleAgentPack),
            },
        };

        assert_eq!(spec.agents.len(), 1);
        assert_eq!(spec.dependency_pack_refs.len(), 1);
        assert_eq!(spec.generated_configs.len(), 1);
        assert!(!spec.metadata.multi_agent);
    }

    #[test]
    fn app_pack_spec_supports_multi_agent_shape_without_inlining_provider_impls() {
        let spec = DwApplicationPackSpec {
            metadata: DwApplicationPackMetadata {
                pack_id: "pack.dw.support-suite".to_string(),
                application_id: "dw.app.support-suite".to_string(),
                display_name: "Support Suite".to_string(),
                version: Some("0.5.0".to_string()),
                multi_agent: true,
            },
            agents: vec![
                DwApplicationPackAgent {
                    agent_id: "support-assistant".to_string(),
                    display_name: "Support Assistant".to_string(),
                    template_id: "dw.support-assistant".to_string(),
                    asset_root: "agents/support-assistant".to_string(),
                },
                DwApplicationPackAgent {
                    agent_id: "approval-worker".to_string(),
                    display_name: "Approval Worker".to_string(),
                    template_id: "dw.approval-worker".to_string(),
                    asset_root: "agents/approval-worker".to_string(),
                },
            ],
            assets: vec![
                DwApplicationPackAsset {
                    asset_id: "asset.flow.router".to_string(),
                    path: "flows/router.json".to_string(),
                    kind: DwApplicationPackAssetKind::Flow,
                    content_type: Some("application/json".to_string()),
                    applies_to_agents: vec![
                        "support-assistant".to_string(),
                        "approval-worker".to_string(),
                    ],
                    source_ref: None,
                },
                DwApplicationPackAsset {
                    asset_id: "asset.prompt.support".to_string(),
                    path: "agents/support-assistant/prompt.md".to_string(),
                    kind: DwApplicationPackAssetKind::Prompt,
                    content_type: Some("text/markdown".to_string()),
                    applies_to_agents: vec!["support-assistant".to_string()],
                    source_ref: None,
                },
            ],
            generated_configs: vec![DwGeneratedConfigAsset {
                asset_id: "cfg.shared".to_string(),
                path: "shared/runtime.json".to_string(),
                format: "json".to_string(),
                applies_to_agents: vec![
                    "support-assistant".to_string(),
                    "approval-worker".to_string(),
                ],
            }],
            generated_flows: vec![DwGeneratedFlowAsset {
                asset_id: "flow.router".to_string(),
                path: "flows/router.json".to_string(),
                entrypoint: Some("support-assistant".to_string()),
                applies_to_agents: vec![
                    "support-assistant".to_string(),
                    "approval-worker".to_string(),
                ],
            }],
            generated_prompts: vec![DwGeneratedPromptAsset {
                asset_id: "prompt.support".to_string(),
                path: "agents/support-assistant/prompt.md".to_string(),
                prompt_kind: "system".to_string(),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            requirements: vec![
                DwApplicationPackRequirement {
                    requirement_id: "req.control".to_string(),
                    summary: "Policy control required".to_string(),
                    capability_id: Some(CapabilityId::new("cap://control/policy").unwrap()),
                    provider_id: Some("provider.control.basic".to_string()),
                    applies_to_agents: vec![
                        "support-assistant".to_string(),
                        "approval-worker".to_string(),
                    ],
                },
                DwApplicationPackRequirement {
                    requirement_id: "req.observer".to_string(),
                    summary: "Audit observer required".to_string(),
                    capability_id: Some(CapabilityId::new("cap://observer/audit").unwrap()),
                    provider_id: Some("provider.observer.audit".to_string()),
                    applies_to_agents: vec![
                        "support-assistant".to_string(),
                        "approval-worker".to_string(),
                    ],
                },
            ],
            dependency_pack_refs: vec![
                PackDependencyRef {
                    pack_id: "provider.control.basic".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/control/basic").unwrap(),
                    version: None,
                    provider_id: Some("provider.control.basic".to_string()),
                    applies_to_agents: vec![
                        "support-assistant".to_string(),
                        "approval-worker".to_string(),
                    ],
                },
                PackDependencyRef {
                    pack_id: "provider.observer.audit".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/observer/audit").unwrap(),
                    version: None,
                    provider_id: Some("provider.observer.audit".to_string()),
                    applies_to_agents: vec![
                        "support-assistant".to_string(),
                        "approval-worker".to_string(),
                    ],
                },
            ],
            setup_requirements: Vec::new(),
            layout: DwApplicationPackLayout {
                app_root: "support-suite.pack".to_string(),
                shared_asset_roots: vec!["shared".to_string(), "flows".to_string()],
                layout_hint: Some(ApplicationPackLayoutHints::MultiAgentSharedProviders),
            },
        };

        let text = serde_json::to_value(&spec).unwrap().to_string();
        assert!(text.contains("provider.control.basic"));
        assert!(text.contains("provider.observer.audit"));
        assert!(!text.contains("component_ref"));
        assert!(spec.metadata.multi_agent);
        assert_eq!(spec.agents.len(), 2);
    }

    #[test]
    fn app_pack_spec_schema_is_exportable() {
        let schema = schema_for!(DwApplicationPackSpec);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("dependency_pack_refs"));
        assert!(schema_text.contains("generated_configs"));
        assert!(schema_text.contains("generated_flows"));
        assert!(schema_text.contains("generated_prompts"));
        assert!(schema_text.contains("layout"));
    }

    #[test]
    fn composition_materializer_generates_single_agent_pack_spec() {
        let composition = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.support".to_string(),
                display_name: "Support App".to_string(),
                version: Some("0.5.0".to_string()),
                tenant: None,
                tags: Vec::new(),
            },
            agents: vec![crate::DwAgentComposition {
                agent_id: "support-assistant".to_string(),
                display_name: "Support Assistant".to_string(),
                template_id: "dw.support-assistant".to_string(),
                selected_template: None,
                capability_bindings: vec![crate::CapabilityBinding {
                    capability_id: CapabilityId::new("cap://llm/chat").unwrap(),
                    provider_binding: crate::ProviderBinding {
                        provider_id: "provider.llm.openai".to_string(),
                        source_ref: crate::DwProviderSourceRef::from_raw(
                            "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                        )
                        .unwrap(),
                        version: Some("0.5.0".to_string()),
                        channel: Some("stable".to_string()),
                        provider_family: Some("llm".to_string()),
                        provider_category: Some("chat".to_string()),
                    },
                    optional: false,
                    pack_capability_id: Some("pack.llm.chat".to_string()),
                }],
                behavior_config: crate::BehaviorConfig::default(),
                source_provenance: crate::CompositionSourceProvenance {
                    template_source_ref: Some(
                        TemplateSourceRef::from_raw("repo://templates/support-assistant").unwrap(),
                    ),
                    provider_source_refs: Vec::new(),
                },
            }],
            shared_pack_dependencies: vec![PackDependencyRef {
                pack_id: "provider.llm.openai".to_string(),
                source_ref: PackSourceRef::from_raw(
                    "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                )
                .unwrap(),
                version: Some("0.5.0".to_string()),
                provider_id: Some("provider.llm.openai".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            bundle_plan: Vec::new(),
            unresolved_setup_items: vec![SetupRequirement {
                requirement_id: "setup.llm.key".to_string(),
                status: SetupRequirementStatus::Required,
                summary: "Provide API key".to_string(),
                provider_id: Some("provider.llm.openai".to_string()),
                setup_schema_ref: Some(
                    TemplateSourceRef::from_raw("repo://setup/llm/openai-api-key").unwrap(),
                ),
                question_block_id: Some("provider.llm.credentials".to_string()),
                applies_to_agents: vec!["support-assistant".to_string()],
            }],
            output_plan: DwCompositionOutputPlan {
                generated_pack_id: Some("pack.generated.support".to_string()),
                generated_bundle_id: None,
                supports_multi_agent_app_pack: false,
            },
            source_provenance: None,
        };

        let spec = composition.to_application_pack_spec().unwrap();
        assert_eq!(spec.metadata.pack_id, "pack.generated.support");
        assert_eq!(spec.agents.len(), 1);
        assert_eq!(spec.generated_configs.len(), 1);
        assert_eq!(spec.requirements.len(), 1);
        assert_eq!(spec.dependency_pack_refs.len(), 1);
        assert_eq!(
            spec.layout.layout_hint,
            Some(ApplicationPackLayoutHints::SingleAgentPack)
        );
    }

    #[test]
    fn composition_materializer_generates_multi_agent_pack_spec() {
        let composition = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.suite".to_string(),
                display_name: "Suite".to_string(),
                version: None,
                tenant: None,
                tags: Vec::new(),
            },
            agents: vec![
                crate::DwAgentComposition {
                    agent_id: "agent-a".to_string(),
                    display_name: "Agent A".to_string(),
                    template_id: "dw.a".to_string(),
                    selected_template: None,
                    capability_bindings: Vec::new(),
                    behavior_config: crate::BehaviorConfig::default(),
                    source_provenance: crate::CompositionSourceProvenance::default(),
                },
                crate::DwAgentComposition {
                    agent_id: "agent-b".to_string(),
                    display_name: "Agent B".to_string(),
                    template_id: "dw.b".to_string(),
                    selected_template: None,
                    capability_bindings: Vec::new(),
                    behavior_config: crate::BehaviorConfig::default(),
                    source_provenance: crate::CompositionSourceProvenance::default(),
                },
            ],
            shared_pack_dependencies: vec![
                PackDependencyRef {
                    pack_id: "provider.z".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/z").unwrap(),
                    version: None,
                    provider_id: Some("provider.z".to_string()),
                    applies_to_agents: vec!["agent-a".to_string(), "agent-b".to_string()],
                },
                PackDependencyRef {
                    pack_id: "provider.a".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/a").unwrap(),
                    version: None,
                    provider_id: Some("provider.a".to_string()),
                    applies_to_agents: vec!["agent-a".to_string()],
                },
            ],
            bundle_plan: Vec::new(),
            unresolved_setup_items: Vec::new(),
            output_plan: DwCompositionOutputPlan {
                generated_pack_id: None,
                generated_bundle_id: None,
                supports_multi_agent_app_pack: true,
            },
            source_provenance: None,
        };

        let spec = composition.to_application_pack_spec().unwrap();
        assert!(spec.metadata.multi_agent);
        assert_eq!(spec.generated_configs.len(), 3);
        assert_eq!(spec.assets.len(), 3);
        assert_eq!(spec.dependency_pack_refs[0].pack_id, "provider.a");
        assert_eq!(spec.dependency_pack_refs[1].pack_id, "provider.z");
        assert_eq!(
            spec.layout.layout_hint,
            Some(ApplicationPackLayoutHints::MultiAgentSharedProviders)
        );
        assert_eq!(spec.layout.shared_asset_roots, vec!["shared".to_string()]);
    }

    #[test]
    fn composition_materializer_rejects_empty_composition() {
        let composition = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.empty".to_string(),
                display_name: "Empty".to_string(),
                version: None,
                tenant: None,
                tags: Vec::new(),
            },
            agents: Vec::new(),
            shared_pack_dependencies: Vec::new(),
            bundle_plan: Vec::new(),
            unresolved_setup_items: Vec::new(),
            output_plan: DwCompositionOutputPlan::default(),
            source_provenance: None,
        };

        let err = composition.to_application_pack_spec().unwrap_err();
        assert!(matches!(
            err,
            DwApplicationPackMaterializationError::NoAgents
        ));
    }
}
