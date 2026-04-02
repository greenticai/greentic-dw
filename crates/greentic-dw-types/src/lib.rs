//! Canonical Digital Worker (DW) core contracts.
//!
//! This crate defines shared types for task envelopes, lifecycle states,
//! locale handling, and tenant/team scope.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Unique task identifier.
pub type TaskId = String;

/// Unique worker identifier.
pub type WorkerId = String;

/// Tenant identifier.
pub type TenantId = String;

/// Team identifier.
pub type TeamId = String;

/// Supported lifecycle states for a Digital Worker task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskLifecycleState {
    Created,
    Running,
    Waiting,
    Delegated,
    Completed,
    Failed,
    Cancelled,
}

impl TaskLifecycleState {
    /// Returns true when `next` is a legal transition from the current state.
    pub fn can_transition_to(self, next: Self) -> bool {
        use TaskLifecycleState::*;

        matches!(
            (self, next),
            (Created, Running)
                | (Created, Cancelled)
                | (Running, Waiting)
                | (Running, Delegated)
                | (Running, Completed)
                | (Running, Failed)
                | (Running, Cancelled)
                | (Waiting, Running)
                | (Waiting, Failed)
                | (Waiting, Cancelled)
                | (Delegated, Running)
                | (Delegated, Failed)
                | (Delegated, Cancelled)
        )
    }

    /// Returns true when the state cannot transition further.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskLifecycleState::Completed
                | TaskLifecycleState::Failed
                | TaskLifecycleState::Cancelled
        )
    }
}

/// Locale behavior policy for a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkerLocalePolicy {
    /// Always use the configured worker default locale.
    WorkerDefault,
    /// Prefer task requested locale when provided, otherwise default.
    PreferRequested,
    /// Prefer human locale when provided, otherwise fallback to requested/default.
    PreferHuman,
    /// Strictly require requested locale to be present and used.
    StrictRequested,
}

/// Locale propagation policy across delegated/child operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalePropagation {
    /// Keep locale only on the current task execution.
    CurrentTaskOnly,
    /// Propagate locale context to child/delegated operations.
    PropagateToDelegates,
}

/// Guidance for response/output locale behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputLocaleGuidance {
    /// Use worker default locale.
    WorkerDefault,
    /// Match task requested locale when available.
    MatchRequested,
    /// Match human locale when available.
    MatchHuman,
    /// Force explicit locale value.
    Explicit(String),
}

/// Locale context carried with a task envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocaleContext {
    /// Default locale configured for the worker.
    pub worker_default_locale: String,
    /// Optional locale requested by caller.
    pub requested_locale: Option<String>,
    /// Optional locale associated with the end user.
    pub human_locale: Option<String>,
    /// Locale policy for this worker.
    pub policy: WorkerLocalePolicy,
    /// Policy controlling locale propagation.
    pub propagation: LocalePropagation,
    /// Guidance for response locale.
    pub output: OutputLocaleGuidance,
}

impl LocaleContext {
    /// Resolve the effective locale according to configured policy.
    pub fn resolve_effective_locale(&self) -> Option<&str> {
        match self.policy {
            WorkerLocalePolicy::WorkerDefault => Some(self.worker_default_locale.as_str()),
            WorkerLocalePolicy::PreferRequested => self
                .requested_locale
                .as_deref()
                .or(Some(self.worker_default_locale.as_str())),
            WorkerLocalePolicy::PreferHuman => self
                .human_locale
                .as_deref()
                .or(self.requested_locale.as_deref())
                .or(Some(self.worker_default_locale.as_str())),
            WorkerLocalePolicy::StrictRequested => self.requested_locale.as_deref(),
        }
    }
}

/// Tenant/team scope for a task or worker execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TenantScope {
    /// Required tenant scope.
    pub tenant: TenantId,
    /// Optional team scope.
    pub team: Option<TeamId>,
}

/// Canonical task envelope propagated across runtime/engine operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskEnvelope {
    pub task_id: TaskId,
    pub worker_id: WorkerId,
    pub state: TaskLifecycleState,
    pub scope: TenantScope,
    pub locale: LocaleContext,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_locale(policy: WorkerLocalePolicy) -> LocaleContext {
        LocaleContext {
            worker_default_locale: "en-US".to_string(),
            requested_locale: Some("fr-FR".to_string()),
            human_locale: Some("nl-NL".to_string()),
            policy,
            propagation: LocalePropagation::PropagateToDelegates,
            output: OutputLocaleGuidance::MatchRequested,
        }
    }

    #[test]
    fn lifecycle_enforces_terminal_behavior() {
        assert!(TaskLifecycleState::Created.can_transition_to(TaskLifecycleState::Running));
        assert!(!TaskLifecycleState::Completed.can_transition_to(TaskLifecycleState::Running));
        assert!(TaskLifecycleState::Completed.is_terminal());
    }

    #[test]
    fn locale_resolution_prefers_human_when_configured() {
        let ctx = sample_locale(WorkerLocalePolicy::PreferHuman);
        assert_eq!(ctx.resolve_effective_locale(), Some("nl-NL"));
    }

    #[test]
    fn strict_requested_requires_requested_locale() {
        let mut ctx = sample_locale(WorkerLocalePolicy::StrictRequested);
        ctx.requested_locale = None;
        assert_eq!(ctx.resolve_effective_locale(), None);
    }
}
