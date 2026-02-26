# cino

cino is a deterministic domain DSL runtime and toolchain.

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
