//! DW CLI and localized wizard flow.

use clap::{Args, Parser, Subcommand};
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, RequestScope, TeamPolicy, TenancyContract,
};
use greentic_dw_runtime::DwRuntime;
use greentic_dw_types::{LocalePropagation, OutputLocaleGuidance, WorkerLocalePolicy};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

const CONTRACT_VERSION: &str = "greentic-dw-cli/v1";

#[derive(Debug, Error)]
pub enum CliError {
    #[error("failed to parse answers document at {path}: {source}")]
    AnswersParse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to read answers document at {path}: {source}")]
    AnswersRead {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to fetch answers document from {url}: {message}")]
    AnswersFetch { url: String, message: String },
    #[error("interactive input failed: {0}")]
    Input(#[from] io::Error),
    #[error(transparent)]
    Manifest(#[from] greentic_dw_manifest::ManifestValidationError),
    #[error(transparent)]
    Runtime(#[from] greentic_dw_runtime::RuntimeError),
    #[error("failed to serialize output: {0}")]
    OutputSerialize(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Parser)]
#[command(name = "greentic-dw", version, about = "Greentic Digital Worker CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    /// Run the localized DW wizard.
    Wizard(WizardArgs),
}

#[derive(Debug, Clone, Args)]
pub struct WizardArgs {
    /// Existing AnswerDocument JSON file to replay.
    #[arg(long)]
    pub answers: Option<PathBuf>,
    /// Print AnswerDocument JSON schema and exit.
    #[arg(long)]
    pub schema: bool,
    /// Include collected AnswerDocument in output.
    #[arg(long)]
    pub emit_answers: bool,
    /// Do not execute runtime; return a dry-run plan.
    #[arg(long)]
    pub dry_run: bool,
    /// Wizard locale used for prompt text.
    #[arg(long, default_value = "en")]
    pub locale: String,
    /// Disable prompts; require values from --answers and/or flags.
    #[arg(long)]
    pub non_interactive: bool,
    #[arg(long)]
    pub manifest_id: Option<String>,
    #[arg(long)]
    pub display_name: Option<String>,
    #[arg(long)]
    pub tenant: Option<String>,
    #[arg(long)]
    pub team: Option<String>,
    #[arg(long)]
    pub requested_locale: Option<String>,
    #[arg(long)]
    pub human_locale: Option<String>,
}

/// AnswerDocument-compatible payload for wizard replay/capture.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnswerDocument {
    pub manifest_id: String,
    pub display_name: String,
    pub manifest_version: String,
    pub tenant: String,
    pub team: Option<String>,
    pub requested_locale: Option<String>,
    pub human_locale: Option<String>,
    pub worker_default_locale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WizardOutput {
    pub contract_version: String,
    pub command: String,
    pub mode: String,
    pub answers: Option<AnswerDocument>,
    pub data: serde_json::Value,
}

pub fn run_from_env() -> Result<(), CliError> {
    run(std::env::args())
}

pub fn run<I, T>(args: I) -> Result<(), CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Command::Wizard(wizard) => run_wizard(wizard),
    }
}

fn run_wizard(args: WizardArgs) -> Result<(), CliError> {
    if args.schema {
        let schema = schema_for!(AnswerDocument);
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    let mut answers = if let Some(path) = &args.answers {
        load_answers(path)?
    } else {
        AnswerDocument {
            manifest_id: String::new(),
            display_name: String::new(),
            manifest_version: "0.1.0".to_string(),
            tenant: String::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        }
    };

    apply_overrides(&mut answers, &args);

    if !args.non_interactive {
        prompt_if_missing(&mut answers, &args.locale)?;
    }

    let manifest = build_manifest(&answers);
    manifest.validate()?;

    let request_scope = RequestScope {
        tenant: answers.tenant.clone(),
        team: answers.team.clone(),
    };

    let mut envelope = manifest.to_task_envelope(
        format!("{}-task", answers.manifest_id),
        answers.manifest_id.clone(),
        &request_scope,
        answers.requested_locale.clone(),
        answers.human_locale.clone(),
    )?;

    let output = if args.dry_run {
        let effective_locale = envelope
            .locale
            .resolve_effective_locale()
            .map(str::to_string);
        WizardOutput {
            contract_version: CONTRACT_VERSION.to_string(),
            command: "wizard".to_string(),
            mode: "dry_run".to_string(),
            answers: args.emit_answers.then_some(answers),
            data: serde_json::json!({
                "manifest_id": manifest.id,
                "tenant": envelope.scope.tenant,
                "team": envelope.scope.team,
                "state": format!("{:?}", envelope.state),
                "effective_locale": effective_locale,
                "requested_locale": envelope.locale.requested_locale,
                "human_locale": envelope.locale.human_locale,
            }),
        }
    } else {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Batch(vec![
            greentic_dw_core::RuntimeOperation::Start,
            greentic_dw_core::RuntimeOperation::Complete,
        ])));

        let events = runtime.tick(&mut envelope)?;

        WizardOutput {
            contract_version: CONTRACT_VERSION.to_string(),
            command: "wizard".to_string(),
            mode: "execute".to_string(),
            answers: args.emit_answers.then_some(answers),
            data: serde_json::json!({
                "final_state": format!("{:?}", envelope.state),
                "event_count": events.len(),
                "events": events.iter().map(|e| e.operation.name()).collect::<Vec<_>>(),
            }),
        }
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn load_answers(path: &Path) -> Result<AnswerDocument, CliError> {
    let path_display = path.display().to_string();
    let raw = if is_http_url(&path_display) {
        fetch_answers_from_url(&path_display)?
    } else {
        fs::read_to_string(path).map_err(|source| CliError::AnswersRead {
            path: path_display.clone(),
            source,
        })?
    };

    serde_json::from_str(&raw).map_err(|source| CliError::AnswersParse {
        path: path_display,
        source,
    })
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn fetch_answers_from_url(url: &str) -> Result<String, CliError> {
    let response = ureq::get(url)
        .call()
        .map_err(|error| CliError::AnswersFetch {
            url: url.to_string(),
            message: error.to_string(),
        })?;

    response
        .into_string()
        .map_err(|error| CliError::AnswersFetch {
            url: url.to_string(),
            message: error.to_string(),
        })
}

fn apply_overrides(answers: &mut AnswerDocument, args: &WizardArgs) {
    if let Some(v) = &args.manifest_id {
        answers.manifest_id = v.clone();
    }
    if let Some(v) = &args.display_name {
        answers.display_name = v.clone();
    }
    if let Some(v) = &args.tenant {
        answers.tenant = v.clone();
    }
    if let Some(v) = &args.team {
        answers.team = Some(v.clone());
    }
    if let Some(v) = &args.requested_locale {
        answers.requested_locale = Some(v.clone());
    }
    if let Some(v) = &args.human_locale {
        answers.human_locale = Some(v.clone());
    }
}

fn prompt_if_missing(answers: &mut AnswerDocument, locale: &str) -> Result<(), io::Error> {
    if answers.manifest_id.trim().is_empty() {
        answers.manifest_id = prompt(locale, MsgKey::ManifestId)?;
    }
    if answers.display_name.trim().is_empty() {
        answers.display_name = prompt(locale, MsgKey::DisplayName)?;
    }
    if answers.tenant.trim().is_empty() {
        answers.tenant = prompt(locale, MsgKey::Tenant)?;
    }
    if answers.team.is_none() {
        let v = prompt(locale, MsgKey::Team)?;
        if !v.trim().is_empty() {
            answers.team = Some(v);
        }
    }
    if answers.requested_locale.is_none() {
        let v = prompt(locale, MsgKey::RequestedLocale)?;
        if !v.trim().is_empty() {
            answers.requested_locale = Some(v);
        }
    }
    if answers.human_locale.is_none() {
        let v = prompt(locale, MsgKey::HumanLocale)?;
        if !v.trim().is_empty() {
            answers.human_locale = Some(v);
        }
    }

    Ok(())
}

fn build_manifest(answers: &AnswerDocument) -> DigitalWorkerManifest {
    DigitalWorkerManifest {
        id: answers.manifest_id.clone(),
        display_name: answers.display_name.clone(),
        version: answers.manifest_version.clone(),
        tenancy: TenancyContract {
            tenant: answers.tenant.clone(),
            team_policy: TeamPolicy::Optional {
                default_team: answers.team.clone(),
                allow_request_override: true,
            },
        },
        locale: LocaleContract {
            worker_default_locale: answers.worker_default_locale.clone(),
            policy: WorkerLocalePolicy::PreferRequested,
            propagation: LocalePropagation::PropagateToDelegates,
            output: OutputLocaleGuidance::MatchRequested,
        },
    }
}

#[derive(Clone, Copy)]
enum MsgKey {
    ManifestId,
    DisplayName,
    Tenant,
    Team,
    RequestedLocale,
    HumanLocale,
}

fn prompt(locale: &str, key: MsgKey) -> Result<String, io::Error> {
    let mut stderr = io::stderr().lock();
    write!(stderr, "{}", localized(locale, key))?;
    stderr.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn localized(locale: &str, key: MsgKey) -> &'static str {
    let lang = locale.split(['-', '_']).next().unwrap_or("en");

    match (lang, key) {
        ("nl", MsgKey::ManifestId) => "Manifest-ID invoeren: ",
        ("nl", MsgKey::DisplayName) => "Weergavenaam invoeren: ",
        ("nl", MsgKey::Tenant) => "Tenant invoeren: ",
        ("nl", MsgKey::Team) => "Team invoeren (optioneel): ",
        ("nl", MsgKey::RequestedLocale) => "Gevraagde locale invoeren (optioneel): ",
        ("nl", MsgKey::HumanLocale) => "Human locale invoeren (optioneel): ",
        (_, MsgKey::ManifestId) => "Enter manifest id: ",
        (_, MsgKey::DisplayName) => "Enter display name: ",
        (_, MsgKey::Tenant) => "Enter tenant: ",
        (_, MsgKey::Team) => "Enter team (optional): ",
        (_, MsgKey::RequestedLocale) => "Enter requested locale (optional): ",
        (_, MsgKey::HumanLocale) => "Enter human locale (optional): ",
    }
}

pub fn print_error(error: &impl Display) {
    eprintln!("greentic-dw: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn applies_cli_overrides_to_answers() {
        let mut answers = AnswerDocument {
            manifest_id: "a".to_string(),
            display_name: "A".to_string(),
            manifest_version: "0.1.0".to_string(),
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
    }

    #[test]
    fn build_manifest_sets_expected_contracts() {
        let answers = AnswerDocument {
            manifest_id: "dw.sample".to_string(),
            display_name: "Sample".to_string(),
            manifest_version: "0.1.0".to_string(),
            tenant: "tenant-a".to_string(),
            team: Some("team-1".to_string()),
            requested_locale: Some("fr-FR".to_string()),
            human_locale: Some("nl-NL".to_string()),
            worker_default_locale: "en-US".to_string(),
        };

        let manifest = build_manifest(&answers);
        assert_eq!(manifest.id, "dw.sample");
        assert_eq!(manifest.tenancy.tenant, "tenant-a");
    }

    #[test]
    fn load_answers_reads_valid_document() {
        let file = NamedTempFile::new().expect("temp file");
        let doc = serde_json::json!({
            "manifest_id": "dw.sample",
            "display_name": "Sample",
            "manifest_version": "0.1.0",
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
    fn detects_http_answers_source() {
        assert!(is_http_url(
            "https://github.com/greenticai/greentic-dw/releases/latest/download/orchestrator-create-answers.json"
        ));
        assert!(!is_http_url(
            "examples/answers/orchestrator-create-answers.json"
        ));
    }
}
