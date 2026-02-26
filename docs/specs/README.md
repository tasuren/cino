# cino Specification Set (MVP)

## Document List

1. `core-language.md`
2. `static-semantics.md`
3. `runtime-memory-rust.md`
4. `ir-codegen-rust.md`
5. `host-abi-ffi.md`
6. `docgen-spec.md`

## Recommended Reading Order

1. Core Language Specification
2. Static Semantics Specification
3. Runtime & Memory Specification
4. IR & Code Generation Specification
5. Host ABI & FFI Specification
6. Documentation Generation Specification

## Purpose

This set defines the following minimum contracts:

- Deterministic domain behavior
- Rust-first interpreter / VM implementation
- Stable host integration and documentation generation

## Implementation Crates (MVP Recommended)

- `cino-syntax`: Syntax tree and parser
- `cino-sema`: Type / purity / exhaustiveness checking
- `cino-ir`: Typed IR and lowering
- `cino-vm`: Bytecode executor (includes a direct IR evaluator as an MVP bootstrap)
- `cino-codec`: CBOR serialization / deserialization
- `cino-runtime`: Execution API and state handle management
- `cino-ffi-c`: C ABI public layer
- `cino-cli`: Developer-facing CLI
