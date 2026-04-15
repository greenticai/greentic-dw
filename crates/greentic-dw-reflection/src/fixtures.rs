use crate::{
    ReviewFinding, ReviewOutcome, ReviewTarget, ReviewTargetKind, ReviewVerdict, SuggestedAction,
};

pub fn review_outcome_fixture() -> ReviewOutcome {
    ReviewOutcome {
        verdict: ReviewVerdict::Revise,
        score: Some(0.5),
        findings: vec![ReviewFinding {
            code: "missing_evidence".to_string(),
            message: "Need one more supporting artifact".to_string(),
            target: ReviewTarget {
                kind: ReviewTargetKind::Artifact,
                reference: "artifact://report-1".to_string(),
            },
        }],
        suggested_actions: vec![SuggestedAction {
            action: "collect_more_evidence".to_string(),
            target: ReviewTarget {
                kind: ReviewTargetKind::PlanStep,
                reference: "step-2".to_string(),
            },
        }],
        binding: true,
    }
}
