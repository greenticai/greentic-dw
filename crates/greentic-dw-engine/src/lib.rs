//! Engine abstraction for Digital Worker runtime decisions.
//!
//! Engines decide *what* should happen next, while runtime remains responsible
//! for applying operations and mediating side effects.

use greentic_dw_core::RuntimeOperation;
use greentic_dw_types::TaskEnvelope;
use thiserror::Error;

/// Context provided to the engine when requesting the next decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineContext {
    pub envelope: TaskEnvelope,
}

/// Structured decisions returned by engines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineDecision {
    /// No state change requested.
    Noop,
    /// Apply a single runtime operation.
    Operation(RuntimeOperation),
    /// Apply multiple operations in order.
    Batch(Vec<RuntimeOperation>),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EngineError {
    #[error("engine rejected empty operation batch")]
    EmptyBatch,
    #[error("engine returned invalid decision: {0}")]
    InvalidDecision(String),
}

/// Decision interface for runtime engine implementations.
pub trait DwEngine {
    fn decide(&self, context: &EngineContext) -> Result<EngineDecision, EngineError>;

    /// Fast path that lets runtimes avoid cloning envelopes when asking
    /// for an engine decision.
    fn decide_with_envelope(&self, envelope: &TaskEnvelope) -> Result<EngineDecision, EngineError> {
        self.decide(&EngineContext {
            envelope: envelope.clone(),
        })
    }
}

/// Basic engine implementation useful for tests and deterministic workflows.
#[derive(Debug, Clone)]
pub struct StaticEngine {
    decision: EngineDecision,
}

impl StaticEngine {
    pub fn new(decision: EngineDecision) -> Self {
        Self { decision }
    }
}

impl DwEngine for StaticEngine {
    fn decide(&self, _context: &EngineContext) -> Result<EngineDecision, EngineError> {
        Self::validate_decision(&self.decision)?;
        Ok(self.decision.clone())
    }

    fn decide_with_envelope(
        &self,
        _envelope: &TaskEnvelope,
    ) -> Result<EngineDecision, EngineError> {
        Self::validate_decision(&self.decision)?;
        Ok(self.decision.clone())
    }
}

impl StaticEngine {
    fn validate_decision(decision: &EngineDecision) -> Result<(), EngineError> {
        if let EngineDecision::Batch(ops) = decision
            && ops.is_empty()
        {
            return Err(EngineError::EmptyBatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskLifecycleState, TenantScope,
        WorkerLocalePolicy,
    };

    fn sample_envelope() -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            state: TaskLifecycleState::Created,
            scope: TenantScope {
                tenant: "tenant-a".to_string(),
                team: None,
            },
            locale: LocaleContext {
                worker_default_locale: "en-US".to_string(),
                requested_locale: None,
                human_locale: None,
                policy: WorkerLocalePolicy::WorkerDefault,
                propagation: LocalePropagation::CurrentTaskOnly,
                output: OutputLocaleGuidance::WorkerDefault,
            },
        }
    }

    #[test]
    fn static_engine_returns_configured_operation() {
        let engine = StaticEngine::new(EngineDecision::Operation(RuntimeOperation::Start));
        let context = EngineContext {
            envelope: sample_envelope(),
        };

        let decision = engine.decide(&context).expect("decision should succeed");
        assert_eq!(decision, EngineDecision::Operation(RuntimeOperation::Start));
    }

    #[test]
    fn static_engine_rejects_empty_batch() {
        let engine = StaticEngine::new(EngineDecision::Batch(vec![]));
        let context = EngineContext {
            envelope: sample_envelope(),
        };

        let err = engine
            .decide(&context)
            .expect_err("empty batch should be rejected");
        assert_eq!(err, EngineError::EmptyBatch);
    }
}
