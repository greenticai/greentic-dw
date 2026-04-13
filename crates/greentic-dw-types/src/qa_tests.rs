#[cfg(test)]
mod tests {
    use crate::{
        DwWizardQuestionAssembly, DwWizardQuestionBlock, QuestionDepthMode, QuestionPhase,
        QuestionScope, QuestionSource, QuestionVisibility, TemplateSourceRef,
    };
    use schemars::schema_for;

    #[test]
    fn qa_assembly_filters_by_phase_and_depth() {
        let assembly = DwWizardQuestionAssembly {
            blocks: vec![
                DwWizardQuestionBlock {
                    block_id: "core.identity".to_string(),
                    source: QuestionSource::DwCore,
                    owner: "core".to_string(),
                    path: "core.app.name".to_string(),
                    answer_key: Some("app_name".to_string()),
                    prompt: Some("Enter application name: ".to_string()),
                    scope: QuestionScope::Application,
                    phase: QuestionPhase::Design,
                    visibility: QuestionVisibility::Required,
                    source_ref: None,
                    summary: Some("Core identity questions".to_string()),
                },
                DwWizardQuestionBlock {
                    block_id: "template.behavior".to_string(),
                    source: QuestionSource::Template {
                        template_id: "dw.support-assistant".to_string(),
                    },
                    owner: "template.support_assistant".to_string(),
                    path: "template.support_assistant.escalation_policy".to_string(),
                    answer_key: Some("escalation_policy".to_string()),
                    prompt: Some("Enter escalation policy: ".to_string()),
                    scope: QuestionScope::Agent { agent_id: None },
                    phase: QuestionPhase::Design,
                    visibility: QuestionVisibility::ReviewAll,
                    source_ref: Some(
                        TemplateSourceRef::from_raw("repo://templates/support-assistant/qa")
                            .unwrap(),
                    ),
                    summary: Some("Template-specific behavior questions".to_string()),
                },
                DwWizardQuestionBlock {
                    block_id: "composition.memory".to_string(),
                    source: QuestionSource::Composition,
                    owner: "composition.shared".to_string(),
                    path: "composition.shared.memory_strategy".to_string(),
                    answer_key: Some("memory_strategy".to_string()),
                    prompt: Some("Enter memory strategy: ".to_string()),
                    scope: QuestionScope::SharedComposition,
                    phase: QuestionPhase::Design,
                    visibility: QuestionVisibility::Optional,
                    source_ref: None,
                    summary: Some("Shared composition tuning".to_string()),
                },
                DwWizardQuestionBlock {
                    block_id: "provider.credentials".to_string(),
                    source: QuestionSource::Provider {
                        provider_id: "provider.llm.openai".to_string(),
                    },
                    owner: "provider.llm.openai".to_string(),
                    path: "provider.llm.openai.api_key_secret".to_string(),
                    answer_key: Some("openai_api_key_secret".to_string()),
                    prompt: Some("Enter OpenAI API key secret: ".to_string()),
                    scope: QuestionScope::Provider {
                        provider_id: Some("provider.llm.openai".to_string()),
                        agent_id: None,
                    },
                    phase: QuestionPhase::Setup,
                    visibility: QuestionVisibility::HiddenUnlessNeeded,
                    source_ref: Some(
                        TemplateSourceRef::from_raw("repo://providers/llm/openai/qa").unwrap(),
                    ),
                    summary: Some("Provider credentials".to_string()),
                },
            ],
        };

        let recommended =
            assembly.blocks_for(QuestionPhase::Design, QuestionDepthMode::Recommended, false);
        let review_all =
            assembly.blocks_for(QuestionPhase::Design, QuestionDepthMode::ReviewAll, false);
        let setup = assembly.blocks_for(QuestionPhase::Setup, QuestionDepthMode::Recommended, true);

        assert_eq!(recommended.len(), 1);
        assert_eq!(recommended[0].path, "core.app.name");

        assert_eq!(review_all.len(), 3);
        assert_eq!(
            review_all
                .iter()
                .map(|block| block.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "core.app.name",
                "template.support_assistant.escalation_policy",
                "composition.shared.memory_strategy",
            ]
        );

        assert_eq!(setup.len(), 1);
        assert_eq!(setup[0].phase, QuestionPhase::Setup);
        assert!(matches!(setup[0].scope, QuestionScope::Provider { .. }));
    }

    #[test]
    fn qa_assembly_serializes_question_metadata() {
        let assembly = DwWizardQuestionAssembly {
            blocks: vec![DwWizardQuestionBlock {
                block_id: "provider.memory.redis".to_string(),
                source: QuestionSource::Provider {
                    provider_id: "provider.memory.redis".to_string(),
                },
                owner: "provider.memory.redis".to_string(),
                path: "provider.memory.redis.connection_url".to_string(),
                answer_key: Some("redis_connection_url".to_string()),
                prompt: Some("Enter Redis connection URL: ".to_string()),
                scope: QuestionScope::Provider {
                    provider_id: Some("provider.memory.redis".to_string()),
                    agent_id: Some("worker-1".to_string()),
                },
                phase: QuestionPhase::Setup,
                visibility: QuestionVisibility::HiddenUnlessNeeded,
                source_ref: Some(
                    TemplateSourceRef::from_raw("repo://providers/memory/redis/qa").unwrap(),
                ),
                summary: Some("Memory provider configuration".to_string()),
            }],
        };

        let text = serde_json::to_value(&assembly).unwrap().to_string();
        assert!(text.contains("provider.memory.redis.connection_url"));
        assert!(text.contains("\"phase\":\"setup\""));
        assert!(text.contains("\"visibility\":\"hidden_unless_needed\""));
        assert!(text.contains("\"kind\":\"provider\""));
        assert!(text.contains("\"answer_key\":\"redis_connection_url\""));
        assert!(text.contains("repo://providers/memory/redis/qa"));
    }

    #[test]
    fn qa_assembly_schema_is_exportable() {
        let schema = schema_for!(DwWizardQuestionAssembly);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("\"phase\""));
        assert!(schema_text.contains("\"visibility\""));
        assert!(schema_text.contains("\"scope\""));
        assert!(schema_text.contains("\"owner\""));
        assert!(schema_text.contains("\"path\""));
    }
}
