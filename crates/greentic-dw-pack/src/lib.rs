//! Hook/sub integration surfaces for Digital Worker runtime.

use greentic_dw_core::{RuntimeEvent, RuntimeOperation};
use greentic_dw_types::TaskEnvelope;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    Continue,
    Block { reason: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HookError {
    #[error("operation blocked by control hook: {reason}")]
    Blocked { reason: String },
}

/// Control hook trait for policy enforcement around runtime operations.
pub trait ControlHook: Send + Sync {
    fn pre_operation(&self, envelope: &TaskEnvelope, operation: &RuntimeOperation) -> HookDecision;
    fn post_operation(&self, envelope: &TaskEnvelope, event: &RuntimeEvent);
}

/// Observer subscription trait for audit/telemetry style notifications.
pub trait ObserverSub: Send + Sync {
    fn on_operation(&self, event: &RuntimeEvent);
}

/// Integration registry for hooks and observers.
#[derive(Default)]
pub struct PackIntegration {
    control_hooks: Vec<Box<dyn ControlHook>>,
    observer_subs: Vec<Box<dyn ObserverSub>>,
}

impl PackIntegration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_control_hook(mut self, hook: impl ControlHook + 'static) -> Self {
        self.control_hooks.push(Box::new(hook));
        self
    }

    pub fn with_observer_sub(mut self, sub: impl ObserverSub + 'static) -> Self {
        self.observer_subs.push(Box::new(sub));
        self
    }

    pub fn run_pre_hooks(
        &self,
        envelope: &TaskEnvelope,
        operation: &RuntimeOperation,
    ) -> Result<(), HookError> {
        for hook in &self.control_hooks {
            match hook.pre_operation(envelope, operation) {
                HookDecision::Continue => {}
                HookDecision::Block { reason } => {
                    return Err(HookError::Blocked { reason });
                }
            }
        }

        Ok(())
    }

    pub fn run_post_hooks(&self, envelope: &TaskEnvelope, event: &RuntimeEvent) {
        for hook in &self.control_hooks {
            hook.post_operation(envelope, event);
        }
    }

    pub fn notify_observers(&self, event: &RuntimeEvent) {
        for observer in &self.observer_subs {
            observer.on_operation(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_core::RuntimeOperation;
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskLifecycleState, TenantScope,
        WorkerLocalePolicy,
    };
    use std::sync::{Arc, Mutex};

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

    struct BlockStartHook;

    impl ControlHook for BlockStartHook {
        fn pre_operation(
            &self,
            _envelope: &TaskEnvelope,
            operation: &RuntimeOperation,
        ) -> HookDecision {
            if matches!(operation, RuntimeOperation::Start) {
                HookDecision::Block {
                    reason: "start disabled by policy".to_string(),
                }
            } else {
                HookDecision::Continue
            }
        }

        fn post_operation(&self, _envelope: &TaskEnvelope, _event: &RuntimeEvent) {}
    }

    struct RecordingObserver {
        count: Arc<Mutex<u32>>,
    }

    impl ObserverSub for RecordingObserver {
        fn on_operation(&self, _event: &RuntimeEvent) {
            let mut count = self.count.lock().expect("lock count");
            *count += 1;
        }
    }

    #[test]
    fn pre_hook_can_block_operation() {
        let integration = PackIntegration::new().with_control_hook(BlockStartHook);
        let env = sample_envelope();

        let err = integration
            .run_pre_hooks(&env, &RuntimeOperation::Start)
            .expect_err("start should be blocked");

        assert_eq!(
            err,
            HookError::Blocked {
                reason: "start disabled by policy".to_string(),
            }
        );
    }

    #[test]
    fn observer_receives_event() {
        let count = Arc::new(Mutex::new(0));
        let integration = PackIntegration::new().with_observer_sub(RecordingObserver {
            count: Arc::clone(&count),
        });

        let event = RuntimeEvent {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            operation: RuntimeOperation::Start,
            from_state: TaskLifecycleState::Created,
            to_state: TaskLifecycleState::Running,
        };

        integration.notify_observers(&event);

        assert_eq!(*count.lock().expect("lock count"), 1);
    }
}
