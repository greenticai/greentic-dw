//! Execution stage of the deep loop: the seam through which a host lets a deep
//! worker ACT on a plan step (typically by running a bounded tool-calling loop).
//!
//! The trait is synchronous and knows nothing about LLMs — the loop runs on a
//! blocking thread and an implementation bridges to whatever it needs. With no
//! executor wired, [`crate::DeepLoopCoordinator`] behaves exactly as it did
//! before this stage existed: a runtime tick plus a placeholder artifact body.

use greentic_dw_planning::PlanStep;
use serde_json::Value;
use thiserror::Error;

/// Tool calls a single step may make, whatever budget remains for the run.
pub const PER_STEP_TOOL_CAP: usize = 6;

/// Tool calls granted per planner iteration. The run's total tool-call budget
/// is `max_iterations * TOOL_CALLS_PER_ITERATION`, separate from the iteration
/// budget itself.
pub const TOOL_CALLS_PER_ITERATION: usize = 3;

/// Everything an executor is told about the step it is asked to carry out.
#[derive(Debug, Clone, Copy)]
pub struct ExecuteStepRequest<'a> {
    /// The plan goal.
    pub goal: &'a str,
    /// The step to execute (never a `Delegate` step).
    pub step: &'a PlanStep,
    /// Rendered knowledge for the step, when the context provider produced any.
    pub context: Option<&'a str>,
    /// Bodies of the steps executed earlier in this run, oldest first.
    pub prior_outputs: &'a [Value],
    /// Tool calls this execution may make. `0` means answer from what is
    /// already known — that is not an error.
    pub max_tool_calls: usize,
}

/// The result of executing one step.
#[derive(Debug, Clone, PartialEq)]
pub struct StepExecution {
    /// The step's output, written verbatim into its workspace artifact.
    pub body: Value,
    /// Tool calls that counted against the budget. The coordinator never
    /// charges more than the `max_tool_calls` it granted.
    pub tool_calls_used: usize,
}

/// A step could not be executed at all. A failing TOOL is not this error: an
/// executor reports that inside [`StepExecution::body`] so the model can react.
#[derive(Debug, Error)]
pub enum StepExecutionError {
    /// The model or another backend the executor depends on failed.
    #[error("step executor backend failed: {0}")]
    Backend(String),
    /// The executor could not use its own state (e.g. a poisoned lock).
    #[error("step executor internal error: {0}")]
    Internal(String),
}

/// Carries out one non-delegate plan step.
pub trait StepExecutor: Send + Sync {
    fn execute_step(
        &self,
        request: ExecuteStepRequest<'_>,
    ) -> Result<StepExecution, StepExecutionError>;
}

/// Tool-call accounting for one deep-loop run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolBudget {
    total: usize,
    used: usize,
}

impl ToolBudget {
    pub(crate) fn for_iterations(max_iterations: usize) -> Self {
        Self {
            total: max_iterations.saturating_mul(TOOL_CALLS_PER_ITERATION),
            used: 0,
        }
    }

    /// The grant for the next step: the per-step cap, bounded by what remains.
    pub(crate) fn grant(&self) -> usize {
        PER_STEP_TOOL_CAP.min(self.remaining())
    }

    pub(crate) fn remaining(&self) -> usize {
        self.total.saturating_sub(self.used)
    }

    pub(crate) fn used(&self) -> usize {
        self.used
    }

    /// Charge an execution, never more than it was granted.
    pub(crate) fn charge(&mut self, granted: usize, reported: usize) {
        self.used = self.used.saturating_add(reported.min(granted));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grant_is_the_per_step_cap_until_the_total_runs_low() {
        let mut budget = ToolBudget::for_iterations(3);
        assert_eq!(budget.remaining(), 9);
        assert_eq!(budget.grant(), PER_STEP_TOOL_CAP);
        budget.charge(6, 6);
        assert_eq!(budget.grant(), 3);
        budget.charge(3, 3);
        assert_eq!(budget.grant(), 0);
        assert_eq!(budget.used(), 9);
    }

    #[test]
    fn an_executor_is_never_charged_more_than_it_was_granted() {
        let mut budget = ToolBudget::for_iterations(1);
        budget.charge(2, 50);
        assert_eq!(budget.used(), 2);
        assert_eq!(budget.remaining(), 1);
    }
}
