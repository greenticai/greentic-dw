use crate::{
    ReflectionError, ReviewFinalRequest, ReviewOutcome, ReviewPlanRequest, ReviewStepRequest,
};

pub trait ReflectionProvider: Send + Sync {
    fn review_step(&self, req: ReviewStepRequest) -> Result<ReviewOutcome, ReflectionError>;
    fn review_plan(&self, req: ReviewPlanRequest) -> Result<ReviewOutcome, ReflectionError>;
    fn review_final(&self, req: ReviewFinalRequest) -> Result<ReviewOutcome, ReflectionError>;
}
