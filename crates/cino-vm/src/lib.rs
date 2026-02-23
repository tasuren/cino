#![forbid(unsafe_code)]

use std::{collections::BTreeMap, fmt, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmValue {
    Unit,
    Int(i64),
    Bool(bool),
    String(String),
    List(Vec<VmValue>),
    Tuple(Vec<VmValue>),
    Map(BTreeMap<String, VmValue>),
}

pub type VmState = VmValue;
pub type VmAction = VmValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLimits {
    pub max_steps: u64,
    pub max_memory_bytes: usize,
    pub max_recursion_depth: u32,
}

impl Default for VmLimits {
    fn default() -> Self {
        Self {
            max_steps: 100_000,
            max_memory_bytes: 64 * 1024 * 1024,
            max_recursion_depth: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmErrorCode {
    StepLimitExceeded,
    MemoryLimitExceeded,
    RecursionLimitExceeded,
    InvalidInput,
    Trap,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmError {
    pub code: VmErrorCode,
    pub message: String,
}

impl VmError {
    pub fn new(code: VmErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for VmError {}

pub trait VmProgram: Send + Sync {
    fn update(
        &self,
        state: &VmState,
        event: &VmValue,
        limits: &VmLimits,
    ) -> Result<(VmState, Vec<VmAction>), VmError>;

    fn query(
        &self,
        state: &VmState,
        query: &VmValue,
        limits: &VmLimits,
    ) -> Result<VmValue, VmError>;
}

pub type UpdateFn = dyn Fn(&VmState, &VmValue, &VmLimits) -> Result<(VmState, Vec<VmAction>), VmError>
    + Send
    + Sync;
pub type QueryFn = dyn Fn(&VmState, &VmValue, &VmLimits) -> Result<VmValue, VmError> + Send + Sync;

pub struct NativeProgram {
    update_fn: Arc<UpdateFn>,
    query_fn: Arc<QueryFn>,
}

impl NativeProgram {
    pub fn new(update_fn: Arc<UpdateFn>, query_fn: Arc<QueryFn>) -> Self {
        Self {
            update_fn,
            query_fn,
        }
    }
}

impl VmProgram for NativeProgram {
    fn update(
        &self,
        state: &VmState,
        event: &VmValue,
        limits: &VmLimits,
    ) -> Result<(VmState, Vec<VmAction>), VmError> {
        (self.update_fn)(state, event, limits)
    }

    fn query(
        &self,
        state: &VmState,
        query: &VmValue,
        limits: &VmLimits,
    ) -> Result<VmValue, VmError> {
        (self.query_fn)(state, query, limits)
    }
}

/// VM crate entry point for the cino MVP workspace.
pub fn crate_name() -> &'static str {
    "cino-vm"
}
