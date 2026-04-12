#[cfg(test)]
mod tests {
    use crate::{
        BundlePackInclusion, BundlePackKind, BundleSourceResolution, DwBundlePlan,
        DwCompositionApplicationMetadata, DwCompositionDocument, DwCompositionOutputPlan,
        GeneratedAppPackRef, PackDependencyRef, PackSourceRef, ProviderPackRef, SupportPackRef,
    };
    use schemars::schema_for;

    #[test]
    fn bundle_plan_supports_multi_agent_application_packs() {
        let generated_app_pack = GeneratedAppPackRef {
            pack_id: "pack.generated.support-suite".to_string(),
            resolution: BundleSourceResolution {
                source_ref: PackSourceRef::from_raw("./dist/support-suite.pack").unwrap(),
                version: Some("0.5.0".to_string()),
                channel: None,
            },
            applies_to_agents: vec![
                "support-assistant".to_string(),
                "approval-worker".to_string(),
            ],
        };

        let provider_pack = ProviderPackRef {
            pack_id: "provider.observer.audit".to_string(),
            provider_id: "provider.observer.audit".to_string(),
            resolution: BundleSourceResolution {
                source_ref: PackSourceRef::from_raw("repo://providers/observer/audit").unwrap(),
                version: Some("0.5.0".to_string()),
                channel: Some("stable".to_string()),
            },
            applies_to_agents: vec![
                "support-assistant".to_string(),
                "approval-worker".to_string(),
            ],
        };

        let support_pack = SupportPackRef {
            pack_id: "pack.shared.audit".to_string(),
            resolution: BundleSourceResolution {
                source_ref: PackSourceRef::from_raw("repo://packs/shared/audit").unwrap(),
                version: None,
                channel: None,
            },
            applies_to_agents: vec![
                "support-assistant".to_string(),
                "approval-worker".to_string(),
            ],
            rationale: Some("Template-required audit support pack".to_string()),
        };

        let plan = DwBundlePlan {
            application_id: "dw.app.support-suite".to_string(),
            multi_agent: true,
            generated_app_pack: generated_app_pack.clone(),
            provider_packs: vec![provider_pack.clone()],
            support_packs: vec![support_pack.clone()],
            inclusions: vec![
                BundlePackInclusion {
                    inclusion_id: "include.generated".to_string(),
                    pack_id: generated_app_pack.pack_id.clone(),
                    kind: BundlePackKind::GeneratedApplicationPack,
                    resolution: generated_app_pack.resolution.clone(),
                    applies_to_agents: generated_app_pack.applies_to_agents.clone(),
                    rationale: Some("Generated application pack".to_string()),
                },
                BundlePackInclusion {
                    inclusion_id: "include.provider.audit".to_string(),
                    pack_id: provider_pack.pack_id.clone(),
                    kind: BundlePackKind::ProviderPack,
                    resolution: provider_pack.resolution.clone(),
                    applies_to_agents: provider_pack.applies_to_agents.clone(),
                    rationale: Some("Shared observer dependency".to_string()),
                },
                BundlePackInclusion {
                    inclusion_id: "include.support.audit".to_string(),
                    pack_id: support_pack.pack_id.clone(),
                    kind: BundlePackKind::SupportPack,
                    resolution: support_pack.resolution.clone(),
                    applies_to_agents: support_pack.applies_to_agents.clone(),
                    rationale: support_pack.rationale.clone(),
                },
            ],
        };

        assert!(plan.multi_agent);
        assert_eq!(plan.provider_packs.len(), 1);
        assert_eq!(plan.support_packs.len(), 1);
        assert_eq!(plan.inclusions.len(), 3);
        assert_eq!(plan.inclusions[1].kind, BundlePackKind::ProviderPack);
    }

    #[test]
    fn bundle_plan_serializes_source_refs_for_all_selected_packs() {
        let plan = DwBundlePlan {
            application_id: "dw.app.single".to_string(),
            multi_agent: false,
            generated_app_pack: GeneratedAppPackRef {
                pack_id: "pack.generated.single".to_string(),
                resolution: BundleSourceResolution {
                    source_ref: PackSourceRef::from_raw("./dist/single.pack").unwrap(),
                    version: None,
                    channel: None,
                },
                applies_to_agents: vec!["agent-1".to_string()],
            },
            provider_packs: vec![ProviderPackRef {
                pack_id: "provider.llm.openai".to_string(),
                provider_id: "provider.llm.openai".to_string(),
                resolution: BundleSourceResolution {
                    source_ref: PackSourceRef::from_raw(
                        "oci://ghcr.io/greenticai/packs/providers/llm/openai:latest",
                    )
                    .unwrap(),
                    version: Some("0.5.0".to_string()),
                    channel: Some("stable".to_string()),
                },
                applies_to_agents: vec!["agent-1".to_string()],
            }],
            support_packs: vec![SupportPackRef {
                pack_id: "pack.shared.audit".to_string(),
                resolution: BundleSourceResolution {
                    source_ref: PackSourceRef::from_raw("repo://packs/shared/audit").unwrap(),
                    version: None,
                    channel: None,
                },
                applies_to_agents: vec!["agent-1".to_string()],
                rationale: Some("Audit".to_string()),
            }],
            inclusions: Vec::new(),
        };

        let text = serde_json::to_value(&plan).unwrap().to_string();
        assert!(text.contains("./dist/single.pack"));
        assert!(text.contains("oci://ghcr.io/greenticai/packs/providers/llm/openai:latest"));
        assert!(text.contains("repo://packs/shared/audit"));
    }

    #[test]
    fn bundle_plan_schema_is_exportable() {
        let schema = schema_for!(DwBundlePlan);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("generated_app_pack"));
        assert!(schema_text.contains("provider_packs"));
        assert!(schema_text.contains("support_packs"));
        assert!(schema_text.contains("inclusions"));
        assert!(schema_text.contains("source_ref"));
    }

    #[test]
    fn composition_bundle_generator_deduplicates_and_orders_provider_packs() {
        let composition = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.support-suite".to_string(),
                display_name: "Support Suite".to_string(),
                version: Some("0.5.0".to_string()),
                tenant: None,
                tags: Vec::new(),
            },
            agents: vec![
                crate::DwAgentComposition {
                    agent_id: "agent-b".to_string(),
                    display_name: "Agent B".to_string(),
                    template_id: "dw.b".to_string(),
                    selected_template: None,
                    capability_bindings: Vec::new(),
                    behavior_config: crate::BehaviorConfig::default(),
                    source_provenance: crate::CompositionSourceProvenance::default(),
                },
                crate::DwAgentComposition {
                    agent_id: "agent-a".to_string(),
                    display_name: "Agent A".to_string(),
                    template_id: "dw.a".to_string(),
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
                    applies_to_agents: vec!["agent-a".to_string()],
                },
                PackDependencyRef {
                    pack_id: "provider.a".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/a").unwrap(),
                    version: None,
                    provider_id: Some("provider.a".to_string()),
                    applies_to_agents: vec!["agent-b".to_string()],
                },
                PackDependencyRef {
                    pack_id: "provider.z".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/z").unwrap(),
                    version: None,
                    provider_id: Some("provider.z".to_string()),
                    applies_to_agents: vec!["agent-b".to_string()],
                },
            ],
            bundle_plan: Vec::new(),
            unresolved_setup_items: Vec::new(),
            output_plan: DwCompositionOutputPlan {
                generated_pack_id: Some("pack.generated.support-suite".to_string()),
                generated_bundle_id: None,
                supports_multi_agent_app_pack: true,
            },
            source_provenance: None,
        };

        let plan = composition.to_bundle_plan().unwrap();
        assert!(plan.multi_agent);
        assert_eq!(
            plan.generated_app_pack.pack_id,
            "pack.generated.support-suite"
        );
        assert_eq!(
            plan.generated_app_pack.applies_to_agents,
            vec!["agent-a", "agent-b"]
        );
        assert_eq!(plan.provider_packs.len(), 2);
        assert_eq!(plan.provider_packs[0].pack_id, "provider.a");
        assert_eq!(plan.provider_packs[1].pack_id, "provider.z");
        assert_eq!(
            plan.provider_packs[1].applies_to_agents,
            vec!["agent-a", "agent-b"]
        );
    }

    #[test]
    fn composition_bundle_generator_preserves_support_pack_rationale() {
        let composition = DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "dw.app.single".to_string(),
                display_name: "Single".to_string(),
                version: None,
                tenant: None,
                tags: Vec::new(),
            },
            agents: vec![crate::DwAgentComposition {
                agent_id: "agent-1".to_string(),
                display_name: "Agent 1".to_string(),
                template_id: "dw.single".to_string(),
                selected_template: None,
                capability_bindings: Vec::new(),
                behavior_config: crate::BehaviorConfig::default(),
                source_provenance: crate::CompositionSourceProvenance::default(),
            }],
            shared_pack_dependencies: vec![PackDependencyRef {
                pack_id: "pack.shared.audit".to_string(),
                source_ref: PackSourceRef::from_raw("repo://packs/shared/audit").unwrap(),
                version: None,
                provider_id: None,
                applies_to_agents: vec!["agent-1".to_string()],
            }],
            bundle_plan: vec![crate::BundleInclusionPlan {
                pack_id: "pack.shared.audit".to_string(),
                source_ref: PackSourceRef::from_raw("repo://packs/shared/audit").unwrap(),
                applies_to_agents: vec!["agent-1".to_string()],
                rationale: Some("Template-required audit pack".to_string()),
            }],
            unresolved_setup_items: Vec::new(),
            output_plan: DwCompositionOutputPlan::default(),
            source_provenance: None,
        };

        let plan = composition.to_bundle_plan().unwrap();
        assert_eq!(plan.support_packs.len(), 1);
        assert_eq!(
            plan.support_packs[0].rationale.as_deref(),
            Some("Template-required audit pack")
        );
        assert_eq!(plan.inclusions[1].kind, BundlePackKind::SupportPack);
    }
}
