//! Providers used when the designer switches reflection or delegation off.

use greentic_dw_delegation::{
    DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult,
    DelegationProvider, DelegationRequest, MergeSubtaskResultRequest, StartSubtaskRequest,
};
use greentic_dw_planning::{PlanDocument, PlanStepKind};
use greentic_dw_reflection::{
    ReflectionError, ReflectionProvider, ReviewFinalRequest, ReviewOutcome, ReviewPlanRequest,
    ReviewStepRequest, ReviewVerdict,
};

/// Planner instruction added to `CreatePlanRequest.constraints` when delegation
/// is off. The planning prompt serialises the whole request, so the model sees it.
pub(crate) const NO_DELEGATION_CONSTRAINT: &str = "Do not create steps of kind `delegate`: \
this worker has no other agents to hand work to, so every step must be carried out by the \
worker itself.";

const DELEGATION_OFF: &str = "delegation is switched off for this deep worker";

fn accept() -> ReviewOutcome {
    ReviewOutcome {
        verdict: ReviewVerdict::Accept,
        score: None,
        findings: vec![],
        suggested_actions: vec![],
        binding: false,
    }
}

/// Reflection switched off: every review accepts, with no LLM call.
pub(crate) struct AcceptAllReflection;

impl ReflectionProvider for AcceptAllReflection {
    fn review_step(&self, _req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
        Ok(accept())
    }

    fn review_plan(&self, _req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> {
        Ok(accept())
    }

    fn review_final(&self, _req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> {
        Ok(accept())
    }
}

/// Delegation switched off: the last line of defence. The invoker already told
/// the planner not to delegate and rewrote any delegate step, so reaching this
/// means a new path emitted one — fail loudly rather than hand work to nobody.
pub(crate) struct RefuseDelegation;

fn refusal() -> DelegationError {
    DelegationError::Validation(DELEGATION_OFF.to_string())
}

impl DelegationProvider for RefuseDelegation {
    fn choose_delegate(
        &self,
        _req: DelegationRequest,
    ) -> Result<DelegationDecision, DelegationError> {
        Err(refusal())
    }

    fn start_subtask(
        &self,
        _req: StartSubtaskRequest,
    ) -> Result<DelegationHandle, DelegationError> {
        Err(refusal())
    }

    fn merge_result(
        &self,
        _req: MergeSubtaskResultRequest,
    ) -> Result<DelegationMergeResult, DelegationError> {
        Err(refusal())
    }
}

/// Reflection ON with delegation OFF: an LLM reviewer may still answer
/// `Delegate`, which the deep loop turns into a delegation (`deep_loop.rs`
/// `ReviewVerdict::Delegate` arm) and [`RefuseDelegation`] would fail the turn.
/// Map that verdict to `Accept`, keeping the rest of the review.
pub(crate) struct NoDelegateVerdict<'a> {
    pub inner: &'a dyn ReflectionProvider,
}

fn without_delegate(mut outcome: ReviewOutcome) -> ReviewOutcome {
    if outcome.verdict == ReviewVerdict::Delegate {
        outcome.verdict = ReviewVerdict::Accept;
    }
    outcome
}

impl ReflectionProvider for NoDelegateVerdict<'_> {
    fn review_step(&self, req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
        self.inner.review_step(req).map(without_delegate)
    }

    fn review_plan(&self, req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> {
        self.inner.review_plan(req).map(without_delegate)
    }

    fn review_final(&self, req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> {
        self.inner.review_final(req).map(without_delegate)
    }
}

/// Rewrite any `Delegate` step the model emitted despite the constraint into a
/// step the worker runs itself. Steps only come from `create_plan` (a revision
/// carries a number, not steps), so doing this once after planning is enough.
pub(crate) fn strip_delegate_steps(plan: &mut PlanDocument) {
    for step in &mut plan.steps {
        if step.kind == PlanStepKind::Delegate {
            step.kind = PlanStepKind::Research;
            step.assigned_agent = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_dw_planning::{PlanDocument, PlanStatus, PlanStep, PlanStepStatus};
    use greentic_dw_reflection::{ReviewPlanRequest, ReviewTarget, ReviewTargetKind};
    use std::collections::BTreeMap;

    fn step_req() -> ReviewStepRequest {
        ReviewStepRequest {
            plan_step_id: "s1".into(),
            output_artifact_ref: "artifact://p/s1/0".into(),
            context: None,
        }
    }

    #[test]
    fn accept_all_reflection_accepts_every_review() {
        let reflector = AcceptAllReflection;
        let step = reflector.review_step(step_req()).expect("ok");
        let plan = reflector
            .review_plan(ReviewPlanRequest {
                plan_id: "p".into(),
                revision: 1,
            })
            .expect("ok");
        let fin = reflector
            .review_final(ReviewFinalRequest {
                run_id: "r".into(),
                output_artifact_ref: "artifact://p/final".into(),
                context: None,
            })
            .expect("ok");
        for outcome in [step, plan, fin] {
            assert_eq!(outcome.verdict, ReviewVerdict::Accept);
            outcome.validate().expect("an accept outcome must validate");
        }
    }

    #[test]
    fn refuse_delegation_refuses_every_call() {
        let delegator = RefuseDelegation;
        let err = delegator
            .choose_delegate(DelegationRequest {
                goal: "g".into(),
                candidate_agents: vec!["helper".into()],
            })
            .expect_err("delegation is off");
        assert!(err.to_string().contains("delegation is switched off"));
    }

    struct AlwaysDelegate;
    impl ReflectionProvider for AlwaysDelegate {
        fn review_step(&self, _req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError> {
            Ok(ReviewOutcome {
                verdict: ReviewVerdict::Delegate,
                score: Some(0.4),
                findings: vec![],
                suggested_actions: vec![greentic_dw_reflection::SuggestedAction {
                    action: "hand off".into(),
                    target: ReviewTarget {
                        kind: ReviewTargetKind::PlanStep,
                        reference: "s1".into(),
                    },
                }],
                binding: false,
            })
        }
        fn review_plan(&self, req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError> {
            self.review_step(ReviewStepRequest {
                plan_step_id: req.plan_id,
                output_artifact_ref: String::new(),
                context: None,
            })
        }
        fn review_final(&self, _req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError> {
            self.review_step(step_req())
        }
    }

    #[test]
    fn no_delegate_verdict_turns_a_delegate_verdict_into_accept() {
        let inner = AlwaysDelegate;
        let guarded = NoDelegateVerdict { inner: &inner };
        let outcome = guarded.review_step(step_req()).expect("ok");
        assert_eq!(outcome.verdict, ReviewVerdict::Accept);
        // Everything else the reviewer said is kept.
        assert_eq!(outcome.score, Some(0.4));
        assert_eq!(outcome.suggested_actions.len(), 1);
    }

    #[test]
    fn strip_delegate_steps_rewrites_only_delegate_steps() {
        let step = |id: &str, kind: PlanStepKind, agent: Option<&str>| PlanStep {
            step_id: id.into(),
            title: format!("Step {id}"),
            kind,
            status: PlanStepStatus::Ready,
            depends_on: vec![],
            assigned_agent: agent.map(str::to_string),
            inputs_schema_ref: None,
            output_schema_ref: None,
            retry_count: 0,
        };
        let mut plan = PlanDocument {
            plan_id: "p".into(),
            goal: "g".into(),
            status: PlanStatus::Active,
            revision: 1,
            assumptions: vec![],
            constraints: vec![],
            success_criteria: vec!["task completed".into()],
            steps: vec![
                step("s1", PlanStepKind::Delegate, Some("helper")),
                step("s2", PlanStepKind::ToolCall, None),
            ],
            edges: vec![],
            metadata: BTreeMap::new(),
        };
        strip_delegate_steps(&mut plan);
        assert_eq!(plan.steps[0].kind, PlanStepKind::Research);
        assert_eq!(plan.steps[0].assigned_agent, None);
        assert_eq!(plan.steps[1].kind, PlanStepKind::ToolCall);
    }
}
