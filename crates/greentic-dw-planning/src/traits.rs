use crate::{
    CompletionCheckRequest, CompletionState, CreatePlanRequest, NextActionsRequest, PlanDocument,
    PlanRevision, PlannedAction, PlanningError, RevisePlanRequest, StepResultRequest,
};

pub trait PlanningProvider: Send + Sync {
    fn create_plan(&self, req: CreatePlanRequest) -> Result<PlanDocument, PlanningError>;
    fn revise_plan(&self, req: RevisePlanRequest) -> Result<PlanRevision, PlanningError>;
    fn next_actions(&self, req: NextActionsRequest) -> Result<Vec<PlannedAction>, PlanningError>;
    fn record_step_result(&self, req: StepResultRequest) -> Result<PlanDocument, PlanningError>;
    fn evaluate_completion(
        &self,
        req: CompletionCheckRequest,
    ) -> Result<CompletionState, PlanningError>;
}
