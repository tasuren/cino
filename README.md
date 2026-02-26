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

## CLI Usage

The `cino` CLI provides tools for developing and verifying cino programs.

### Check syntax and static semantics
```bash
cino check --file examples/counter.cino
```

### Run update or query
```bash
# カウントを 0 から Increment
cino run update --file examples/counter.cino \
  --state '0' \
  --event '{"$tag": "Increment", "$fields": {}}'

# カウント 5 の状態から GetCount クエリ
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
- `cino-ir` -> `cino-sema`
- `cino-vm` -> `cino-ir`
- `cino-runtime` -> `cino-vm`
- `cino-cli` -> `cino-runtime`
