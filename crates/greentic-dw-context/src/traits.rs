use crate::{
    BuildContextRequest, CompressContextRequest, CompressedContext, ContextError, ContextPackage,
    SummarizeContextRequest, SummaryArtifactRef,
};

pub trait ContextProvider: Send + Sync {
    fn build_context(&self, req: BuildContextRequest) -> Result<ContextPackage, ContextError>;
    fn compress_context(
        &self,
        req: CompressContextRequest,
    ) -> Result<CompressedContext, ContextError>;
    fn summarize_context(
        &self,
        req: SummarizeContextRequest,
    ) -> Result<SummaryArtifactRef, ContextError>;
}
