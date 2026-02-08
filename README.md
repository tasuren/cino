# cino

cino is a deterministic domain DSL runtime and toolchain.

## Cargo workspace (MVP)

This repository uses a Cargo workspace with the following crates:

- `cino-syntax`: syntax tree and parser layer
- `cino-sema`: static semantics layer
- `cino-ir`: typed IR and lowering layer
- `cino-vm`: bytecode execution layer
- `cino-runtime`: public runtime API layer
- `cino-cli`: developer CLI

## Dependency direction

Dependencies are configured as a one-way layered graph without cycles.

- `cino-sema` -> `cino-syntax`
- `cino-ir` -> `cino-sema`
- `cino-vm` -> `cino-ir`
- `cino-runtime` -> `cino-vm`
- `cino-cli` -> `cino-runtime`
