use clap::{Args, Parser, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use thiserror::Error;

pub const CONTRACT_VERSION: &str = "greentic-dw-cli/v1";

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
    #[error("answers document URL must use https: {url}")]
    InsecureAnswersUrl { url: String },
    #[error("interactive input failed: {0}")]
    Input(#[from] io::Error),
    #[error("`{usage}` requires --template-catalog <path>")]
    TemplateCatalogPathRequired { usage: String },
    #[error(transparent)]
    TemplateCatalog(#[from] greentic_dw_types::TemplateCatalogError),
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
    pub(crate) command: Command,
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum Command {
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
    /// Template catalog JSON used for listing or selecting templates.
    #[arg(long)]
    pub template_catalog: Option<PathBuf>,
    /// Print template catalog entries and exit.
    #[arg(long)]
    pub list_templates: bool,
    /// Template id to resolve from --template-catalog and use for defaults.
    #[arg(long)]
    pub template: Option<String>,
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
