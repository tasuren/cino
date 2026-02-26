# cino IR & Code Generation Specification (Rust-first, Draft)

## 1. Scope

This document defines the compilation pipeline.

- Parser AST
- Validated IR
- Bytecode for the Rust VM (primary path)
- Optional Rust source generation (secondary path)

## 2. Pipeline

1. Parse source into AST
2. Name resolution and symbol table construction
3. Type / purity / exhaustiveness / closure constraint checking
4. Lower to typed IR
5. Generate bytecode from typed IR

If an error occurs at any stage, compilation fails.

## 3. AST Requirements

AST nodes must hold the following:

- Source location (`file`, `line`, `column`)
- Metadata for documentation generation (ja/en names, description, constraints)
- Type information slot (resolved in later stages)

## 4. Typed IR Requirements

The IR must satisfy the following:

- Explicit representation with all syntactic sugar eliminated
- Fully typed
- Purity-checked
- Evaluation order is deterministic

Minimum instruction elements:

- Constants
- Variable binding / reference
- Enum / record construction
- Match branching
- List / map operations
- Function calls
- Tuple / result returns

## 5. Bytecode VM (Primary Path)

The canonical execution format for MVP is bytecode.

- The canonical abstract machine for MVP uses a **stack-based** design.
- Instruction semantics must be deterministic.
- Runtime traps must be converted to structured errors.

### 5.0 MVP Bootstrap (Provisional)

During the initial implementation phase, in addition to the canonical path (IR -> bytecode -> VM),
`cino-vm` may provide an **evaluator that directly executes typed IR**.

- Permitted scope covers the minimum MVP expressions: `LocalRef` / `Int` / `Bool` / `Tuple` / `List` / `Record` / `Binary` / `Call` / `Let` / `Match`
- `Match` arm patterns may be `Wildcard` / `Binding` / `Variant`
- The public contract, determinism, and over-limit error contract for `update/query` are identical to bytecode execution.
- Panics / traps must be converted to structured errors.
- Once the bytecode path is stable, the direct IR evaluator may be demoted to a development / validation tool.

### 5.1 Abstract Machine State

Execution state is defined by the following tuple:

- `pc`: Position of the next instruction to execute
- `stack`: Value stack (LIFO)
- `locals`: Local variable array for the current frame
- `call_stack`: Call frame sequence (`return_pc`, `locals`, `function_id`)
- `budget`: Remaining execution step budget

`budget` is decremented by 1 for each instruction executed; reaching 0 returns `E-RUNTIME-STEP-LIMIT`.

### 5.2 MVP Instruction Set

Instructions are represented by an opcode and fixed-length / variable-length operands. The following is the minimum MVP set.

| Instruction | Precondition | Output (on success) | Failure condition |
| --- | --- | --- | --- |
| `CONST k` | - | `stack.push(const_pool[k])` | `k` out of range (`E-BC-INVALID-CONST`) |
| `LOAD_LOCAL i` | `locals[i]` exists | `stack.push(locals[i])` | `i` out of range (`E-BC-INVALID-LOCAL`) |
| `STORE_LOCAL i` | Value `v` at top of `stack` | `locals[i] = v` | `stack` empty / `i` out of range |
| `MAKE_RECORD type_id, n` | `n` field values on `stack` | Push `Record(type_id, fields)` | `n` invalid / type mismatch |
| `MAKE_ENUM tag_id, n` | `n` payload values on `stack` | Push `Enum(tag_id, payload)` | `tag_id` invalid / arity mismatch |
| `LIST_NEW n` | `n` element values on `stack` | Push `List` | `n` invalid |
| `MAP_NEW n` | `2n` values on `stack` (`k1,v1,...`) | Push `Map` | Non-comparable key / duplicate key / `n` invalid |
| `GET_FIELD field_idx` | Top of `stack` is a record | Push field value | Not a record / out of range |
| `JUMP target` | - | `pc = target` | `target` out of range (`E-BC-INVALID-JUMP`) |
| `JUMP_IF_FALSE target` | Top of `stack` is `Bool` | If false, `pc = target` | Not a `Bool` / `target` out of range |
| `MATCH_TAG {tag->target}` | Top of `stack` is an enum | Update `pc` to matching branch | Not an enum / undefined tag |
| `CALL fn_id, argc` | `argc` arguments at top of `stack` | Push new frame and jump to callee | `fn_id` invalid / arity mismatch / recursion limit exceeded |
| `RETURN` | Return value at top of `stack` | Return to caller and push return value | Frame inconsistency / empty stack |
| `TUPLE2` | 2 values at top of `stack` | Push `(a, b)` | Stack underflow |
| `RESULT_OK` | Value `v` at top of `stack` | Push `Result::Ok(v)` | Stack underflow |
| `RESULT_ERR` | Value `e` at top of `stack` | Push `Result::Err(e)` | Stack underflow |

Notes:

- Duplicate keys in `MAP_NEW` are **runtime errors** and do not silently use the first key.
- `MATCH_TAG` references a precompiled branch table and does not perform linear search (eliminates ordering differences due to implementation variance).

### 5.3 Determinism Rules

Instruction generation and execution must satisfy the following:

- Expression evaluation order is always **left to right**.
- `record` field evaluation order follows the source declaration order.
- `List` element evaluation order follows the source declaration order.
- `Map` key/value pairs are evaluated in source declaration order and constructed in that order.
- `match` arm checking order follows the source declaration order (though execution uses direct jumps via `MATCH_TAG`).
- Function argument evaluation order is left to right; `CALL` binds all arguments to the frame after evaluation.
- The VM does not call back to the host during execution (`Action` is only constructed as a value).

### 5.4 Execution Example (Key Instructions)

Input cino (conceptual):

```cino
update(state: S, event: E) -> (S, List<Action>) {
  match event {
    Tick {} => ({ count: state.count + 1 }, [Action.Notify])
  }
}
```

Corresponding instruction sequence (conceptual):

```text
LOAD_LOCAL 1                 ; event
MATCH_TAG { Tick -> L_tick }
L_tick:
LOAD_LOCAL 0                 ; state
GET_FIELD 0                  ; count
CONST 0                      ; 1
CALL add_int 2
MAKE_RECORD S 1
CONST 1                      ; Action.Notify
LIST_NEW 1
TUPLE2
RETURN
```

This example fixes the semantics of `MATCH_TAG`, `GET_FIELD`, `MAKE_RECORD`, `LIST_NEW`, `TUPLE2`, and `RETURN`.

## 6. Rust Source Generation (Optional)

Rust code generation is available only for the following purposes:

- Debugging
- Observability
- Offline verification

The canonical semantics reside in IR + VM; nothing depends on the generated Rust output.

## 7. Compatibility

### 7.1 Bytecode Header

Every bytecode file begins with the following header:

- `magic`: `CINOBC` (6 bytes)
- `major`: u16
- `minor`: u16
- `flags`: u16 (fixed to 0 in MVP)

### 7.2 Versioning Rules

- `major`: incremented on breaking changes to instruction semantics, encoding, or validation rules.
- `minor`: incremented on backward-compatible instruction additions or metadata additions.
- A `minor` change must not alter the semantics of any existing instruction.

### 7.3 Compatibility Determination

The runtime accepts or rejects bytecode according to the following rules:

- `magic` mismatch → `E-BC-BAD-MAGIC`
- `major` mismatch → `E-BC-MAJOR-MISMATCH`
- Executable only when `major` matches and `bytecode.minor <= runtime.supported_minor`
- If the above conditions are not met, the runtime fails with an explicit error before execution begins.

### 7.4 Examples of Breaking Changes

The following always require a `major` increment:

- Changing the semantics of an existing opcode (e.g., changing the duplicate key rule for `MAP_NEW`)
- Changing operand interpretation (e.g., changing argument order for `CALL`)
- Changing the meaning of an existing trap / error code

## 8. Rust Workspace Structure (MVP Recommended)

MVP adopts a Cargo workspace and splits responsibilities into the following crates:

1. `cino-syntax`
2. `cino-sema`
3. `cino-ir`
4. `cino-vm`
5. `cino-codec`
6. `cino-runtime`
7. `cino-ffi-c`
8. `cino-cli`

In the initial phase, `cino-syntax` / `cino-sema` / `cino-ir` / `cino-vm` / `cino-cli` form the minimum set;
CBOR serialization (`cino-codec`) and FFI (`cino-ffi-c`) may be added incrementally.

## 9. Crate Dependency Direction

Dependencies follow these directions as a principle:

- `cino-sema` -> `cino-syntax`
- `cino-ir` -> `cino-sema`, `cino-syntax`
- `cino-vm` -> `cino-ir`, `cino-syntax`
- `cino-codec` -> `cino-vm`
- `cino-runtime` -> `cino-vm`
- `cino-ffi-c` -> `cino-runtime`, `cino-vm`, `cino-codec`
- `cino-cli` -> `cino-runtime`, `cino-syntax`, `cino-sema`, `cino-ir`, `cino-vm`, `cino-codec`

Circular dependencies are prohibited.
