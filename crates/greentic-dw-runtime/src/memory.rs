use greentic_dw_types::TaskEnvelope;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

use crate::CapabilityDispatchError;

/// Logical operation used when dispatching memory capability calls.
pub const MEMORY_GET_OPERATION: &str = "get";
/// Logical operation used when dispatching memory capability calls.
pub const MEMORY_PUT_OPERATION: &str = "put";
/// Logical operation used when dispatching task-state capability calls.
pub const STATE_LOAD_OPERATION: &str = "load";
/// Logical operation used when dispatching task-state capability calls.
pub const STATE_SAVE_OPERATION: &str = "save";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Task,
    Worker,
    Tenant,
}

/// Portable memory record exchanged between runtime and memory provider packs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub scope: MemoryScope,
    pub subject: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub scope: MemoryScope,
    pub subject: String,
    pub key: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryProviderError {
    #[error("memory backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryPolicyError {
    #[error("memory access denied: {0}")]
    Denied(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error(transparent)]
    Provider(#[from] MemoryProviderError),
    #[error(transparent)]
    Policy(#[from] MemoryPolicyError),
    #[error("memory extension is not configured")]
    NotConfigured,
    #[error(transparent)]
    Dispatch(#[from] CapabilityDispatchError),
}

/// Backend interface for memory packs. Runtime does not assume any concrete storage.
pub trait MemoryProvider: Send + Sync {
    fn put(&self, record: MemoryRecord) -> Result<(), MemoryProviderError>;
    fn get(&self, query: &MemoryQuery) -> Result<Option<MemoryRecord>, MemoryProviderError>;
}

/// Policy interface for controlling memory read/write behavior.
pub trait MemoryPolicy: Send + Sync {
    fn allow_write(
        &self,
        envelope: &TaskEnvelope,
        record: &MemoryRecord,
    ) -> Result<(), MemoryPolicyError>;

    fn allow_read(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<(), MemoryPolicyError>;
}

/// Default permissive memory policy; useful for integration and local tests.
#[derive(Default)]
pub struct AllowAllMemoryPolicy;

impl MemoryPolicy for AllowAllMemoryPolicy {
    fn allow_write(
        &self,
        _envelope: &TaskEnvelope,
        _record: &MemoryRecord,
    ) -> Result<(), MemoryPolicyError> {
        Ok(())
    }

    fn allow_read(
        &self,
        _envelope: &TaskEnvelope,
        _query: &MemoryQuery,
    ) -> Result<(), MemoryPolicyError> {
        Ok(())
    }
}

/// Runtime memory extension entrypoint.
#[derive(Clone)]
pub struct MemoryExtension {
    provider: Arc<dyn MemoryProvider>,
    policy: Arc<dyn MemoryPolicy>,
}

impl MemoryExtension {
    pub fn new(provider: Arc<dyn MemoryProvider>, policy: Arc<dyn MemoryPolicy>) -> Self {
        Self { provider, policy }
    }

    pub fn remember(
        &self,
        envelope: &TaskEnvelope,
        record: MemoryRecord,
    ) -> Result<(), MemoryError> {
        self.policy.allow_write(envelope, &record)?;
        self.provider.put(record)?;
        Ok(())
    }

    pub fn recall(
        &self,
        envelope: &TaskEnvelope,
        query: &MemoryQuery,
    ) -> Result<Option<MemoryRecord>, MemoryError> {
        self.policy.allow_read(envelope, query)?;
        self.provider.get(query).map_err(MemoryError::from)
    }
}
