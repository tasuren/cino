#![allow(non_camel_case_types)]

use std::{
    collections::BTreeMap,
    ffi::CString,
    os::raw::c_char,
    ptr,
    sync::Arc,
};

use cino_runtime::{Runtime, RuntimeError, RuntimeErrorCode, StateHandle as RuntimeStateHandle};
use cino_vm::{NativeProgram, VmError, VmErrorCode, VmLimits, VmProgram, VmValue};
use serde_cbor::Value as CborValue;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum cino_status_t {
    CINO_STATUS_OK = 0,
    CINO_STATUS_ERR = 1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum cino_error_code_t {
    CINO_ERROR_RUNTIME_STEP_LIMIT_EXCEEDED = 1,
    CINO_ERROR_RUNTIME_MEMORY_LIMIT_EXCEEDED = 2,
    CINO_ERROR_RUNTIME_RECURSION_LIMIT_EXCEEDED = 3,
    CINO_ERROR_RUNTIME_INVALID_INPUT = 4,
    CINO_ERROR_RUNTIME_TRAP = 5,
    CINO_ERROR_RUNTIME_PANIC = 6,
    CINO_ERROR_ABI_NULL_POINTER = 101,
    CINO_ERROR_ABI_INVALID_CBOR = 102,
    CINO_ERROR_ABI_INVALID_HANDLE = 103,
    CINO_ERROR_ABI_INTERNAL = 199,
}

pub struct cino_program_t {
    runtime: Runtime,
}

pub struct cino_state_t {
    inner: RuntimeStateHandle,
}

pub struct cino_value_t {
    bytes: Vec<u8>,
}

pub struct cino_actions_t {
    bytes: Vec<u8>,
}

pub struct cino_error_t {
    code: cino_error_code_t,
    message: CString,
}

#[derive(Debug)]
struct FfiError {
    code: cino_error_code_t,
    message: String,
}

impl FfiError {
    fn new(code: cino_error_code_t, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn null_pointer(name: &str) -> Self {
        Self::new(
            cino_error_code_t::CINO_ERROR_ABI_NULL_POINTER,
            format!("null pointer: {name}"),
        )
    }
}

fn make_c_error(error: FfiError) -> *mut cino_error_t {
    let message = CString::new(error.message).unwrap_or_else(|_| {
        CString::new("ffi error message contained interior NUL").expect("static string is valid")
    });
    Box::into_raw(Box::new(cino_error_t {
        code: error.code,
        message,
    }))
}

fn map_runtime_error(error: RuntimeError) -> FfiError {
    let code = match error.code {
        RuntimeErrorCode::StepLimitExceeded => {
            cino_error_code_t::CINO_ERROR_RUNTIME_STEP_LIMIT_EXCEEDED
        }
        RuntimeErrorCode::MemoryLimitExceeded => {
            cino_error_code_t::CINO_ERROR_RUNTIME_MEMORY_LIMIT_EXCEEDED
        }
        RuntimeErrorCode::RecursionLimitExceeded => {
            cino_error_code_t::CINO_ERROR_RUNTIME_RECURSION_LIMIT_EXCEEDED
        }
        RuntimeErrorCode::InvalidInput => cino_error_code_t::CINO_ERROR_RUNTIME_INVALID_INPUT,
        RuntimeErrorCode::Trap => cino_error_code_t::CINO_ERROR_RUNTIME_TRAP,
        RuntimeErrorCode::Panic => cino_error_code_t::CINO_ERROR_RUNTIME_PANIC,
    };
    FfiError::new(code, error.message)
}

fn decode_vm_value(bytes: &[u8]) -> Result<VmValue, FfiError> {
    let cbor = serde_cbor::from_slice::<CborValue>(bytes).map_err(|err| {
        FfiError::new(
            cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
            format!("failed to decode CBOR: {err}"),
        )
    })?;
    vm_value_from_cbor(&cbor)
}

fn encode_vm_value(value: &VmValue) -> Result<Vec<u8>, FfiError> {
    serde_cbor::to_vec(&cbor_from_vm_value(value)).map_err(|err| {
        FfiError::new(
            cino_error_code_t::CINO_ERROR_ABI_INTERNAL,
            format!("failed to encode CBOR: {err}"),
        )
    })
}

fn vm_value_from_cbor(value: &CborValue) -> Result<VmValue, FfiError> {
    match value {
        CborValue::Null => Ok(VmValue::Unit),
        CborValue::Bool(v) => Ok(VmValue::Bool(*v)),
        CborValue::Integer(v) => i64::try_from(*v)
            .map(VmValue::Int)
            .map_err(|_| {
                FfiError::new(
                    cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                    "integer does not fit in i64",
                )
            }),
        CborValue::Text(s) => Ok(VmValue::String(s.clone())),
        CborValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(vm_value_from_cbor(item)?);
            }
            Ok(VmValue::List(out))
        }
        CborValue::Map(entries) => parse_vm_map(entries),
        _ => Err(FfiError::new(
            cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
            "unsupported CBOR type for VmValue",
        )),
    }
}

fn parse_vm_map(entries: &BTreeMap<CborValue, CborValue>) -> Result<VmValue, FfiError> {
    if entries.len() == 1 {
        if let Some(tuple_value) = entries.get(&CborValue::Text("$tuple".to_string())) {
            let CborValue::Array(items) = tuple_value else {
                return Err(FfiError::new(
                    cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                    "`$tuple` must be an array",
                ));
            };
            let mut tuple = Vec::with_capacity(items.len());
            for item in items {
                tuple.push(vm_value_from_cbor(item)?);
            }
            return Ok(VmValue::Tuple(tuple));
        }
    }

    if entries.contains_key(&CborValue::Text("$tag".to_string())) {
        let tag = entries
            .get(&CborValue::Text("$tag".to_string()))
            .ok_or_else(|| {
                FfiError::new(
                    cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                    "enum map is missing `$tag`",
                )
            })?;
        let fields = entries
            .get(&CborValue::Text("$fields".to_string()))
            .ok_or_else(|| {
                FfiError::new(
                    cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                    "enum map is missing `$fields`",
                )
            })?;

        let CborValue::Text(tag) = tag else {
            return Err(FfiError::new(
                cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                "`$tag` must be text",
            ));
        };
        let CborValue::Map(raw_fields) = fields else {
            return Err(FfiError::new(
                cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                "`$fields` must be a map",
            ));
        };

        let mut mapped = BTreeMap::new();
        for (key, value) in raw_fields {
            let CborValue::Text(field_name) = key else {
                return Err(FfiError::new(
                    cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                    "enum field key must be text",
                ));
            };
            mapped.insert(field_name.clone(), vm_value_from_cbor(value)?);
        }

        return Ok(VmValue::Enum {
            tag: tag.clone(),
            fields: mapped,
        });
    }

    let mut map = BTreeMap::new();
    for (key, value) in entries {
        let CborValue::Text(k) = key else {
            return Err(FfiError::new(
                cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR,
                "map key must be text",
            ));
        };
        map.insert(k.clone(), vm_value_from_cbor(value)?);
    }
    Ok(VmValue::Map(map))
}

fn cbor_from_vm_value(value: &VmValue) -> CborValue {
    match value {
        VmValue::Unit => CborValue::Null,
        VmValue::Int(v) => CborValue::Integer((*v).into()),
        VmValue::Bool(v) => CborValue::Bool(*v),
        VmValue::String(v) => CborValue::Text(v.clone()),
        VmValue::List(items) => {
            CborValue::Array(items.iter().map(cbor_from_vm_value).collect())
        }
        VmValue::Tuple(items) => {
            let mut map = BTreeMap::new();
            map.insert(
                CborValue::Text("$tuple".to_string()),
                CborValue::Array(items.iter().map(cbor_from_vm_value).collect()),
            );
            CborValue::Map(map)
        }
        VmValue::Map(entries) => {
            let mut map = BTreeMap::new();
            for (key, value) in entries {
                map.insert(CborValue::Text(key.clone()), cbor_from_vm_value(value));
            }
            CborValue::Map(map)
        }
        VmValue::Enum { tag, fields } => {
            let mut raw_fields = BTreeMap::new();
            for (key, value) in fields {
                raw_fields.insert(CborValue::Text(key.clone()), cbor_from_vm_value(value));
            }
            let mut map = BTreeMap::new();
            map.insert(CborValue::Text("$tag".to_string()), CborValue::Text(tag.clone()));
            map.insert(CborValue::Text("$fields".to_string()), CborValue::Map(raw_fields));
            CborValue::Map(map)
        }
    }
}

fn mock_counter_program() -> Arc<dyn VmProgram> {
    Arc::new(NativeProgram::new(
        Arc::new(|state, event, _limits| {
            let (VmValue::Int(current), VmValue::Int(delta)) = (state, event) else {
                return Err(VmError::new(
                    VmErrorCode::InvalidInput,
                    "expected Int state/event",
                ));
            };
            Ok((VmValue::Int(current + delta), vec![VmValue::Int(*delta)]))
        }),
        Arc::new(|state, query, _limits| {
            let VmValue::Bool(read_balance) = query else {
                return Err(VmError::new(
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
    ))
}

unsafe fn write_error(out_error: *mut *mut cino_error_t, error: FfiError) {
    if !out_error.is_null() {
        // SAFETY: out_error is checked for null and points to writable storage from caller.
        unsafe {
            *out_error = make_c_error(error);
        }
    }
}

unsafe fn clear_error(out_error: *mut *mut cino_error_t) {
    if !out_error.is_null() {
        // SAFETY: out_error is checked for null and points to writable storage from caller.
        unsafe {
            *out_error = ptr::null_mut();
        }
    }
}

unsafe fn require_ptr<'a, T>(ptr: *const T, name: &str) -> Result<&'a T, FfiError> {
    if ptr.is_null() {
        return Err(FfiError::null_pointer(name));
    }
    // SAFETY: null is checked above and lifetime is tied to caller-owned handle.
    unsafe { Ok(&*ptr) }
}

unsafe fn require_mut_ptr<'a, T>(ptr: *mut T, name: &str) -> Result<&'a mut T, FfiError> {
    if ptr.is_null() {
        return Err(FfiError::null_pointer(name));
    }
    // SAFETY: null is checked above and lifetime is tied to caller-owned handle.
    unsafe { Ok(&mut *ptr) }
}

unsafe fn require_bytes_ptr<'a>(
    data: *const u8,
    len: usize,
    name: &str,
) -> Result<&'a [u8], FfiError> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(FfiError::null_pointer(name));
    }
    // SAFETY: data is non-null and len bytes are expected to be readable by caller contract.
    unsafe { Ok(std::slice::from_raw_parts(data, len)) }
}

fn run_ffi<T, F>(
    out_error: *mut *mut cino_error_t,
    run: F,
) -> (cino_status_t, Option<T>)
where
    F: FnOnce() -> Result<T, FfiError>,
{
    match run() {
        Ok(value) => {
            // SAFETY: out_error is an optional output pointer.
            unsafe { clear_error(out_error) };
            (cino_status_t::CINO_STATUS_OK, Some(value))
        }
        Err(error) => {
            // SAFETY: out_error is an optional output pointer.
            unsafe { write_error(out_error, error) };
            (cino_status_t::CINO_STATUS_ERR, None)
        }
    }
}

#[no_mangle]
pub extern "C" fn cino_program_new_mock_counter(
    out_program: *mut *mut cino_program_t,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, program) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let out_program = unsafe { require_mut_ptr(out_program, "out_program") }?;
        let runtime = Runtime::with_config(
            mock_counter_program(),
            cino_runtime::RuntimeConfig {
                limits: VmLimits::default(),
            },
        );
        let handle = Box::into_raw(Box::new(cino_program_t { runtime }));
        *out_program = handle;
        Ok(())
    });
    let _ = program;
    status
}

#[no_mangle]
pub extern "C" fn cino_program_destroy(program: *mut cino_program_t) {
    if program.is_null() {
        return;
    }
    // SAFETY: program was allocated with Box::into_raw in this library.
    unsafe {
        drop(Box::from_raw(program));
    }
}

#[no_mangle]
pub extern "C" fn cino_state_new(
    program: *const cino_program_t,
    initial_value: *const cino_value_t,
    out_state: *mut *mut cino_state_t,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, state) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let _program = unsafe { require_ptr(program, "program") }?;
        // SAFETY: pointer validation is handled in helper.
        let value = unsafe { require_ptr(initial_value, "initial_value") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_state = unsafe { require_mut_ptr(out_state, "out_state") }?;

        let vm_state = decode_vm_value(&value.bytes)?;
        let state = cino_state_t {
            inner: RuntimeStateHandle::from_value(vm_state),
        };
        *out_state = Box::into_raw(Box::new(state));
        Ok(())
    });
    let _ = state;
    status
}

#[no_mangle]
pub extern "C" fn cino_state_destroy(state: *mut cino_state_t) {
    if state.is_null() {
        return;
    }
    // SAFETY: state was allocated with Box::into_raw in this library.
    unsafe {
        drop(Box::from_raw(state));
    }
}

#[no_mangle]
pub extern "C" fn cino_state_to_value(
    state: *const cino_state_t,
    out_value: *mut *mut cino_value_t,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, value) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let state = unsafe { require_ptr(state, "state") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_value = unsafe { require_mut_ptr(out_value, "out_value") }?;
        let bytes = encode_vm_value(state.inner.as_value())?;
        *out_value = Box::into_raw(Box::new(cino_value_t { bytes }));
        Ok(())
    });
    let _ = value;
    status
}

#[no_mangle]
pub extern "C" fn cino_update(
    program: *const cino_program_t,
    state: *const cino_state_t,
    event: *const cino_value_t,
    out_next_state: *mut *mut cino_state_t,
    out_actions: *mut *mut cino_actions_t,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, result) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let program = unsafe { require_ptr(program, "program") }?;
        // SAFETY: pointer validation is handled in helper.
        let state = unsafe { require_ptr(state, "state") }?;
        // SAFETY: pointer validation is handled in helper.
        let event = unsafe { require_ptr(event, "event") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_next_state = unsafe { require_mut_ptr(out_next_state, "out_next_state") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_actions = unsafe { require_mut_ptr(out_actions, "out_actions") }?;

        let event = decode_vm_value(&event.bytes)?;
        let updated = program
            .runtime
            .update(&state.inner, &event)
            .map_err(map_runtime_error)?;

        let actions_bytes = encode_vm_value(&VmValue::List(updated.actions))?;

        *out_next_state = Box::into_raw(Box::new(cino_state_t {
            inner: updated.state,
        }));
        *out_actions = Box::into_raw(Box::new(cino_actions_t {
            bytes: actions_bytes,
        }));

        Ok(())
    });
    let _ = result;
    status
}

#[no_mangle]
pub extern "C" fn cino_query(
    program: *const cino_program_t,
    state: *const cino_state_t,
    query: *const cino_value_t,
    out_result: *mut *mut cino_value_t,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, result) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let program = unsafe { require_ptr(program, "program") }?;
        // SAFETY: pointer validation is handled in helper.
        let state = unsafe { require_ptr(state, "state") }?;
        // SAFETY: pointer validation is handled in helper.
        let query = unsafe { require_ptr(query, "query") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_result = unsafe { require_mut_ptr(out_result, "out_result") }?;

        let query = decode_vm_value(&query.bytes)?;
        let result = program
            .runtime
            .query(&state.inner, &query)
            .map_err(map_runtime_error)?;
        let bytes = encode_vm_value(&result.result)?;
        *out_result = Box::into_raw(Box::new(cino_value_t { bytes }));

        Ok(())
    });
    let _ = result;
    status
}

#[no_mangle]
pub extern "C" fn cino_value_new_from_cbor(
    data: *const u8,
    len: usize,
    out_value: *mut *mut cino_value_t,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, value) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let bytes = unsafe { require_bytes_ptr(data, len, "data") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_value = unsafe { require_mut_ptr(out_value, "out_value") }?;
        let decoded = decode_vm_value(bytes)?;
        let canonical = encode_vm_value(&decoded)?;
        *out_value = Box::into_raw(Box::new(cino_value_t { bytes: canonical }));
        Ok(())
    });
    let _ = value;
    status
}

#[no_mangle]
pub extern "C" fn cino_value_destroy(value: *mut cino_value_t) {
    if value.is_null() {
        return;
    }
    // SAFETY: value was allocated with Box::into_raw in this library.
    unsafe {
        drop(Box::from_raw(value));
    }
}

#[no_mangle]
pub extern "C" fn cino_value_bytes(
    value: *const cino_value_t,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, bytes) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let value = unsafe { require_ptr(value, "value") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_ptr = unsafe { require_mut_ptr(out_ptr, "out_ptr") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_len = unsafe { require_mut_ptr(out_len, "out_len") }?;

        *out_ptr = value.bytes.as_ptr();
        *out_len = value.bytes.len();
        Ok(())
    });
    let _ = bytes;
    status
}

#[no_mangle]
pub extern "C" fn cino_actions_destroy(actions: *mut cino_actions_t) {
    if actions.is_null() {
        return;
    }
    // SAFETY: actions was allocated with Box::into_raw in this library.
    unsafe {
        drop(Box::from_raw(actions));
    }
}

#[no_mangle]
pub extern "C" fn cino_actions_bytes(
    actions: *const cino_actions_t,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
    out_error: *mut *mut cino_error_t,
) -> cino_status_t {
    let (status, bytes) = run_ffi(out_error, || {
        // SAFETY: pointer validation is handled in helper.
        let actions = unsafe { require_ptr(actions, "actions") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_ptr = unsafe { require_mut_ptr(out_ptr, "out_ptr") }?;
        // SAFETY: pointer validation is handled in helper.
        let out_len = unsafe { require_mut_ptr(out_len, "out_len") }?;

        *out_ptr = actions.bytes.as_ptr();
        *out_len = actions.bytes.len();
        Ok(())
    });
    let _ = bytes;
    status
}

#[no_mangle]
pub extern "C" fn cino_error_destroy(error: *mut cino_error_t) {
    if error.is_null() {
        return;
    }
    // SAFETY: error was allocated with Box::into_raw in this library.
    unsafe {
        drop(Box::from_raw(error));
    }
}

#[no_mangle]
pub extern "C" fn cino_error_code(error: *const cino_error_t) -> cino_error_code_t {
    if error.is_null() {
        return cino_error_code_t::CINO_ERROR_ABI_INVALID_HANDLE;
    }
    // SAFETY: error is checked for null and points to a valid handle by caller contract.
    unsafe { (*error).code }
}

#[no_mangle]
pub extern "C" fn cino_error_message(error: *const cino_error_t) -> *const c_char {
    if error.is_null() {
        return ptr::null();
    }
    // SAFETY: error is checked for null and points to a valid handle by caller contract.
    unsafe { (*error).message.as_ptr() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[link(name = "cino_ffi_integration", kind = "static")]
    unsafe extern "C" {
        fn cino_ffi_c_integration_test() -> i32;
    }

    #[test]
    fn cbor_roundtrip_for_tuple_and_enum() {
        let value = VmValue::Tuple(vec![VmValue::Int(1), VmValue::Enum {
            tag: "Event".to_string(),
            fields: BTreeMap::from([("id".to_string(), VmValue::Int(42))]),
        }]);

        let encoded = encode_vm_value(&value).expect("must encode");
        let decoded = decode_vm_value(&encoded).expect("must decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn value_new_from_cbor_rejects_invalid_bytes() {
        let mut out_value = ptr::null_mut();
        let mut out_error = ptr::null_mut();
        let status = cino_value_new_from_cbor(
            [0xff].as_ptr(),
            1,
            &mut out_value,
            &mut out_error,
        );
        assert_eq!(status as u32, cino_status_t::CINO_STATUS_ERR as u32);
        assert!(!out_error.is_null());
        assert_eq!(
            cino_error_code(out_error),
            cino_error_code_t::CINO_ERROR_ABI_INVALID_CBOR
        );
        cino_error_destroy(out_error);
    }

    #[test]
    fn can_run_update_query_from_ffi_handles() {
        let mut program = ptr::null_mut();
        let mut error = ptr::null_mut();
        assert_eq!(
            cino_program_new_mock_counter(&mut program, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );
        assert!(!program.is_null());

        let mut initial_value = ptr::null_mut();
        assert_eq!(
            cino_value_new_from_cbor([0x0a].as_ptr(), 1, &mut initial_value, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );

        let mut state = ptr::null_mut();
        assert_eq!(
            cino_state_new(program, initial_value, &mut state, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );

        let mut event = ptr::null_mut();
        assert_eq!(
            cino_value_new_from_cbor([0x07].as_ptr(), 1, &mut event, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );

        let mut next_state = ptr::null_mut();
        let mut actions = ptr::null_mut();
        assert_eq!(
            cino_update(program, state, event, &mut next_state, &mut actions, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );

        let mut query = ptr::null_mut();
        assert_eq!(
            cino_value_new_from_cbor([0xf5].as_ptr(), 1, &mut query, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );

        let mut result = ptr::null_mut();
        assert_eq!(
            cino_query(program, next_state, query, &mut result, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );

        let mut result_ptr: *const u8 = ptr::null();
        let mut result_len = 0usize;
        assert_eq!(
            cino_value_bytes(result, &mut result_ptr, &mut result_len, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );
        assert_eq!(unsafe { std::slice::from_raw_parts(result_ptr, result_len) }, [0x11]);

        let mut actions_ptr: *const u8 = ptr::null();
        let mut actions_len = 0usize;
        assert_eq!(
            cino_actions_bytes(actions, &mut actions_ptr, &mut actions_len, &mut error) as u32,
            cino_status_t::CINO_STATUS_OK as u32
        );
        assert_eq!(
            unsafe { std::slice::from_raw_parts(actions_ptr, actions_len) },
            [0x81, 0x07]
        );

        cino_value_destroy(result);
        cino_value_destroy(query);
        cino_actions_destroy(actions);
        cino_state_destroy(next_state);
        cino_value_destroy(event);
        cino_state_destroy(state);
        cino_value_destroy(initial_value);
        cino_program_destroy(program);
    }

    #[test]
    fn c_integration_can_call_update_and_query() {
        // SAFETY: this function is defined in tests/ffi_integration.c and has no parameters.
        let status = unsafe { cino_ffi_c_integration_test() };
        assert_eq!(status, 0);
    }
}
