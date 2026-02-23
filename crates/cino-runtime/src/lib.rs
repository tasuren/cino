#![forbid(unsafe_code)]

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};

use cino_vm::{VmAction, VmError, VmErrorCode, VmLimits, VmProgram, VmState, VmValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub limits: VmLimits,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            limits: VmLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorCode {
    StepLimitExceeded,
    MemoryLimitExceeded,
    RecursionLimitExceeded,
    InvalidInput,
    Trap,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: RuntimeErrorCode,
    pub message: String,
}

impl RuntimeError {
    fn from_vm_error(error: VmError) -> Self {
        Self {
            code: map_error_code(error.code),
            message: error.message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHandle {
    inner: VmState,
}

impl StateHandle {
    pub fn from_value(value: VmState) -> Self {
        Self { inner: value }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateResult {
    pub state: StateHandle,
    pub actions: Vec<VmAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub result: VmValue,
}

pub struct Runtime {
    program: Arc<dyn VmProgram>,
    config: RuntimeConfig,
}

impl Runtime {
    pub fn new(program: Arc<dyn VmProgram>) -> Self {
        Self {
            program,
            config: RuntimeConfig::default(),
        }
    }

    pub fn with_config(program: Arc<dyn VmProgram>, config: RuntimeConfig) -> Self {
        Self { program, config }
    }

    pub fn update(
        &self,
        state: &StateHandle,
        event: &VmValue,
    ) -> Result<UpdateResult, RuntimeError> {
        // 呼び出し単位の一時データはこのスコープ内でのみ保持する。
        let (next_state, actions) = run_vm_call(|| {
            self.program
                .update(&state.inner, event, &self.config.limits)
        })?;
        Ok(UpdateResult {
            state: StateHandle::from_value(next_state),
            actions,
        })
    }

    pub fn query(&self, state: &StateHandle, query: &VmValue) -> Result<QueryResult, RuntimeError> {
        // 呼び出し単位の一時データはこのスコープ内でのみ保持する。
        let result = run_vm_call(|| self.program.query(&state.inner, query, &self.config.limits))?;
        Ok(QueryResult { result })
    }
}

fn run_vm_call<T, F>(call: F) -> Result<T, RuntimeError>
where
    F: FnOnce() -> Result<T, VmError>,
{
    match catch_unwind(AssertUnwindSafe(call)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(vm_error)) => Err(RuntimeError::from_vm_error(vm_error)),
        Err(_) => Err(RuntimeError {
            code: RuntimeErrorCode::Panic,
            message: "runtime converted panic to structured error".to_string(),
        }),
    }
}

fn map_error_code(code: VmErrorCode) -> RuntimeErrorCode {
    match code {
        VmErrorCode::StepLimitExceeded => RuntimeErrorCode::StepLimitExceeded,
        VmErrorCode::MemoryLimitExceeded => RuntimeErrorCode::MemoryLimitExceeded,
        VmErrorCode::RecursionLimitExceeded => RuntimeErrorCode::RecursionLimitExceeded,
        VmErrorCode::InvalidInput => RuntimeErrorCode::InvalidInput,
        VmErrorCode::Trap => RuntimeErrorCode::Trap,
        VmErrorCode::Panic => RuntimeErrorCode::Panic,
    }
}

/// Runtime crate entry point for the cino MVP workspace.
pub fn crate_name() -> &'static str {
    "cino-runtime"
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use cino_vm::{NativeProgram, VmErrorCode, VmValue};

    use crate::{Runtime, RuntimeErrorCode, StateHandle};

    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn sync_update_and_query_api_work() {
        let program = Arc::new(NativeProgram::new(
            Arc::new(|state, event, _limits| {
                let (VmValue::Int(current), VmValue::Int(delta)) = (state, event) else {
                    return Err(cino_vm::VmError::new(
                        VmErrorCode::InvalidInput,
                        "expected Int state/event",
                    ));
                };
                Ok((VmValue::Int(current + delta), vec![VmValue::Int(*delta)]))
            }),
            Arc::new(|state, query, _limits| {
                let VmValue::Bool(read_balance) = query else {
                    return Err(cino_vm::VmError::new(
                        VmErrorCode::InvalidInput,
                        "expected Bool query",
                    ));
                };
                if *read_balance {
                    Ok(state.clone())
                } else {
                    Ok(VmValue::Unit)
                }
            }),
        ));

        let runtime = Runtime::new(program);
        let state = StateHandle::from_value(VmValue::Int(10));

        let updated = runtime
            .update(&state, &VmValue::Int(7))
            .expect("update should succeed");
        assert_eq!(updated.state, StateHandle::from_value(VmValue::Int(17)));
        assert_eq!(updated.actions, vec![VmValue::Int(7)]);

        let queried = runtime
            .query(&updated.state, &VmValue::Bool(true))
            .expect("query should succeed");
        assert_eq!(queried.result, VmValue::Int(17));
    }

    #[test]
    fn runtime_converts_vm_error_without_panicking() {
        let program = Arc::new(NativeProgram::new(
            Arc::new(|_state, _event, _limits| {
                Err(cino_vm::VmError::new(
                    VmErrorCode::StepLimitExceeded,
                    "step budget exceeded",
                ))
            }),
            Arc::new(|_state, _query, _limits| Ok(VmValue::Unit)),
        ));

        let runtime = Runtime::new(program);
        let state = StateHandle::from_value(VmValue::Unit);

        let error = runtime
            .update(&state, &VmValue::Unit)
            .expect_err("update should fail");
        assert_eq!(error.code, RuntimeErrorCode::StepLimitExceeded);
        assert_eq!(error.message, "step budget exceeded");
    }

    #[test]
    fn runtime_does_not_keep_call_scoped_temporaries() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let dropped_in_update = Arc::clone(&dropped);
        let program = Arc::new(NativeProgram::new(
            Arc::new(move |state, _event, _limits| {
                let _temporary = DropCounter(Arc::clone(&dropped_in_update));
                Ok((state.clone(), vec![]))
            }),
            Arc::new(|state, _query, _limits| Ok(state.clone())),
        ));

        let runtime = Runtime::new(program);
        let state = StateHandle::from_value(VmValue::Unit);

        runtime
            .update(&state, &VmValue::Unit)
            .expect("update should succeed");
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn runtime_converts_panic_to_structured_error() {
        let program = Arc::new(NativeProgram::new(
            Arc::new(|_state, _event, _limits| panic!("unexpected panic")),
            Arc::new(|_state, _query, _limits| Ok(VmValue::Unit)),
        ));
        let runtime = Runtime::new(program);
        let state = StateHandle::from_value(VmValue::Unit);

        let error = runtime
            .update(&state, &VmValue::Unit)
            .expect_err("panic must be converted");
        assert_eq!(error.code, RuntimeErrorCode::Panic);
    }
}
