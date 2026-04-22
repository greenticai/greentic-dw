#[cfg(test)]
mod tests {
    use crate::cli_types::{AnswerDocument, CliError, WizardArgs};
    use crate::i18n::{MsgKey, localized};
    use crate::wizard::{
        apply_overrides, apply_template_defaults, build_dry_run_output, build_manifest,
        build_question_assembly, build_review_envelope, is_insecure_http_url,
        is_remote_answers_url, load_answers, prompt_design_flow_with, prompt_template_selection,
        run,
    };
    use greentic_dw_manifest::MANIFEST_SCHEMA_VERSION;
    use schemars::schema_for;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn workspace_examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .canonicalize()
            .expect("workspace examples dir")
    }

    #[test]
    fn applies_cli_overrides_to_answers() {
        let mut answers = AnswerDocument {
            manifest_id: "a".to_string(),
            display_name: "A".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: None,
            review_mode: None,
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let args = WizardArgs {
            answers: None,
            schema: false,
            emit_answers: false,
            dry_run: false,
            locale: "en".to_string(),
            non_interactive: true,
            template_catalog: None,
            provider_catalog: None,
            list_templates: false,
            template: None,
            review_all: false,
            provider_overrides: Vec::new(),
            manifest_id: Some("dw.sample".to_string()),
            display_name: Some("Sample".to_string()),
            tenant: Some("tenant-b".to_string()),
            team: Some("team-x".to_string()),
            requested_locale: Some("fr-FR".to_string()),
            human_locale: Some("nl-NL".to_string()),
        };

        apply_overrides(&mut answers, &args).unwrap();
        assert_eq!(answers.manifest_id, "dw.sample");
        assert_eq!(answers.tenant, "tenant-b");
        assert_eq!(answers.team.as_deref(), Some("team-x"));
    }

    #[test]
    fn schema_contains_required_tenant_field() {
        let schema = schema_for!(AnswerDocument);
        let value = serde_json::to_value(schema).expect("schema serialization");
        let required = value
            .pointer("/required")
            .and_then(serde_json::Value::as_array)
            .expect("required array");

        let required_values: Vec<&str> = required
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert!(required_values.contains(&"tenant"));
        assert!(
            value.pointer("/properties/agent_answers").is_some(),
            "schema should expose structured per-agent replay data"
        );
    }

    #[test]
    fn localized_defaults_to_english() {
        assert_eq!(localized("xx", MsgKey::Tenant), "Enter tenant: ");
    }

    #[test]
    fn localized_supports_dutch_prompts() {
        assert_eq!(localized("nl", MsgKey::Tenant), "Tenant invoeren: ");
        assert_eq!(
            localized("nl-NL", MsgKey::DisplayName),
            "Weergavenaam invoeren: "
        );
        assert_eq!(
            localized("nl", MsgKey::ManifestId),
            "Manifest-ID invoeren: "
        );
        assert_eq!(localized("nl", MsgKey::Team), "Team invoeren (optioneel): ");
        assert_eq!(
            localized("nl", MsgKey::RequestedLocale),
            "Gevraagde locale invoeren (optioneel): "
        );
        assert_eq!(
            localized("nl", MsgKey::HumanLocale),
            "Human locale invoeren (optioneel): "
        );
    }

    #[test]
    fn localized_covers_english_fallback_prompts() {
        assert_eq!(localized("en", MsgKey::ManifestId), "Enter manifest id: ");
        assert_eq!(localized("en", MsgKey::DisplayName), "Enter display name: ");
        assert_eq!(localized("en", MsgKey::Team), "Enter team (optional): ");
        assert_eq!(
            localized("en", MsgKey::RequestedLocale),
            "Enter requested locale (optional): "
        );
        assert_eq!(
            localized("en", MsgKey::HumanLocale),
            "Enter human locale (optional): "
        );
    }

    #[test]
    fn build_manifest_sets_expected_contracts() {
        let answers = AnswerDocument {
            manifest_id: "dw.sample".to_string(),
            display_name: "Sample".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: None,
            review_mode: None,
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: Some("team-1".to_string()),
            requested_locale: Some("fr-FR".to_string()),
            human_locale: Some("nl-NL".to_string()),
            worker_default_locale: "en-US".to_string(),
        };

        let manifest = build_manifest(&answers);
        assert_eq!(manifest.id, "dw.sample");
        assert_eq!(manifest.version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(manifest.worker_version.as_deref(), Some("0.5"));
        assert_eq!(manifest.tenancy.tenant, "tenant-a");
    }

    #[test]
    fn load_answers_reads_valid_document() {
        let file = NamedTempFile::new().expect("temp file");
        let doc = serde_json::json!({
            "manifest_id": "dw.sample",
            "display_name": "Sample",
            "manifest_version": "0.5",
            "tenant": "tenant-a",
            "team": "team-1",
            "requested_locale": "fr-FR",
            "human_locale": "nl-NL",
            "worker_default_locale": "en-US"
        });

        fs::write(file.path(), serde_json::to_vec(&doc).expect("json")).expect("write answers");
        let loaded = load_answers(file.path()).expect("load answers");
        assert_eq!(loaded.manifest_id, "dw.sample");
        assert_eq!(loaded.team.as_deref(), Some("team-1"));
    }

    #[test]
    fn load_answers_reads_structured_agent_answers_document() {
        let examples_dir = workspace_examples_dir();
        let path = examples_dir.join("answers/support-squad-create-answers.json");

        let loaded = load_answers(&path).expect("load structured answers");

        assert_eq!(loaded.manifest_id, "dw.support.squad");
        assert_eq!(
            loaded.design_answers.get("agent_count"),
            Some(&serde_json::Value::String("2".to_string()))
        );
        assert_eq!(
            loaded
                .agent_answers
                .get("agent-1")
                .and_then(|agent| agent.provider_overrides.get("cap://llm/chat")),
            Some(&"provider.llm.openai.chat".to_string())
        );
        assert_eq!(
            loaded
                .agent_answers
                .get("agent-2")
                .and_then(|agent| agent.design_answers.get("support_behavior")),
            Some(&serde_json::Value::String(
                "Handle standard triage and FAQ responses.".to_string()
            ))
        );
    }

    #[test]
    fn load_answers_returns_parse_error_for_invalid_json() {
        let file = NamedTempFile::new().expect("temp file");
        fs::write(file.path(), "{invalid").expect("write invalid JSON");

        let err = load_answers(file.path()).expect_err("expected parse error");
        assert!(matches!(err, CliError::AnswersParse { .. }));
    }

    #[test]
    fn load_answers_returns_read_error_for_missing_file() {
        let missing = PathBuf::from("/tmp/greentic-dw-cli-missing-answers.json");
        let err = load_answers(&missing).expect_err("expected read error");
        assert!(matches!(err, CliError::AnswersRead { .. }));
    }

    #[test]
    fn run_wizard_dry_run_succeeds_non_interactive() {
        let args = [
            "greentic-dw",
            "wizard",
            "--dry-run",
            "--non-interactive",
            "--manifest-id",
            "dw.sample",
            "--display-name",
            "Sample",
            "--tenant",
            "tenant-a",
            "--team",
            "team-1",
            "--requested-locale",
            "fr-FR",
            "--human-locale",
            "nl-NL",
            "--emit-answers",
        ];

        run(args).expect("dry-run should succeed");
    }

    #[test]
    fn run_wizard_execute_succeeds_non_interactive() {
        let args = [
            "greentic-dw",
            "wizard",
            "--non-interactive",
            "--manifest-id",
            "dw.sample",
            "--display-name",
            "Sample",
            "--tenant",
            "tenant-a",
        ];

        run(args).expect("execute mode should succeed");
    }

    #[test]
    fn run_wizard_rejects_empty_required_fields() {
        let args = ["greentic-dw", "wizard", "--non-interactive"];
        let err = run(args).expect_err("manifest validation should fail");
        assert!(matches!(err, CliError::Manifest(_)));
    }

    #[test]
    fn run_wizard_schema_mode_succeeds() {
        let args = ["greentic-dw", "wizard", "--schema"];
        run(args).expect("schema mode should succeed");
    }

    #[test]
    fn apply_template_defaults_populates_missing_answers() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "beta"
              },
              "capability_plan": {},
              "defaults": {
                "values": {
                  "tenant": "tenant-template",
                  "requested_locale": "fr-FR"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();

        let mut answers = AnswerDocument {
            manifest_id: String::new(),
            display_name: String::new(),
            manifest_version: "0.5".to_string(),
            tenant: String::new(),
            template_id: None,
            review_mode: None,
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        apply_template_defaults(&mut answers, &template);
        assert_eq!(answers.manifest_id, "dw.support-assistant");
        assert_eq!(answers.display_name, "Support Assistant");
        assert_eq!(answers.tenant, "tenant-template");
        assert_eq!(answers.requested_locale.as_deref(), Some("fr-FR"));
    }

    #[test]
    fn run_wizard_lists_templates_from_catalog() {
        let template_file = NamedTempFile::new().unwrap();
        fs::write(
            template_file.path(),
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
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

        let catalog_file = NamedTempFile::new().unwrap();
        fs::write(
            catalog_file.path(),
            format!(
                r#"{{
                  "entries": [
                    {{
                      "template_id": "dw.support-assistant",
                      "display_name": "Support Assistant",
                      "summary": "Handles support intake.",
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
            ),
        )
        .unwrap();

        let args = [
            "greentic-dw",
            "wizard",
            "--list-templates",
            "--template-catalog",
            catalog_file.path().to_str().unwrap(),
        ];

        run(args).expect("template catalog listing should succeed");
    }

    #[test]
    fn run_wizard_uses_template_catalog_defaults() {
        let template_file = NamedTempFile::new().unwrap();
        fs::write(
            template_file.path(),
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "defaults": {
                "values": {
                  "tenant": "tenant-template"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();

        let catalog_file = NamedTempFile::new().unwrap();
        fs::write(
            catalog_file.path(),
            format!(
                r#"{{
                  "entries": [
                    {{
                      "template_id": "dw.support-assistant",
                      "display_name": "Support Assistant",
                      "summary": "Handles support intake.",
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
            ),
        )
        .unwrap();

        let args = [
            "greentic-dw",
            "wizard",
            "--non-interactive",
            "--dry-run",
            "--template-catalog",
            catalog_file.path().to_str().unwrap(),
            "--template",
            "dw.support-assistant",
        ];

        run(args).expect("template-backed dry-run should succeed");
    }

    #[test]
    fn run_wizard_uses_starter_examples_template_catalog() {
        let examples_dir = workspace_examples_dir();
        let catalog_path = examples_dir.join("templates/catalog.json");

        let args = [
            "greentic-dw",
            "wizard",
            "--non-interactive",
            "--dry-run",
            "--template-catalog",
            catalog_path.to_str().unwrap(),
            "--template",
            "dw.support-assistant",
            "--tenant",
            "tenant-starter",
        ];

        run(args).expect("starter examples template catalog should succeed");
    }

    #[test]
    fn prompt_template_selection_defaults_to_first_entry() {
        let catalog = greentic_dw_types::TemplateCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "template_id": "dw.support-assistant",
                  "display_name": "Support Assistant",
                  "summary": "Handles support intake.",
                  "source_ref": {
                    "raw_ref": "./templates/support-assistant.json",
                    "kind": "local_path"
                  },
                  "maturity": "stable",
                  "mode_suitability": "both_modes"
                },
                {
                  "template_id": "dw.approval-worker",
                  "display_name": "Approval Worker",
                  "summary": "Handles approvals.",
                  "source_ref": {
                    "raw_ref": "./templates/approval-worker.json",
                    "kind": "local_path"
                  },
                  "maturity": "stable",
                  "mode_suitability": "both_modes"
                }
              ]
            }"#,
        )
        .unwrap();

        let selected = prompt_template_selection(&catalog, |_| Ok(String::new())).unwrap();

        assert_eq!(selected.as_deref(), Some("dw.support-assistant"));
    }

    #[test]
    fn detects_https_answers_source() {
        assert!(is_remote_answers_url(
            "https://github.com/greenticai/greentic-dw/releases/latest/download/orchestrator-create-answers.json"
        ));
        assert!(!is_remote_answers_url(
            "http://github.com/greenticai/greentic-dw/releases/latest/download/orchestrator-create-answers.json"
        ));
        assert!(!is_remote_answers_url(
            "examples/answers/orchestrator-create-answers.json"
        ));
    }

    #[test]
    fn rejects_insecure_http_answers_source() {
        assert!(is_insecure_http_url(
            "http://example.com/orchestrator-create-answers.json"
        ));

        let err = load_answers(PathBuf::from("http://example.com/answers.json").as_path())
            .expect_err("insecure http should be rejected");
        assert!(matches!(err, CliError::InsecureAnswersUrl { .. }));
    }

    #[test]
    fn build_review_envelope_generates_review_sections_for_template_dry_run() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {
                "required_capabilities": ["cap://llm/chat"]
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::Recommended),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: Some("en-GB".to_string()),
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let review = build_review_envelope(&answers, &template, None, None).unwrap();
        assert_eq!(
            review.composition.application.application_id,
            "dw.support-assistant"
        );
        assert_eq!(
            review.application_pack_spec.metadata.pack_id,
            "pack.generated.dw.support-assistant"
        );
        assert!(review.setup_requirements.iter().any(|item| {
            item.summary.contains("No provider selected")
                || item.summary.contains("Setup required for provider")
        }));
    }

    #[test]
    fn build_dry_run_output_includes_review_envelope_for_template_flows() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
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

        let answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::Recommended),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: Some("en-GB".to_string()),
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };
        let manifest = build_manifest(&answers);
        let request_scope = greentic_dw_manifest::RequestScope {
            tenant: answers.tenant.clone(),
            team: answers.team.clone(),
        };
        let envelope = manifest
            .to_task_envelope(
                format!("{}-task", answers.manifest_id),
                answers.manifest_id.clone(),
                &request_scope,
                answers.requested_locale.clone(),
                answers.human_locale.clone(),
            )
            .unwrap();

        let output = build_dry_run_output(
            &manifest,
            &envelope,
            &answers,
            Some(&template),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(output.answers.is_some());
        assert!(output.data.get("question_assembly").is_some());
        assert!(output.data.get("review_envelope").is_some());
        assert!(
            output.data["review_envelope"]
                .get("application_pack_spec")
                .is_some()
        );
        assert!(output.data["review_envelope"].get("bundle_plan").is_some());
    }

    #[test]
    fn build_dry_run_output_surfaces_required_setup_blocks() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
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
                  "cap://llm/chat": "provider.llm.openai.chat"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.openai.chat",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat",
                  "summary": "Managed LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  },
                  "required_setup_schema_refs": [
                    {
                      "raw_ref": "repo://setup/llm/openai",
                      "kind": "repo"
                    }
                  ],
                  "required_question_block_ids": ["provider.llm.chat.openai.setup"],
                  "question_blocks": [
                    {
                      "block_id": "provider.llm.chat.openai.setup",
                      "answer_key": "openai_api_key_secret",
                      "prompt": "Enter OpenAI API key secret name: ",
                      "setup_schema_ref": {
                        "raw_ref": "repo://setup/llm/openai",
                        "kind": "repo"
                      }
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::Recommended),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };
        let manifest = build_manifest(&answers);
        let request_scope = greentic_dw_manifest::RequestScope {
            tenant: answers.tenant.clone(),
            team: answers.team.clone(),
        };
        let envelope = manifest
            .to_task_envelope(
                format!("{}-task", answers.manifest_id),
                answers.manifest_id.clone(),
                &request_scope,
                answers.requested_locale.clone(),
                answers.human_locale.clone(),
            )
            .unwrap();

        let output = build_dry_run_output(
            &manifest,
            &envelope,
            &answers,
            Some(&template),
            None,
            Some(&provider_catalog),
            false,
        )
        .unwrap();

        let visible_setup_blocks = output
            .data
            .get("visible_setup_blocks")
            .and_then(serde_json::Value::as_array)
            .expect("visible_setup_blocks array");

        assert_eq!(visible_setup_blocks.len(), 1);
        assert_eq!(
            visible_setup_blocks[0]["block_id"],
            serde_json::Value::String("provider.llm.chat.openai.setup".to_string())
        );
    }

    #[test]
    fn build_dry_run_output_resolves_agent_scoped_provider_blocks() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
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
                  "cap://llm/chat": "provider.llm.openai.chat"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.openai.chat",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat",
                  "summary": "Managed LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-app".to_string(),
            display_name: "Support App".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::from([
                (
                    "agent_count".to_string(),
                    serde_json::Value::String("2".to_string()),
                ),
                (
                    "provider_strategy".to_string(),
                    serde_json::Value::String("per_agent".to_string()),
                ),
            ]),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };
        let manifest = build_manifest(&answers);
        let request_scope = greentic_dw_manifest::RequestScope {
            tenant: answers.tenant.clone(),
            team: answers.team.clone(),
        };
        let envelope = manifest
            .to_task_envelope(
                format!("{}-task", answers.manifest_id),
                answers.manifest_id.clone(),
                &request_scope,
                answers.requested_locale.clone(),
                answers.human_locale.clone(),
            )
            .unwrap();

        let output = build_dry_run_output(
            &manifest,
            &envelope,
            &answers,
            Some(&template),
            None,
            Some(&provider_catalog),
            false,
        )
        .unwrap();

        let resolved_visible_design_blocks = output
            .data
            .get("resolved_visible_design_blocks")
            .and_then(serde_json::Value::as_array)
            .expect("resolved_visible_design_blocks array");

        let provider_selection_blocks = resolved_visible_design_blocks
            .iter()
            .filter(|block| block["scope"]["kind"] == "provider")
            .filter(|block| {
                block["answer_key"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("agent.")
                    && block["answer_key"]
                        .as_str()
                        .unwrap_or("")
                        .contains(".provider.cap://llm/chat")
            })
            .collect::<Vec<_>>();

        assert_eq!(provider_selection_blocks.len(), 2);
        assert_eq!(
            provider_selection_blocks[0]["answer_key"],
            serde_json::Value::String("agent.1.provider.cap://llm/chat".to_string())
        );
        assert_eq!(
            provider_selection_blocks[0]["scope"]["agent_id"],
            serde_json::Value::String("agent-1".to_string())
        );
        assert_eq!(
            provider_selection_blocks[1]["answer_key"],
            serde_json::Value::String("agent.2.provider.cap://llm/chat".to_string())
        );
        assert_eq!(
            provider_selection_blocks[1]["scope"]["agent_id"],
            serde_json::Value::String("agent-2".to_string())
        );
    }

    #[test]
    fn build_question_assembly_includes_core_template_and_provider_blocks() {
        let examples_dir = workspace_examples_dir();
        let template_catalog = greentic_dw_types::TemplateCatalog::from_json_path(
            examples_dir.join("templates/catalog.json"),
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_path(
            examples_dir.join("providers/catalog.json"),
        )
        .unwrap();
        let template = template_catalog
            .resolve_template("dw.support-assistant")
            .unwrap();
        let selected_entry = template_catalog.find("dw.support-assistant");

        let assembly =
            build_question_assembly(&template, selected_entry, Some(&provider_catalog)).unwrap();

        assert!(
            assembly
                .blocks
                .iter()
                .any(|block| block.path == "core.app.identity")
        );
        assert!(assembly.blocks.iter().any(|block| {
            block.path == "template.dw.support-assistant.dw.core.identity"
                || block.path.contains("template.dw.support-assistant")
        }));
        assert!(
            assembly.blocks.iter().any(|block| {
                block.path == "provider.provider.llm.openai.chat.selection.llm.chat"
            })
        );
        assert!(assembly.blocks.iter().any(|block| {
            block.block_id == "provider.llm.chat.openai"
                && block.phase == greentic_dw_types::QuestionPhase::Design
                && matches!(
                    block.source,
                    greentic_dw_types::QuestionSource::Provider { .. }
                )
        }));
    }

    #[test]
    fn build_review_envelope_uses_provider_catalog_defaults_for_starter_template() {
        let examples_dir = workspace_examples_dir();
        let template_catalog = greentic_dw_types::TemplateCatalog::from_json_path(
            examples_dir.join("templates/catalog.json"),
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_path(
            examples_dir.join("providers/catalog.json"),
        )
        .unwrap();
        let template = template_catalog
            .resolve_template("dw.support-assistant")
            .unwrap();
        let selected_entry = template_catalog.find("dw.support-assistant");

        let answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::Recommended),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: Some("en-GB".to_string()),
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let review =
            build_review_envelope(&answers, &template, selected_entry, Some(&provider_catalog))
                .unwrap();

        assert!(!review.application_pack_spec.dependency_pack_refs.is_empty());
        assert!(!review.bundle_plan.provider_packs.is_empty());
        assert!(
            review
                .setup_requirements
                .iter()
                .all(|item| !item.summary.contains("No provider selected"))
        );
    }

    #[test]
    fn build_review_envelope_expands_multi_agent_count_into_stable_agents() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-app".to_string(),
            display_name: "Support App".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::from([(
                "agent_count".to_string(),
                serde_json::Value::String("2".to_string()),
            )]),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let review = build_review_envelope(&answers, &template, None, None).unwrap();

        assert_eq!(review.composition.agents.len(), 2);
        assert_eq!(
            review.composition.agents[0].agent_id,
            "dw.support-app.agent-1"
        );
        assert_eq!(
            review.composition.agents[1].agent_id,
            "dw.support-app.agent-2"
        );
        assert_eq!(review.application_pack_spec.agents.len(), 2);
        assert!(review.bundle_plan.multi_agent);
    }

    #[test]
    fn build_review_envelope_applies_per_agent_scoped_design_answers() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-app".to_string(),
            display_name: "Support App".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::from([
                (
                    "support_behavior".to_string(),
                    serde_json::Value::String("shared guidance".to_string()),
                ),
                (
                    "agent.1.support_behavior".to_string(),
                    serde_json::Value::String("agent one guidance".to_string()),
                ),
                (
                    "agent.dw.support-app.agent-2.support_behavior".to_string(),
                    serde_json::Value::String("agent two guidance".to_string()),
                ),
                (
                    "agent_count".to_string(),
                    serde_json::Value::String("2".to_string()),
                ),
            ]),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let review = build_review_envelope(&answers, &template, None, None).unwrap();

        assert_eq!(
            review.composition.agents[0]
                .behavior_config
                .values
                .get("support_behavior"),
            Some(&serde_json::Value::String("agent one guidance".to_string()))
        );
        assert_eq!(
            review.composition.agents[1]
                .behavior_config
                .values
                .get("support_behavior"),
            Some(&serde_json::Value::String("agent two guidance".to_string()))
        );
        assert_eq!(
            review.composition.agents[0]
                .behavior_config
                .values
                .get("agent.1.support_behavior"),
            None
        );
    }

    #[test]
    fn build_review_envelope_applies_per_agent_provider_overrides() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
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
                  "cap://llm/chat": "provider.llm.openai.chat"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.openai.chat",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat",
                  "summary": "Managed LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  }
                },
                {
                  "provider_id": "provider.llm.openai.chat-lite",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat Lite",
                  "summary": "Lower cost LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat-lite:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-app".to_string(),
            display_name: "Support App".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::from([
                (
                    "agent_count".to_string(),
                    serde_json::Value::String("2".to_string()),
                ),
                (
                    "provider_strategy".to_string(),
                    serde_json::Value::String("n".to_string()),
                ),
                (
                    "agent.1.provider.cap://llm/chat".to_string(),
                    serde_json::Value::String("provider.llm.openai.chat".to_string()),
                ),
                (
                    "agent.dw.support-app.agent-2.provider.cap://llm/chat".to_string(),
                    serde_json::Value::String("provider.llm.openai.chat-lite".to_string()),
                ),
            ]),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let review =
            build_review_envelope(&answers, &template, None, Some(&provider_catalog)).unwrap();

        assert_eq!(
            review.composition.agents[0].capability_bindings[0]
                .provider_binding
                .provider_id,
            "provider.llm.openai.chat"
        );
        assert_eq!(
            review.composition.agents[1].capability_bindings[0]
                .provider_binding
                .provider_id,
            "provider.llm.openai.chat-lite"
        );
        assert_eq!(review.bundle_plan.provider_packs.len(), 2);
    }

    #[test]
    fn prompt_design_flow_sets_review_mode_agent_count_and_provider_override() {
        let examples_dir = workspace_examples_dir();
        let template_catalog = greentic_dw_types::TemplateCatalog::from_json_path(
            examples_dir.join("templates/catalog.json"),
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_path(
            examples_dir.join("providers/catalog.json"),
        )
        .unwrap();
        let template = template_catalog
            .resolve_template("dw.support-assistant")
            .unwrap();
        let selected_entry = template_catalog.find("dw.support-assistant");

        let mut answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: None,
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        prompt_design_flow_with(
            &mut answers,
            &template,
            selected_entry,
            Some(&provider_catalog),
            |prompt: &str| {
                if prompt.contains("recommended setup") {
                    Ok("n".to_string())
                } else if prompt.contains("agent count") {
                    Ok("2".to_string())
                } else if prompt.contains("shared providers") {
                    Ok(String::new())
                } else if prompt.contains("Provider override") && prompt.contains("cap://llm/chat")
                {
                    Ok("provider.llm.openai.chat-lite".to_string())
                } else if prompt.contains("support assistant behavior guidance") {
                    Ok("Warm and escalation-aware".to_string())
                } else {
                    Ok(String::new())
                }
            },
        )
        .unwrap();

        assert_eq!(
            answers.review_mode,
            Some(greentic_dw_types::DwResolutionMode::ReviewAll)
        );
        assert_eq!(
            answers.design_answers.get("agent_count"),
            Some(&serde_json::Value::String("2".to_string()))
        );
        assert_eq!(
            answers
                .provider_overrides
                .get("cap://llm/chat")
                .map(String::as_str),
            Some("provider.llm.openai.chat-lite")
        );
    }

    #[test]
    fn prompt_design_flow_captures_template_block_answers() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "question_blocks": [
                {
                  "block_id": "dw.support.behavior",
                  "answer_key": "support_behavior",
                  "prompt": "Enter support assistant behavior guidance: ",
                  "required": true
                }
              ],
              "behavior_scaffold": {
                "default_mode_behavior": {
                  "question_block_ids": ["dw.support.behavior"]
                },
                "personalised_mode_behavior": {
                  "question_block_ids": ["dw.support.behavior"]
                }
              }
            }"#,
        )
        .unwrap();

        let mut answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::Recommended),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        let mut responses = VecDeque::from(vec!["Warm, concise, and escalation-aware".to_string()]);

        prompt_design_flow_with(&mut answers, &template, None, None, |_: &str| {
            Ok(responses.pop_front().unwrap_or_default())
        })
        .unwrap();

        assert_eq!(
            answers.design_answers.get("support_behavior"),
            Some(&serde_json::Value::String(
                "Warm, concise, and escalation-aware".to_string()
            ))
        );
    }

    #[test]
    fn prompt_design_flow_captures_provider_block_answers() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {
                "required_capabilities": [
                  "cap://llm/chat",
                  "cap://memory/short-term"
                ],
                "default_provider_ids": {
                  "cap://llm/chat": "provider.llm.openai.chat",
                  "cap://memory/short-term": "provider.memory.redis"
                }
              },
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              }
            }"#,
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.openai.chat",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat",
                  "summary": "Managed LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  },
                  "required_question_block_ids": ["provider.llm.chat.openai"],
                  "question_blocks": [
                    {
                      "block_id": "provider.llm.chat.openai",
                      "answer_key": "openai_api_key_secret",
                      "prompt": "Enter OpenAI API key secret name: "
                    }
                  ]
                },
                {
                  "provider_id": "provider.memory.redis",
                  "family": "memory",
                  "category": "short_term",
                  "display_name": "Redis Memory",
                  "summary": "Memory provider",
                  "source_ref": {
                    "raw_ref": "repo://providers/memory/redis",
                    "kind": "repo"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://memory/short-term"]
                  },
                  "question_blocks": [
                    {
                      "block_id": "provider.memory.redis",
                      "answer_key": "redis_connection_url",
                      "prompt": "Enter Redis connection URL: "
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();

        let mut answers = AnswerDocument {
            manifest_id: "dw.support-assistant".to_string(),
            display_name: "Support Assistant".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::from([
                (
                    "cap://llm/chat".to_string(),
                    "provider.llm.openai.chat".to_string(),
                ),
                (
                    "cap://memory/short-term".to_string(),
                    "provider.memory.redis".to_string(),
                ),
            ]),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        prompt_design_flow_with(
            &mut answers,
            &template,
            None,
            Some(&provider_catalog),
            |prompt: &str| {
                if prompt.contains("OpenAI") {
                    Ok("secret/openai-api-key".to_string())
                } else if prompt.contains("Redis") {
                    Ok("redis://localhost:6379/0".to_string())
                } else {
                    Ok(String::new())
                }
            },
        )
        .unwrap();

        assert_eq!(
            answers.design_answers.get("openai_api_key_secret"),
            Some(&serde_json::Value::String(
                "secret/openai-api-key".to_string()
            ))
        );
        assert_eq!(
            answers.design_answers.get("redis_connection_url"),
            Some(&serde_json::Value::String(
                "redis://localhost:6379/0".to_string()
            ))
        );
    }

    #[test]
    fn prompt_design_flow_captures_agent_scoped_multi_agent_answers() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
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
                  "cap://llm/chat": "provider.llm.openai.chat"
                }
              },
              "question_blocks": [
                {
                  "block_id": "dw.support.behavior",
                  "answer_key": "support_behavior",
                  "prompt": "Enter support assistant behavior guidance: ",
                  "required": true
                }
              ],
              "behavior_scaffold": {
                "default_mode_behavior": {
                  "question_block_ids": ["dw.support.behavior"]
                },
                "personalised_mode_behavior": {
                  "question_block_ids": ["dw.support.behavior"]
                }
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();
        let provider_catalog = greentic_dw_types::DwProviderCatalog::from_json_str(
            r#"{
              "entries": [
                {
                  "provider_id": "provider.llm.openai.chat",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat",
                  "summary": "Managed LLM provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  },
                  "required_question_block_ids": ["provider.llm.chat.openai"],
                  "question_blocks": [
                    {
                      "block_id": "provider.llm.chat.openai",
                      "answer_key": "openai_api_key_secret",
                      "prompt": "Enter OpenAI API key secret name: "
                    }
                  ]
                },
                {
                  "provider_id": "provider.llm.openai.chat-lite",
                  "family": "llm",
                  "category": "chat",
                  "display_name": "OpenAI Chat Lite",
                  "summary": "Lower cost provider",
                  "source_ref": {
                    "raw_ref": "oci://ghcr.io/greenticai/packs/providers/llm/openai-chat-lite:latest",
                    "kind": "oci"
                  },
                  "maturity": "stable",
                  "capability_profile": {
                    "capability_contract_ids": ["cap://llm/chat"]
                  },
                  "required_question_block_ids": ["provider.llm.chat.openai"],
                  "question_blocks": [
                    {
                      "block_id": "provider.llm.chat.openai",
                      "answer_key": "openai_api_key_secret",
                      "prompt": "Enter OpenAI API key secret name: "
                    }
                  ]
                }
              ]
            }"#,
        )
        .unwrap();

        let mut answers = AnswerDocument {
            manifest_id: "dw.support-app".to_string(),
            display_name: "Support App".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::new(),
            agent_answers: std::collections::BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };

        prompt_design_flow_with(
            &mut answers,
            &template,
            None,
            Some(&provider_catalog),
            |prompt: &str| {
                if prompt.contains("agent count") {
                    Ok("2".to_string())
                } else if prompt.contains("shared providers") {
                    Ok("n".to_string())
                } else if prompt.starts_with("Agent 1: Enter support assistant behavior") {
                    Ok("Agent one guidance".to_string())
                } else if prompt.starts_with("Agent 2: Enter support assistant behavior") {
                    Ok("Agent two guidance".to_string())
                } else if prompt.contains("Agent 1:")
                    && prompt.contains("Provider override")
                    && prompt.contains("cap://llm/chat")
                {
                    Ok("provider.llm.openai.chat".to_string())
                } else if prompt.contains("Agent 2:")
                    && prompt.contains("Provider override")
                    && prompt.contains("cap://llm/chat")
                {
                    Ok("provider.llm.openai.chat-lite".to_string())
                } else if prompt.starts_with("Agent 1: Enter OpenAI API key secret name") {
                    Ok("secret/agent-1-openai".to_string())
                } else if prompt.starts_with("Agent 2: Enter OpenAI API key secret name") {
                    Ok("secret/agent-2-openai".to_string())
                } else {
                    Ok(String::new())
                }
            },
        )
        .unwrap();

        assert_eq!(
            answers.design_answers.get("agent.1.support_behavior"),
            Some(&serde_json::Value::String("Agent one guidance".to_string()))
        );
        assert_eq!(
            answers.design_answers.get("agent.2.support_behavior"),
            Some(&serde_json::Value::String("Agent two guidance".to_string()))
        );
        assert_eq!(
            answers
                .design_answers
                .get("agent.1.provider.cap://llm/chat"),
            Some(&serde_json::Value::String(
                "provider.llm.openai.chat".to_string()
            ))
        );
        assert_eq!(
            answers
                .design_answers
                .get("agent.2.provider.cap://llm/chat"),
            Some(&serde_json::Value::String(
                "provider.llm.openai.chat-lite".to_string()
            ))
        );
        assert_eq!(
            answers.design_answers.get("agent.1.openai_api_key_secret"),
            Some(&serde_json::Value::String(
                "secret/agent-1-openai".to_string()
            ))
        );
        assert_eq!(
            answers.design_answers.get("agent.2.openai_api_key_secret"),
            Some(&serde_json::Value::String(
                "secret/agent-2-openai".to_string()
            ))
        );
        assert_eq!(
            answers
                .agent_answers
                .get("agent-1")
                .and_then(|agent| agent.design_answers.get("support_behavior")),
            Some(&serde_json::Value::String("Agent one guidance".to_string()))
        );
        assert_eq!(
            answers
                .agent_answers
                .get("agent-2")
                .and_then(|agent| agent.provider_overrides.get("cap://llm/chat")),
            Some(&"provider.llm.openai.chat-lite".to_string())
        );
        assert_eq!(
            answers
                .agent_answers
                .get("agent-2")
                .and_then(|agent| agent.design_answers.get("openai_api_key_secret")),
            Some(&serde_json::Value::String(
                "secret/agent-2-openai".to_string()
            ))
        );
    }

    #[test]
    fn build_dry_run_output_emits_structured_agent_answers() {
        let template = greentic_dw_types::DigitalWorkerTemplate::from_json_str(
            r#"{
              "metadata": {
                "id": "dw.support-assistant",
                "name": "Support Assistant",
                "summary": "Handles support intake.",
                "maturity": "stable"
              },
              "capability_plan": {},
              "behavior_scaffold": {
                "default_mode_behavior": {},
                "personalised_mode_behavior": {}
              },
              "supports_multi_agent_app_pack": true
            }"#,
        )
        .unwrap();

        let answers = AnswerDocument {
            manifest_id: "dw.support-app".to_string(),
            display_name: "Support App".to_string(),
            manifest_version: "0.5".to_string(),
            tenant: "tenant-a".to_string(),
            template_id: Some("dw.support-assistant".to_string()),
            review_mode: Some(greentic_dw_types::DwResolutionMode::ReviewAll),
            provider_overrides: std::collections::BTreeMap::new(),
            design_answers: std::collections::BTreeMap::from([(
                "agent_count".to_string(),
                serde_json::Value::String("2".to_string()),
            )]),
            agent_answers: std::collections::BTreeMap::from([(
                "agent-1".to_string(),
                crate::cli_types::AgentAnswerDocument {
                    provider_overrides: std::collections::BTreeMap::from([(
                        "cap://llm/chat".to_string(),
                        "provider.llm.openai.chat".to_string(),
                    )]),
                    design_answers: std::collections::BTreeMap::from([(
                        "support_behavior".to_string(),
                        serde_json::Value::String("Agent one guidance".to_string()),
                    )]),
                },
            )]),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        };
        let manifest = build_manifest(&answers);
        let request_scope = greentic_dw_manifest::RequestScope {
            tenant: answers.tenant.clone(),
            team: answers.team.clone(),
        };
        let envelope = manifest
            .to_task_envelope(
                format!("{}-task", answers.manifest_id),
                answers.manifest_id.clone(),
                &request_scope,
                answers.requested_locale.clone(),
                answers.human_locale.clone(),
            )
            .unwrap();

        let output = build_dry_run_output(
            &manifest,
            &envelope,
            &answers,
            Some(&template),
            None,
            None,
            true,
        )
        .unwrap();

        let emitted_answers = output.answers.expect("emitted answers");
        assert_eq!(
            emitted_answers
                .agent_answers
                .get("agent-1")
                .and_then(|agent| agent.design_answers.get("support_behavior")),
            Some(&serde_json::Value::String("Agent one guidance".to_string()))
        );
        assert_eq!(
            emitted_answers
                .agent_answers
                .get("agent-1")
                .and_then(|agent| agent.provider_overrides.get("cap://llm/chat")),
            Some(&"provider.llm.openai.chat".to_string())
        );
    }
}
