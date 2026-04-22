use crate::{
    DelegationDecision, DelegationError, DelegationHandle, DelegationMergeResult,
    DelegationRequest, MergeSubtaskResultRequest, StartSubtaskRequest,
};

pub trait DelegationProvider: Send + Sync {
    fn choose_delegate(
        &self,
        req: DelegationRequest,
    ) -> Result<DelegationDecision, DelegationError>;
    fn start_subtask(&self, req: StartSubtaskRequest) -> Result<DelegationHandle, DelegationError>;
    fn merge_result(
        &self,
        req: MergeSubtaskResultRequest,
    ) -> Result<DelegationMergeResult, DelegationError>;
}
