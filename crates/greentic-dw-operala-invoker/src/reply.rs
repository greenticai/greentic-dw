//! Turn a finished deep-loop run into the prose `output.reply`.
//!
//! The deep loop produces no prose itself (artifact bodies are placeholders and
//! the final review is a verdict), and greentic-runner's operala node falls back
//! to the whole output object when `reply` is absent — which an operator reads as
//! a JSON blob. One extra LLM call after the loop writes the answer; if it fails
//! or returns nothing, a status sentence is used instead. Never JSON.

use greentic_dw_planning::PlanStepStatus;
use greentic_dw_runtime::{DeepLoopRun, DeepLoopStatus};
use greentic_llm::{ChatMessage, ChatRequest, LlmProvider};

const REPLY_SYSTEM_PROMPT: &str = "You write the final answer of a deep worker to the person \
who asked for the task. Using the goal, the outcome and the plan below, reply in a few \
sentences of plain prose (no JSON, no markdown headings) saying what was done and what the \
result is. If the task did not complete, say so plainly and say what was left undone.";

fn step_status_label(status: &PlanStepStatus) -> &'static str {
    match status {
        PlanStepStatus::Pending => "pending",
        PlanStepStatus::Ready => "ready",
        PlanStepStatus::Running => "running",
        PlanStepStatus::Blocked => "blocked",
        PlanStepStatus::Completed => "completed",
        PlanStepStatus::Failed => "failed",
        PlanStepStatus::Skipped => "skipped",
    }
}

/// The user message of the synthesis call: goal, outcome, then every step.
pub(crate) fn reply_user_prompt(goal: &str, run: &DeepLoopRun) -> String {
    let steps = if run.plan.steps.is_empty() {
        "(no steps)".to_string()
    } else {
        run.plan
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                format!(
                    "{}. {} — {}",
                    index + 1,
                    step.title,
                    step_status_label(&step.status)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Goal: {goal}\nOutcome: {}\n\nPlan steps:\n{steps}",
        run.status
    )
}

fn steps_word(count: usize) -> &'static str {
    if count == 1 { "step" } else { "steps" }
}

/// Human-readable status sentence used when synthesis fails or is empty.
pub(crate) fn fallback_reply(run: &DeepLoopRun) -> String {
    let total = run.plan.steps.len();
    let done = run
        .plan
        .steps
        .iter()
        .filter(|step| step.status == PlanStepStatus::Completed)
        .count();
    match run.status {
        DeepLoopStatus::Completed => {
            format!("The task completed in {done} {}.", steps_word(done))
        }
        DeepLoopStatus::BudgetExhausted => format!(
            "The task stopped after reaching its step budget, with {done} of {total} {} done.",
            steps_word(total)
        ),
        DeepLoopStatus::Failed => format!(
            "The task could not be completed; {done} of {total} {} were done before it stopped.",
            steps_word(total)
        ),
        DeepLoopStatus::Revising => format!(
            "The task stopped because its plan needed revising; {done} of {total} {} were done.",
            steps_word(total)
        ),
        DeepLoopStatus::Delegating => {
            "The task was handed to other workers and has no result yet.".to_string()
        }
        DeepLoopStatus::Idle
        | DeepLoopStatus::Planning
        | DeepLoopStatus::Executing
        | DeepLoopStatus::Reflecting => format!("The task stopped while {}.", run.status),
    }
}

/// One LLM call summarising the run. Falls back to [`fallback_reply`] on an
/// error or an empty answer, and logs why.
pub(crate) async fn synthesize_reply(
    llm: &dyn LlmProvider,
    goal: &str,
    run: &DeepLoopRun,
) -> String {
    let request = ChatRequest {
        messages: vec![
            ChatMessage::system(REPLY_SYSTEM_PROMPT),
            ChatMessage::user(reply_user_prompt(goal, run)),
        ],
        tools: vec![],
        tool_choice: None,
        max_tokens: Some(1024),
        temperature: Some(0.3),
    };
    match llm.chat(request).await {
        Ok(response) => {
            let text = response.content.trim();
            if text.is_empty() {
                tracing::warn!(status = %run.status, "deep-worker reply synthesis returned no text; using the status sentence");
                fallback_reply(run)
            } else {
                text.to_string()
            }
        }
        Err(error) => {
            tracing::warn!(status = %run.status, %error, "deep-worker reply synthesis failed; using the status sentence");
            fallback_reply(run)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_planning::{PlanDocument, PlanStatus, PlanStep, PlanStepKind, PlanStepStatus};
    use std::collections::BTreeMap;

    fn run(status: DeepLoopStatus, steps: &[(&str, PlanStepStatus)]) -> DeepLoopRun {
        DeepLoopRun {
            plan: PlanDocument {
                plan_id: "p".into(),
                goal: "g".into(),
                status: PlanStatus::Active,
                revision: 1,
                assumptions: vec![],
                constraints: vec![],
                success_criteria: vec!["task completed".into()],
                steps: steps
                    .iter()
                    .enumerate()
                    .map(|(i, (title, status))| PlanStep {
                        step_id: format!("s{i}"),
                        title: (*title).to_string(),
                        kind: PlanStepKind::ToolCall,
                        status: status.clone(),
                        depends_on: vec![],
                        assigned_agent: None,
                        inputs_schema_ref: None,
                        output_schema_ref: None,
                        retry_count: 0,
                    })
                    .collect(),
                edges: vec![],
                metadata: BTreeMap::new(),
            },
            status,
            emitted_subtasks: vec![],
            output_artifact_ids: vec![],
        }
    }

    #[test]
    fn user_prompt_carries_goal_outcome_and_every_step() {
        let prompt = reply_user_prompt(
            "Refund order 42",
            &run(
                DeepLoopStatus::BudgetExhausted,
                &[
                    ("Look up the order", PlanStepStatus::Completed),
                    ("Issue the refund", PlanStepStatus::Ready),
                ],
            ),
        );
        assert!(prompt.contains("Goal: Refund order 42"));
        assert!(prompt.contains("Outcome: budget_exhausted"));
        assert!(prompt.contains("1. Look up the order — completed"));
        assert!(prompt.contains("2. Issue the refund — ready"));
    }

    #[test]
    fn fallback_sentences_are_prose_for_every_terminal_status() {
        let done = [
            ("A", PlanStepStatus::Completed),
            ("B", PlanStepStatus::Completed),
        ];
        assert_eq!(
            fallback_reply(&run(DeepLoopStatus::Completed, &done)),
            "The task completed in 2 steps."
        );
        assert_eq!(
            fallback_reply(&run(
                DeepLoopStatus::Completed,
                &[("A", PlanStepStatus::Completed)]
            )),
            "The task completed in 1 step."
        );
        let half = [
            ("A", PlanStepStatus::Completed),
            ("B", PlanStepStatus::Ready),
        ];
        assert_eq!(
            fallback_reply(&run(DeepLoopStatus::BudgetExhausted, &half)),
            "The task stopped after reaching its step budget, with 1 of 2 steps done."
        );
        assert_eq!(
            fallback_reply(&run(DeepLoopStatus::Failed, &half)),
            "The task could not be completed; 1 of 2 steps were done before it stopped."
        );
        assert_eq!(
            fallback_reply(&run(DeepLoopStatus::Revising, &half)),
            "The task stopped because its plan needed revising; 1 of 2 steps were done."
        );
        assert_eq!(
            fallback_reply(&run(DeepLoopStatus::Delegating, &half)),
            "The task was handed to other workers and has no result yet."
        );
        for sentence in [
            fallback_reply(&run(DeepLoopStatus::Planning, &[])),
            fallback_reply(&run(DeepLoopStatus::Failed, &[])),
        ] {
            assert!(
                !sentence.trim_start().starts_with('{'),
                "never JSON: {sentence}"
            );
        }
    }
}
