# cino Runtime & Memory Specification (Rust, Draft)

## 1. Scope

This document defines the runtime behavior of the Rust-based interpreter / VM.

Goals:

- Determinism
- Robust memory safety
- Simple host integration

## 2. Execution Units

`update` and `query` are treated as independent execution units.

- Input is immutable.
- Only output-reachable data survives after a call.
- A step limit is enforced to prevent runaway execution.

## 3. Memory Model

MVP adopts a per-call region approach.

- A temporary arena is allocated for each call.
- The temporary region is discarded when `update` / `query` returns.
- Only values reachable from the output survive.

Surviving data:

- `update`: the returned `State` and `List<Action>`
- `query`: the returned `Result`

## 4. Persistent State

`State` is immutable and may use internal sharing (persistent data structures).
The host treats it as an opaque handle.

## 5. Deterministic Execution Rules

- No dependency on wall-clock time.
- No dependency on random numbers.
- No host callbacks during evaluation.
- Collection traversal order is fixed by specification.

## 6. Error Behavior

Runtime failures are returned as explicit error values.
Panics across the FFI boundary are prohibited.
Internal panics are converted to structured errors.

## 7. Resource Limits

The following limits must be configurable:

- Maximum execution step count
- Maximum memory usage per call
- Maximum recursion depth (or equivalent stack budget)

When a limit is reached, a structured error is returned rather than a trap.

## 8. Concurrency

The MVP runtime itself may be single-threaded.
Concurrent execution is managed by the host on a per-state-handle basis.
Shared mutable global state is prohibited.

## 9. Crate Responsibility Boundaries (Execution Layer)

The execution layer is separated into at least two layers:

- `cino-vm`: Bytecode executor (and, as an MVP bootstrap, a direct typed-IR evaluator), instruction semantics, step limiting, per-call memory management.
- `cino-runtime`: Public API for `update/query`, `State` handle management, host-facing execution context.

`cino-runtime` is the host integration boundary and must not directly expose the internal representations of `cino-vm`.
