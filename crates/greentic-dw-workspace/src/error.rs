use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("provider error: {0}")]
    Provider(String),
}
