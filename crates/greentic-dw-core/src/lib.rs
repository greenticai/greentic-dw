//! Digital Worker runtime core operations and lifecycle transition logic.

use greentic_dw_types::{TaskEnvelope, TaskLifecycleState};
use thiserror::Error;

/// Runtime operation performed by the DW kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeOperation {
    Start,
    Step,
    Wait,
    Delegate { delegate_worker_id: String },
    Complete,
    Fail { reason: String },
    Cancel,
}

impl RuntimeOperation {
    pub fn name(&self) -> &'static str {
        match self {
            RuntimeOperation::Start => "start",
            RuntimeOperation::Step => "step",
            RuntimeOperation::Wait => "wait",
            RuntimeOperation::Delegate { .. } => "delegate",
            RuntimeOperation::Complete => "complete",
            RuntimeOperation::Fail { .. } => "fail",
            RuntimeOperation::Cancel => "cancel",
        }
    }
}

/// Structured event emitted when runtime applies an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvent {
    pub task_id: String,
    pub worker_id: String,
    pub operation: RuntimeOperation,
    pub from_state: TaskLifecycleState,
    pub to_state: TaskLifecycleState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreRuntimeError {
    #[error("illegal transition: operation '{operation}' from {from:?} to {to:?}")]
    IllegalTransition {
        operation: String,
        from: TaskLifecycleState,
        to: TaskLifecycleState,
    },
}

/// Computes target state for a given operation.
pub fn target_state(
    operation: &RuntimeOperation,
    current: TaskLifecycleState,
) -> TaskLifecycleState {
    match operation {
        RuntimeOperation::Start => TaskLifecycleState::Running,
        RuntimeOperation::Step => current,
        RuntimeOperation::Wait => TaskLifecycleState::Waiting,
        RuntimeOperation::Delegate { .. } => TaskLifecycleState::Delegated,
        RuntimeOperation::Complete => TaskLifecycleState::Completed,
        RuntimeOperation::Fail { .. } => TaskLifecycleState::Failed,
        RuntimeOperation::Cancel => TaskLifecycleState::Cancelled,
    }
}

/// Validates and applies an operation, returning a runtime event.
pub fn apply_operation(
    envelope: &mut TaskEnvelope,
    operation: RuntimeOperation,
) -> Result<RuntimeEvent, CoreRuntimeError> {
    let from_state = envelope.state;
    let to_state = target_state(&operation, from_state);

    if from_state != to_state && !from_state.can_transition_to(to_state) {
        return Err(CoreRuntimeError::IllegalTransition {
            operation: operation.name().to_string(),
            from: from_state,
            to: to_state,
        });
    }

    envelope.state = to_state;

    Ok(RuntimeEvent {
        task_id: envelope.task_id.clone(),
        worker_id: envelope.worker_id.clone(),
        operation,
        from_state,
        to_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskEnvelope, TenantScope,
        WorkerLocalePolicy,
    };

    fn envelope_in(state: TaskLifecycleState) -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            state,
            scope: TenantScope {
                tenant: "tenant-a".to_string(),
                team: Some("team-a".to_string()),
            },
            locale: LocaleContext {
                worker_default_locale: "en-US".to_string(),
                requested_locale: Some("fr-FR".to_string()),
                human_locale: None,
                policy: WorkerLocalePolicy::PreferRequested,
                propagation: LocalePropagation::PropagateToDelegates,
                output: OutputLocaleGuidance::MatchRequested,
            },
        }
    }

    #[test]
    fn applies_legal_transition() {
        let mut env = envelope_in(TaskLifecycleState::Created);
        let event =
            apply_operation(&mut env, RuntimeOperation::Start).expect("start should be legal");

        assert_eq!(event.from_state, TaskLifecycleState::Created);
        assert_eq!(event.to_state, TaskLifecycleState::Running);
        assert_eq!(env.state, TaskLifecycleState::Running);
    }

    #[test]
    fn step_keeps_running_state() {
        let mut env = envelope_in(TaskLifecycleState::Running);
        let event =
            apply_operation(&mut env, RuntimeOperation::Step).expect("step should be legal");

        assert_eq!(event.from_state, TaskLifecycleState::Running);
        assert_eq!(event.to_state, TaskLifecycleState::Running);
    }

    #[test]
    fn rejects_illegal_transition() {
        let mut env = envelope_in(TaskLifecycleState::Created);
        let err = apply_operation(&mut env, RuntimeOperation::Complete)
            .expect_err("complete from created should fail");

        assert_eq!(
            err,
            CoreRuntimeError::IllegalTransition {
                operation: "complete".to_string(),
                from: TaskLifecycleState::Created,
                to: TaskLifecycleState::Completed,
            }
        );
    }
}
