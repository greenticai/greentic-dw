//! Runs the real `greentic-dw` binary so its `main` (success and error exit
//! paths) is exercised and counted by the coverage policy.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_greentic-dw"))
}

#[test]
fn binary_prints_answer_schema_and_exits_zero() {
    let output = bin()
        .args(["wizard", "--schema"])
        .output()
        .expect("spawn greentic-dw");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is a JSON schema");
    assert!(schema.is_object());
}

#[test]
fn binary_reports_cli_errors_on_stderr_and_exits_one() {
    let output = bin()
        .args(["wizard", "--list-templates"])
        .output()
        .expect("spawn greentic-dw");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("greentic-dw: "), "stderr: {stderr}");
}
