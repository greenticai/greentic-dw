#[cfg(test)]
mod tests {
    use crate::cli_types::{AnswerDocument, CliError, WizardArgs};
    use crate::i18n::{MsgKey, localized};
    use crate::wizard::{
        apply_overrides, apply_template_defaults, build_manifest, is_insecure_http_url,
        is_remote_answers_url, load_answers, run,
    };
    use greentic_dw_manifest::MANIFEST_SCHEMA_VERSION;
    use schemars::schema_for;
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
            list_templates: false,
            template: None,
            manifest_id: Some("dw.sample".to_string()),
            display_name: Some("Sample".to_string()),
            tenant: Some("tenant-b".to_string()),
            team: Some("team-x".to_string()),
            requested_locale: Some("fr-FR".to_string()),
            human_locale: Some("nl-NL".to_string()),
        };

        apply_overrides(&mut answers, &args);
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
}
