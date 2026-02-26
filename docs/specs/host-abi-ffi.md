# cino Host ABI & FFI Specification (Draft)

## 1. Scope

This document defines the host integration contract.
The canonical low-level interface is the C ABI.
The Rust crate API is provided as a safe wrapper around the C ABI.

## 2. Opaque Handles

Host-visible opaque types:

- `cino_program_t`
- `cino_state_t`
- `cino_value_t` (when needed)
- `cino_actions_t`
- `cino_error_t`

The host must not access the internal layout of these handles.

## 3. Lifecycle API (Conceptual)

- Program creation / loading
- Initial state creation
- Execute update
- Execute query
- Release result / action / error

Every created handle must have a paired `destroy/free` function.

## 4. Update / Query Contract

- update input: `(state, event)`
- update output: `(new_state, actions)` or `error`
- query input: `(state, query)`
- query output: `result` or `error`

Exceptions must not cross the ABI boundary.

## 5. Ownership Rules

- The caller owns the returned handle.
- Passing a handle as an argument does not transfer ownership (except through explicit transfer APIs).
- Ownership-transferring APIs must be clearly named and documented in the specification.

## 6. Serialization Boundary

MVP fixes the serialization format to CBOR (`cino-codec` crate).

- `cino_value_t` / `cino_actions_t` internally hold CBOR byte sequences.
- The host passes a CBOR byte sequence via `cino_value_new_from_cbor` and retrieves it via `cino_value_bytes` / `cino_actions_bytes`.
- CBOR encoding conforms to RFC 8949 Core Deterministic Encoding Requirements.
- JSON is used only for debugging purposes and is not treated as the canonical ABI format.

## 7. Error Model

All failures are returned as explicit values.

- Compilation / loading failures
- Validation / type errors
- Runtime limit exceeded
- Invalid handle / API misuse

Each error holds the following:

- A stable error code
- A human-readable message
- An optional source location

## 8. Thread Rules

- Thread safety for each handle must be explicitly documented.
- If a handle is not thread-safe, external synchronization is required.

## 9. WASM Notes

The WASM API must honor the same semantic contract as the C ABI.

- Opaque state
- Explicit update / query
- Explicit error values

## 10. Implementation Crate Placement

The C ABI implementation is consolidated in the `cino-ffi-c` crate.
`cino-ffi-c` is implemented as a thin boundary layer that calls `cino-runtime` and holds no domain evaluation logic.

This policy separates the responsibilities of the host-facing contract and the internal execution engine.

## 11. MVP C ABI (Finalized)

MVP fixes the serialization boundary to CBOR.
`cino_value_t` / `cino_actions_t` internally hold CBOR byte sequences and decode them to VM values when needed.

### 11.1 Return Convention

All functions return `cino_status_t`.

- `CINO_STATUS_OK`: Success
- `CINO_STATUS_ERR`: Failure (`out_error` is set)

`out_*` pointers are written only on success; they are left unchanged on failure.

### 11.2 Opaque Handles

- `cino_program_t`
- `cino_state_t`
- `cino_value_t`
- `cino_actions_t`
- `cino_error_t`

### 11.3 Primary API

- `cino_program_new_mock_counter(out_program, out_error)`
- `cino_program_destroy(program)`
- `cino_state_new(program, initial_value, out_state, out_error)`
- `cino_state_destroy(state)`
- `cino_state_to_value(state, out_value, out_error)`
- `cino_update(program, state, event, out_next_state, out_actions, out_error)`
- `cino_query(program, state, query, out_result, out_error)`
- `cino_value_new_from_cbor(data, len, out_value, out_error)`
- `cino_value_destroy(value)`
- `cino_value_bytes(value, out_ptr, out_len)`
- `cino_actions_destroy(actions)`
- `cino_actions_bytes(actions, out_ptr, out_len)`
- `cino_error_destroy(error)`
- `cino_error_code(error)`
- `cino_error_message(error)`

### 11.4 Ownership / Deallocation

- Handles returned by `new` / `update` / `query` / `to_value` are owned by the caller.
- The caller must always call the corresponding `destroy` / `free`.
- Pointers returned by `bytes` APIs point to memory owned by the handle and remain valid until the handle is destroyed.

### 11.5 Error Codes

- `CINO_ERROR_RUNTIME_STEP_LIMIT_EXCEEDED`
- `CINO_ERROR_RUNTIME_MEMORY_LIMIT_EXCEEDED`
- `CINO_ERROR_RUNTIME_RECURSION_LIMIT_EXCEEDED`
- `CINO_ERROR_RUNTIME_INVALID_INPUT`
- `CINO_ERROR_RUNTIME_TRAP`
- `CINO_ERROR_RUNTIME_PANIC`
- `CINO_ERROR_ABI_NULL_POINTER`
- `CINO_ERROR_ABI_INVALID_CBOR`
- `CINO_ERROR_ABI_INVALID_HANDLE`
- `CINO_ERROR_ABI_INTERNAL`

### 11.6 Compatibility Note

The MVP `program` creation provides only `mock_counter`.
When adding IR / bytecode loading APIs in the future, backward compatibility must be maintained and the semantics of existing functions must not be changed.
