use crate::cli_types::{
    AnswerDocument, CONTRACT_VERSION, Cli, CliError, Command, WizardArgs, WizardOutput,
};
use crate::i18n::{MsgKey, prompt};
use clap::Parser;
use greentic_cap_types::CapabilityDeclaration;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, MANIFEST_SCHEMA_VERSION, RequestScope, TeamPolicy,
    TenancyContract,
};
use greentic_dw_runtime::DwRuntime;
use greentic_dw_types::{
    DigitalWorkerTemplate, LocalePropagation, OutputLocaleGuidance, TemplateCatalog,
    WorkerLocalePolicy,
};
use schemars::schema_for;
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::Path;

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

    if args.list_templates {
        let catalog_path = args.template_catalog.as_ref().ok_or_else(|| {
            CliError::TemplateCatalogPathRequired {
                usage: "--list-templates".to_string(),
            }
        })?;
        let catalog = TemplateCatalog::from_json_path(catalog_path)?;
        let output = WizardOutput {
            contract_version: CONTRACT_VERSION.to_string(),
            command: "wizard".to_string(),
            mode: "template_catalog".to_string(),
            answers: None,
            data: serde_json::json!({
                "catalog_path": catalog_path,
                "entries": catalog.entries,
            }),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut answers = if let Some(path) = &args.answers {
        load_answers(path)?
    } else {
        AnswerDocument {
            manifest_id: String::new(),
            display_name: String::new(),
            manifest_version: "0.5".to_string(),
            tenant: String::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        }
    };

    let selected_template = if let Some(template_id) = &args.template {
        let catalog_path = args.template_catalog.as_ref().ok_or_else(|| {
            CliError::TemplateCatalogPathRequired {
                usage: "--template".to_string(),
            }
        })?;
        let catalog = TemplateCatalog::from_json_path(catalog_path)?;
        let template = catalog.resolve_template(template_id)?;
        apply_template_defaults(&mut answers, &template);
        Some(template)
    } else {
        None
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

    let mut output = output;
    if let Some(template) = selected_template
        && let Some(data) = output.data.as_object_mut()
    {
        data.insert(
            "selected_template".to_string(),
            serde_json::json!({
                "id": template.metadata.id,
                "name": template.metadata.name,
                "supports_multi_agent_app_pack": template.supports_multi_agent_app_pack,
            }),
        );
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub(crate) fn load_answers(path: &Path) -> Result<AnswerDocument, CliError> {
    let path_display = path.display().to_string();
    let raw = if is_remote_answers_url(&path_display) {
        fetch_answers_from_url(&path_display)?
    } else if is_insecure_http_url(&path_display) {
        return Err(CliError::InsecureAnswersUrl { url: path_display });
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

pub(crate) fn is_remote_answers_url(value: &str) -> bool {
    value.starts_with("https://")
}

pub(crate) fn is_insecure_http_url(value: &str) -> bool {
    value.starts_with("http://")
}

fn fetch_answers_from_url(url: &str) -> Result<String, CliError> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|error| CliError::AnswersFetch {
            url: url.to_string(),
            message: error.to_string(),
        })?;

    response
        .body_mut()
        .read_to_string()
        .map_err(|error| CliError::AnswersFetch {
            url: url.to_string(),
            message: error.to_string(),
        })
}

pub(crate) fn apply_overrides(answers: &mut AnswerDocument, args: &WizardArgs) {
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

pub(crate) fn apply_template_defaults(
    answers: &mut AnswerDocument,
    template: &DigitalWorkerTemplate,
) {
    if answers.manifest_id.trim().is_empty() {
        answers.manifest_id = template.metadata.id.clone();
    }
    if answers.display_name.trim().is_empty() {
        answers.display_name = template.metadata.name.clone();
    }

    for (key, value) in &template.defaults.values {
        match (key.as_str(), value.as_str()) {
            ("manifest_id", Some(v)) if answers.manifest_id.trim().is_empty() => {
                answers.manifest_id = v.to_string()
            }
            ("display_name", Some(v)) if answers.display_name.trim().is_empty() => {
                answers.display_name = v.to_string()
            }
            ("manifest_version", Some(v)) if answers.manifest_version.trim().is_empty() => {
                answers.manifest_version = v.to_string()
            }
            ("tenant", Some(v)) if answers.tenant.trim().is_empty() => {
                answers.tenant = v.to_string()
            }
            ("team", Some(v)) if answers.team.is_none() => answers.team = Some(v.to_string()),
            ("requested_locale", Some(v)) if answers.requested_locale.is_none() => {
                answers.requested_locale = Some(v.to_string())
            }
            ("human_locale", Some(v)) if answers.human_locale.is_none() => {
                answers.human_locale = Some(v.to_string())
            }
            ("worker_default_locale", Some(v))
                if answers.worker_default_locale.trim().is_empty() =>
            {
                answers.worker_default_locale = v.to_string()
            }
            _ => {}
        }
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

pub(crate) fn build_manifest(answers: &AnswerDocument) -> DigitalWorkerManifest {
    DigitalWorkerManifest {
        version: MANIFEST_SCHEMA_VERSION.to_string(),
        id: answers.manifest_id.clone(),
        display_name: answers.display_name.clone(),
        worker_version: Some(answers.manifest_version.clone()),
        capabilities: CapabilityDeclaration::new(),
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
        deep_agent: None,
    }
}

pub fn print_error(error: &impl Display) {
    eprintln!("greentic-dw: {error}");
}
