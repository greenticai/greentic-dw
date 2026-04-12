#[cfg(test)]
mod tests {
    use crate::{
        DigitalWorkerTemplate, TemplateAgentLayoutHint, TemplateBehaviorScaffold,
        TemplateCapabilityPlan, TemplateDefaults, TemplateMaturity, TemplateMetadata,
        TemplateModeBehavior, TemplatePackagingHints, TemplateQuestionBlockRef,
    };
    use greentic_cap_types::CapabilityId;
    use schemars::schema_for;
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{self, Cursor, Read};
    use tempfile::NamedTempFile;

    #[test]
    fn template_descriptor_loads_from_json_string() {
        let template = DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "category": "support",
                "tags": ["support", "assistant"],
                "maturity": "beta"
              },
              "capability_plan": {
                "required_capabilities": ["cap://llm/chat"],
                "optional_capabilities": ["cap://memory/short-term"],
                "default_provider_ids": {
                  "cap://llm/chat": "provider.llm.openai",
                  "cap://memory/short-term": "provider.memory.redis"
                }
              },
              "question_blocks": [
                {
                  "block_id": "dw.core.identity",
                  "required": true
                },
                {
                  "block_id": "provider.llm.defaults",
                  "source": {
                    "raw_ref": "repo://qa/provider-llm-defaults",
                    "kind": "repo"
                  }
                }
              ],
              "defaults": {
                "values": {
                  "display_name": "Support Assistant",
                  "worker_default_locale": "en-US"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {
                  "summary": "Ask only unresolved required inputs.",
                  "question_block_ids": ["dw.core.identity"],
                  "allow_provider_overrides": false
                },
                "personalised_mode_behavior": {
                  "summary": "Expose advanced provider and packaging controls.",
                  "question_block_ids": ["dw.core.identity", "provider.llm.defaults"],
                  "include_optional_sections": true,
                  "allow_provider_overrides": true,
                  "allow_packaging_overrides": true
                }
              },
              "packaging_hints": {
                "suggested_agent_layout": "multi_agent_ready",
                "support_pack_refs": [
                  {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/support/audit:latest",
                    "kind": "oci"
                  }
                ],
                "suggested_agent_roles": ["triage", "resolver"],
                "bundle_notes": ["Share the audit support pack across agents."]
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();

        assert_eq!(template.metadata.id, "dw.support-assistant");
        assert_eq!(template.metadata.maturity, TemplateMaturity::Beta);
        assert_eq!(template.capability_plan.required_capabilities.len(), 1);
        assert_eq!(template.capability_plan.optional_capabilities.len(), 1);
        assert_eq!(
            template
                .capability_plan
                .default_provider_ids
                .get(&CapabilityId::new("cap://llm/chat").unwrap())
                .map(String::as_str),
            Some("provider.llm.openai")
        );
        assert!(template.supports_multi_agent_app_pack);
        assert_eq!(
            template.packaging_hints.suggested_agent_layout,
            Some(TemplateAgentLayoutHint::MultiAgentReady)
        );
    }

    #[test]
    fn template_descriptor_loads_from_file() {
        let file = NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            r#"{
              "metadata": {
                "id": "dw.workflow-executor",
                "name": "Workflow Executor",
                "summary": "Executes workflow tasks.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {
                  "include_optional_sections": true
                }
              }
            }"#,
        )
        .unwrap();

        let template = DigitalWorkerTemplate::from_json_path(file.path()).unwrap();
        assert_eq!(template.metadata.name, "Workflow Executor");
        assert_eq!(template.metadata.maturity, TemplateMaturity::Stable);
    }

    #[test]
    fn template_descriptor_supports_descriptor_object_construction() {
        let template = DigitalWorkerTemplate {
            metadata: TemplateMetadata {
                id: "dw.approval-worker".to_string(),
                name: "Approval Worker".to_string(),
                summary: "Handles approval steps.".to_string(),
                category: Some("workflow".to_string()),
                tags: vec!["approval".to_string()],
                maturity: TemplateMaturity::Experimental,
            },
            capability_plan: TemplateCapabilityPlan {
                required_capabilities: vec![CapabilityId::new("cap://control/policy").unwrap()],
                optional_capabilities: vec![CapabilityId::new("cap://observer/audit").unwrap()],
                default_provider_ids: BTreeMap::from([(
                    CapabilityId::new("cap://control/policy").unwrap(),
                    "provider.control.basic".to_string(),
                )]),
            },
            question_blocks: vec![TemplateQuestionBlockRef {
                block_id: "dw.workflow.steps".to_string(),
                source: None,
                required: true,
                summary: Some("Approval workflow steps".to_string()),
            }],
            defaults: TemplateDefaults {
                values: BTreeMap::from([("tenant".to_string(), serde_json::json!("tenant-a"))]),
            },
            behavior_scaffold: TemplateBehaviorScaffold {
                default_mode_behavior: TemplateModeBehavior::default(),
                personalised_mode_behavior: TemplateModeBehavior {
                    include_optional_sections: true,
                    allow_provider_overrides: true,
                    ..TemplateModeBehavior::default()
                },
            },
            packaging_hints: TemplatePackagingHints {
                suggested_agent_layout: Some(TemplateAgentLayoutHint::SingleAgent),
                ..TemplatePackagingHints::default()
            },
            supports_multi_agent_app_pack: false,
        };

        assert_eq!(template.question_blocks.len(), 1);
        assert_eq!(
            template.capability_plan.required_capabilities[0],
            CapabilityId::new("cap://control/policy").unwrap()
        );
    }

    #[test]
    fn template_descriptor_loads_from_reader() {
        let template = DigitalWorkerTemplate::from_json_reader(Cursor::new(
            r#"{
              "metadata": {
                "id": "dw.reader-template",
                "name": "Reader Template",
                "summary": "Loaded from reader.",
                "maturity": "deprecated"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "packaging_hints": {
                "suggested_agent_layout": "multi_agent_recommended"
              }
            }"#,
        ))
        .unwrap();

        assert_eq!(template.metadata.maturity, TemplateMaturity::Deprecated);
        assert_eq!(
            template.packaging_hints.suggested_agent_layout,
            Some(TemplateAgentLayoutHint::MultiAgentRecommended)
        );
    }

    #[test]
    fn template_descriptor_reader_returns_read_error() {
        struct BrokenReader;

        impl Read for BrokenReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("boom"))
            }
        }

        let err = DigitalWorkerTemplate::from_json_reader(BrokenReader).unwrap_err();
        match err {
            crate::TemplateDescriptorError::Read { path, .. } => assert_eq!(path, "<reader>"),
            other => panic!("expected read error, got {other:?}"),
        }
    }

    #[test]
    fn template_descriptor_returns_parse_error_for_inline_json() {
        let err = DigitalWorkerTemplate::from_json_str("{invalid").unwrap_err();
        match err {
            crate::TemplateDescriptorError::Parse { origin, .. } => {
                assert_eq!(origin, "inline template json")
            }
            other => panic!("expected parse error, got {other:?}"),
        }
    }

    #[test]
    fn template_descriptor_returns_read_error_for_missing_file() {
        let err = DigitalWorkerTemplate::from_json_path("/tmp/greentic-dw-missing-template.json")
            .unwrap_err();
        match err {
            crate::TemplateDescriptorError::Read { path, .. } => {
                assert!(path.ends_with("greentic-dw-missing-template.json"))
            }
            other => panic!("expected read error, got {other:?}"),
        }
    }

    #[test]
    fn template_schema_is_exportable() {
        let schema = schema_for!(DigitalWorkerTemplate);
        let schema_text = serde_json::to_value(schema).unwrap().to_string();
        assert!(schema_text.contains("supports_multi_agent_app_pack"));
        assert!(schema_text.contains("default_mode_behavior"));
        assert!(schema_text.contains("default_provider_ids"));
        assert!(schema_text.contains("multi_agent_recommended"));
        assert!(schema_text.contains("deprecated"));
    }
}
