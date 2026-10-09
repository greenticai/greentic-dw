//! Wizard conformance tests. They lived in greentic-dw-testing until the CLI
//! left the workspace (see the `exclude` comment in the root `Cargo.toml`), and
//! move with it so the testing crate no longer depends on the CLI.

use std::path::PathBuf;

fn workspace_examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .canonicalize()
        .expect("workspace examples dir")
}

#[test]
fn conformance_wizard_dry_run_contract_executes() {
    let args = vec![
        "greentic-dw",
        "wizard",
        "--non-interactive",
        "--manifest-id",
        "dw.fixture",
        "--display-name",
        "DW Fixture",
        "--tenant",
        "tenant-a",
        "--dry-run",
        "--emit-answers",
    ];

    greentic_dw_cli::run(args).expect("wizard dry-run should succeed");
}

#[test]
fn conformance_wizard_dry_run_replays_structured_multi_agent_answers() {
    let examples_dir = workspace_examples_dir();
    let answers_path = examples_dir.join("answers/support-squad-create-answers.json");
    let template_catalog_path = examples_dir.join("templates/catalog.json");
    let provider_catalog_path = examples_dir.join("providers/catalog.json");
    let args = vec![
        "greentic-dw",
        "wizard",
        "--non-interactive",
        "--dry-run",
        "--emit-answers",
        "--answers",
        answers_path.to_str().expect("answers path"),
        "--template-catalog",
        template_catalog_path
            .to_str()
            .expect("template catalog path"),
        "--template",
        "dw.support-assistant",
        "--provider-catalog",
        provider_catalog_path
            .to_str()
            .expect("provider catalog path"),
    ];

    greentic_dw_cli::run(args).expect("structured multi-agent wizard dry-run should succeed");
}
