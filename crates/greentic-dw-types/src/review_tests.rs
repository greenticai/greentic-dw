#[cfg(test)]
mod tests {
    use crate::{
        BehaviorConfig, BundleInclusionPlan, CompositionSourceProvenance, DwAgentComposition,
        DwCompositionApplicationMetadata, DwCompositionDocument, DwCompositionOutputPlan,
        DwReviewEnvelope, PackDependencyRef, PackSourceRef, ProviderBinding, SetupRequirement,
        SetupRequirementStatus, TemplateSourceRef,
    };
    use greentic_cap_types::CapabilityId;
    use schemars::schema_for;

    fn sample_composition() -> DwCompositionDocument {
        DwCompositionDocument {
            application: DwCompositionApplicationMetadata {
                application_id: "support-suite".to_string(),
                display_name: "Support Suite".to_string(),
                version: Some("1.0.0".to_string()),
                tenant: Some("tenant-a".to_string()),
                tags: vec!["support".to_string()],
            },
            agents: vec![DwAgentComposition {
                agent_id: "worker-1".to_string(),
                display_name: "Support Worker".to_string(),
                template_id: "dw.support-assistant".to_string(),
                selected_template: None,
                capability_bindings: vec![crate::CapabilityBinding {
                    capability_id: CapabilityId::new("cap://llm/chat").unwrap(),
                    provider_binding: ProviderBinding {
                        provider_id: "provider.llm.openai".to_string(),
                        source_ref: crate::DwProviderSourceRef::from_raw(
                            "repo://providers/llm/openai",
                        )
                        .unwrap(),
                        version: Some("1.2.3".to_string()),
                        channel: None,
                        provider_family: Some("llm".to_string()),
                        provider_category: Some("inference".to_string()),
                    },
                    optional: false,
                    pack_capability_id: Some("cap.llm".to_string()),
                }],
                behavior_config: BehaviorConfig {
                    enabled_question_block_ids: vec!["core.identity".to_string()],
                    values: Default::default(),
                },
                source_provenance: CompositionSourceProvenance {
                    template_source_ref: Some(
                        TemplateSourceRef::from_raw("repo://templates/support-assistant").unwrap(),
                    ),
                    provider_source_refs: vec![crate::DwProviderSourceRef {
                        source: PackSourceRef::from_raw("repo://providers/llm/openai").unwrap(),
                    }],
                },
            }],
            shared_pack_dependencies: vec![
                PackDependencyRef {
                    pack_id: "provider.llm.openai".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://providers/llm/openai").unwrap(),
                    version: Some("1.2.3".to_string()),
                    provider_id: Some("provider.llm.openai".to_string()),
                    applies_to_agents: vec!["worker-1".to_string()],
                },
                PackDependencyRef {
                    pack_id: "support.shared-observer".to_string(),
                    source_ref: PackSourceRef::from_raw("repo://packs/shared-observer").unwrap(),
                    version: None,
                    provider_id: None,
                    applies_to_agents: vec!["worker-1".to_string()],
                },
            ],
            bundle_plan: vec![BundleInclusionPlan {
                pack_id: "support.shared-observer".to_string(),
                source_ref: PackSourceRef::from_raw("repo://packs/shared-observer").unwrap(),
                applies_to_agents: vec!["worker-1".to_string()],
                rationale: Some("Template packaging support pack".to_string()),
            }],
            unresolved_setup_items: vec![SetupRequirement {
                requirement_id: "setup.worker-1.provider.llm.openai".to_string(),
                status: SetupRequirementStatus::Required,
                summary: "Setup required for provider `provider.llm.openai`".to_string(),
                provider_id: Some("provider.llm.openai".to_string()),
                setup_schema_ref: Some(
                    TemplateSourceRef::from_raw("repo://providers/llm/openai/setup").unwrap(),
                ),
                question_block_id: Some("provider.llm.openai.credentials".to_string()),
                applies_to_agents: vec!["worker-1".to_string()],
            }],
            output_plan: DwCompositionOutputPlan {
                generated_pack_id: Some("pack.generated.support-suite".to_string()),
                generated_bundle_id: Some("bundle.generated.support-suite".to_string()),
                supports_multi_agent_app_pack: false,
            },
            source_provenance: Some(CompositionSourceProvenance {
                template_source_ref: Some(
                    TemplateSourceRef::from_raw("repo://templates/support-assistant").unwrap(),
                ),
                provider_source_refs: vec![
                    crate::DwProviderSourceRef::from_raw("repo://providers/llm/openai").unwrap(),
                ],
            }),
        }
    }

    #[test]
    fn review_envelope_materializes_pack_bundle_and_warnings() {
        let composition = sample_composition();

        let review = composition.to_review_envelope().unwrap();

        assert_eq!(
            review.composition.application.application_id,
            "support-suite"
        );
        assert_eq!(
            review.application_pack_spec.metadata.pack_id,
            "pack.generated.support-suite"
        );
        assert_eq!(
            review.bundle_plan.generated_app_pack.pack_id,
            "pack.generated.support-suite"
        );
        assert_eq!(review.setup_requirements.len(), 1);
        assert_eq!(review.warnings.len(), 1);
        assert_eq!(
            review.warnings[0].requirement_id.as_deref(),
            Some("setup.worker-1.provider.llm.openai")
        );
        assert_eq!(
            review.provenance.generated_bundle_id.as_deref(),
            Some("bundle.generated.support-suite")
        );
    }

    #[test]
    fn review_envelope_schema_is_exportable() {
        let schema = schema_for!(DwReviewEnvelope);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("\"application_pack_spec\""));
        assert!(schema_text.contains("\"bundle_plan\""));
        assert!(schema_text.contains("\"setup_requirements\""));
        assert!(schema_text.contains("\"warnings\""));
        assert!(schema_text.contains("\"provenance\""));
    }
}
