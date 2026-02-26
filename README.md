# cino

cino is an experimental DSL and runtime for writing domain logic that is structurally incapable of side effects.

The core idea is a hard boundary between domain logic and the outside world. In cino, I/O, time, randomness, and external calls are not merely discouraged — they are forbidden at the language level. Domain behavior is expressed solely through pure `update` (state transition) and `query` (state read) functions, while any interaction with the outside world is represented as an `Action` value returned to the host. This constraint is enforced by the compiler, not by convention.

The goal is to explore how strictly enforcing this boundary affects the testability, portability, and long-term maintainability of domain logic.

For the full rationale and design overview, see [docs/proposal.md](docs/proposal.md).

## Cargo workspace (MVP)

This repository uses a Cargo workspace with the following crates:

- `cino-syntax`: syntax tree and parser layer
- `cino-sema`: static semantics layer
- `cino-ir`: typed IR and lowering layer
- `cino-vm`: bytecode execution layer
- `cino-codec`: CBOR serialization/deserialization layer for VM values
- `cino-runtime`: public runtime API layer
- `cino-ffi-c`: C ABI bindings (cdylib/rlib) for host integration
- `cino-cli`: developer CLI

## Examples

The [`examples/`](examples/) directory contains standalone `.cino` programs and a Rust integration sample. See [examples/README.md](examples/README.md) for the full list and usage instructions.

## CLI Usage

The `cino` CLI provides tools for developing and verifying cino programs.

### Check syntax and static semantics
```bash
cino check --file examples/counter.cino
```

### Run update or query
```bash
# Increment count state to 1 from 0
cino run update --file examples/counter.cino \
  --state '0' \
  --event '{"$tag": "Increment", "$fields": {}}'

# Get count state when count state is 5
cino run query --file examples/counter.cino \
  --state '5' \
  --query '{"$tag": "GetCount", "$fields": {}}'
```

### Generate documentation
```bash
cino docgen --file examples/counter.cino --out ./docs
```

## Dependency direction

Dependencies are configured as a one-way layered graph without cycles.

- `cino-sema` -> `cino-syntax`
- `cino-ir` -> `cino-sema`, `cino-syntax`
- `cino-vm` -> `cino-ir`, `cino-syntax`
- `cino-codec` -> `cino-vm`
- `cino-runtime` -> `cino-vm`
- `cino-ffi-c` -> `cino-runtime`, `cino-vm`, `cino-codec`
- `cino-cli` -> `cino-runtime`, `cino-syntax`, `cino-sema`, `cino-ir`, `cino-vm`, `cino-codec`
