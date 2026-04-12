use crate::{TaskId, TeamId, TenantId, WorkerId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    WorkerDefault,
    PreferRequested,
    PreferHuman,
    StrictRequested,
}

/// Locale propagation policy across delegated/child operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalePropagation {
    CurrentTaskOnly,
    PropagateToDelegates,
}

/// Guidance for response/output locale behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputLocaleGuidance {
    WorkerDefault,
    MatchRequested,
    MatchHuman,
    Explicit(String),
}

/// Locale context carried with a task envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocaleContext {
    pub worker_default_locale: String,
    pub requested_locale: Option<String>,
    pub human_locale: Option<String>,
    pub policy: WorkerLocalePolicy,
    pub propagation: LocalePropagation,
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
    pub tenant: TenantId,
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
