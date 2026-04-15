use crate::{PlanDocument, PlanEdge, PlanStepStatus, PlanningError};
use std::collections::BTreeSet;

pub fn validate_plan(plan: &PlanDocument) -> Result<(), PlanningError> {
    if plan.plan_id.trim().is_empty() {
        return Err(PlanningError::Validation(
            "plan_id must not be empty".to_string(),
        ));
    }
    if plan.goal.trim().is_empty() {
        return Err(PlanningError::Validation(
            "goal must not be empty".to_string(),
        ));
    }
    if plan.success_criteria.is_empty() {
        return Err(PlanningError::Validation(
            "success_criteria must not be empty".to_string(),
        ));
    }

    let mut step_ids = BTreeSet::new();
    for step in &plan.steps {
        if step.step_id.trim().is_empty() {
            return Err(PlanningError::Validation(
                "step_id must not be empty".to_string(),
            ));
        }
        if !step_ids.insert(step.step_id.clone()) {
            return Err(PlanningError::Validation(format!(
                "duplicate step_id `{}`",
                step.step_id
            )));
        }
        if step.title.trim().is_empty() {
            return Err(PlanningError::Validation(format!(
                "step `{}` title must not be empty",
                step.step_id
            )));
        }
        for dependency in &step.depends_on {
            if dependency.trim().is_empty() {
                return Err(PlanningError::Validation(format!(
                    "step `{}` has an empty dependency",
                    step.step_id
                )));
            }
        }
    }

    for edge in &plan.edges {
        validate_edge(edge, &step_ids)?;
    }

    for step in &plan.steps {
        for dependency in &step.depends_on {
            if !step_ids.contains(dependency) {
                return Err(PlanningError::Validation(format!(
                    "step `{}` depends on unknown step `{dependency}`",
                    step.step_id
                )));
            }
            if dependency == &step.step_id {
                return Err(PlanningError::Validation(format!(
                    "step `{}` cannot depend on itself",
                    step.step_id
                )));
            }
        }
    }

    Ok(())
}

fn validate_edge(edge: &PlanEdge, step_ids: &BTreeSet<String>) -> Result<(), PlanningError> {
    if !step_ids.contains(&edge.from_step_id) {
        return Err(PlanningError::Validation(format!(
            "edge references unknown from_step_id `{}`",
            edge.from_step_id
        )));
    }
    if !step_ids.contains(&edge.to_step_id) {
        return Err(PlanningError::Validation(format!(
            "edge references unknown to_step_id `{}`",
            edge.to_step_id
        )));
    }
    if edge.from_step_id == edge.to_step_id {
        return Err(PlanningError::Validation(
            "edge cannot connect a step to itself".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_step_transition(
    previous: &PlanStepStatus,
    next: &PlanStepStatus,
) -> Result<(), PlanningError> {
    let allowed = matches!(
        (previous, next),
        (PlanStepStatus::Pending, PlanStepStatus::Ready)
            | (PlanStepStatus::Ready, PlanStepStatus::Running)
            | (PlanStepStatus::Running, PlanStepStatus::Completed)
            | (PlanStepStatus::Running, PlanStepStatus::Failed)
            | (PlanStepStatus::Running, PlanStepStatus::Blocked)
            | (PlanStepStatus::Blocked, PlanStepStatus::Ready)
            | (PlanStepStatus::Pending, PlanStepStatus::Skipped)
            | (PlanStepStatus::Ready, PlanStepStatus::Skipped)
            | (PlanStepStatus::Running, PlanStepStatus::Running)
            | (PlanStepStatus::Completed, PlanStepStatus::Completed)
            | (PlanStepStatus::Failed, PlanStepStatus::Failed)
            | (PlanStepStatus::Skipped, PlanStepStatus::Skipped)
    );

    if allowed {
        Ok(())
    } else {
        Err(PlanningError::Validation(format!(
            "illegal plan step transition from `{previous:?}` to `{next:?}`"
        )))
    }
}
