#[cfg(test)]
mod tests {
    use crate::{
        DefaultModeFilter, DwWizardQuestionAssembly, DwWizardQuestionBlock, ModeVisibilityPolicy,
        PersonalisedModeFilter, QuestionSource, TemplateSourceRef,
    };
    use schemars::schema_for;

    #[test]
    fn qa_assembly_supports_default_and_personalised_modes() {
        let assembly = DwWizardQuestionAssembly {
            blocks: vec![
                DwWizardQuestionBlock {
                    block_id: "dw.core.identity".to_string(),
                    source: QuestionSource::DwCore,
                    visibility: ModeVisibilityPolicy {
                        visible_in_default_mode: true,
                        visible_in_personalised_mode: true,
                        required_in_default_mode: true,
                        include_when_dependency_required: true,
                    },
                    source_ref: None,
                    summary: Some("Core identity questions".to_string()),
                },
                DwWizardQuestionBlock {
                    block_id: "template.support.behavior".to_string(),
                    source: QuestionSource::Template {
                        template_id: "dw.support-assistant".to_string(),
                    },
                    visibility: ModeVisibilityPolicy {
                        visible_in_default_mode: false,
                        visible_in_personalised_mode: true,
                        required_in_default_mode: false,
                        include_when_dependency_required: false,
                    },
                    source_ref: Some(
                        TemplateSourceRef::from_raw("repo://templates/support-assistant/qa")
                            .unwrap(),
                    ),
                    summary: Some("Template-specific behavior questions".to_string()),
                },
                DwWizardQuestionBlock {
                    block_id: "provider.llm.credentials".to_string(),
                    source: QuestionSource::Provider {
                        provider_id: "provider.llm.openai".to_string(),
                    },
                    visibility: ModeVisibilityPolicy {
                        visible_in_default_mode: true,
                        visible_in_personalised_mode: true,
                        required_in_default_mode: false,
                        include_when_dependency_required: true,
                    },
                    source_ref: Some(
                        TemplateSourceRef::from_raw("repo://providers/llm/openai/qa").unwrap(),
                    ),
                    summary: Some("Provider credentials".to_string()),
                },
                DwWizardQuestionBlock {
                    block_id: "packaging.bundle-options".to_string(),
                    source: QuestionSource::Packaging,
                    visibility: ModeVisibilityPolicy {
                        visible_in_default_mode: false,
                        visible_in_personalised_mode: true,
                        required_in_default_mode: false,
                        include_when_dependency_required: false,
                    },
                    source_ref: None,
                    summary: Some("Advanced packaging options".to_string()),
                },
            ],
            default_mode_filter: DefaultModeFilter {
                include_only_required_without_defaults: true,
                include_dependency_required_questions: true,
                include_provider_defaults: true,
                include_setup_questions: true,
            },
            personalised_mode_filter: PersonalisedModeFilter {
                include_optional_sections: true,
                include_provider_overrides: true,
                include_advanced_sections: true,
                include_packaging_options: true,
            },
        };

        assert_eq!(assembly.blocks.len(), 4);
        assert!(assembly.default_mode_filter.include_setup_questions);
        assert!(assembly.personalised_mode_filter.include_packaging_options);
        assert!(matches!(
            assembly.blocks[1].source,
            QuestionSource::Template { .. }
        ));
    }

    #[test]
    fn qa_assembly_serializes_question_sources_and_visibility() {
        let assembly = DwWizardQuestionAssembly {
            blocks: vec![DwWizardQuestionBlock {
                block_id: "provider.memory.redis".to_string(),
                source: QuestionSource::Provider {
                    provider_id: "provider.memory.redis".to_string(),
                },
                visibility: ModeVisibilityPolicy {
                    visible_in_default_mode: true,
                    visible_in_personalised_mode: true,
                    required_in_default_mode: false,
                    include_when_dependency_required: true,
                },
                source_ref: Some(
                    TemplateSourceRef::from_raw("repo://providers/memory/redis/qa").unwrap(),
                ),
                summary: Some("Memory provider configuration".to_string()),
            }],
            default_mode_filter: DefaultModeFilter::default(),
            personalised_mode_filter: PersonalisedModeFilter::default(),
        };

        let text = serde_json::to_value(&assembly).unwrap().to_string();
        assert!(text.contains("provider.memory.redis"));
        assert!(text.contains("visible_in_default_mode"));
        assert!(text.contains("repo://providers/memory/redis/qa"));
    }

    #[test]
    fn qa_assembly_schema_is_exportable() {
        let schema = schema_for!(DwWizardQuestionAssembly);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("default_mode_filter"));
        assert!(schema_text.contains("personalised_mode_filter"));
        assert!(schema_text.contains("visible_in_default_mode"));
        assert!(schema_text.contains("include_provider_overrides"));
    }
}
