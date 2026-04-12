#[cfg(test)]
mod tests {
    use crate::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskLifecycleState,
        WorkerLocalePolicy,
    };

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
