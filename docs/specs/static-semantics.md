# cino Static Semantics Specification (MVP)

## 1. Scope

This document defines the static checking contract for cino MVP.

- Type checking
- Purity checking
- `fn` constraint checking (determinism, recursion limit)
- `match` exhaustiveness / unreachable arm checking
- Diagnostic code system

Violations of the static rules defined in this document are reported as compile errors and must include `file:line:column`.
Runtime constraints (such as recursion depth limits) are reported as runtime errors.

## 2. Fundamental Policies

- Record types are compared by structural equivalence.
- Enumeration types (`event` / `query` / `enum`) are compared by nominal equivalence.
- Implicit type conversions are not performed (disallowed until explicit syntax is specified).
- The same input must always produce the same output (determinism).

## 3. Type Rules

### 3.1 Built-in / Generic Types

- Literal types: `Int`, `Decimal`, `Bool`, `String`
- Generics: `List<T>`, `Map<K, V>`, `Option<T>`, `Result<T, E>`
- User-defined types: `record`, `enum`, `state`, `event`, `query`

`K` in `Map<K, V>` must be a type that supports deterministic comparison.

TODO: The `Decimal` type is recognized at the static semantics layer but has no corresponding variant in the VM value representation (`VmValue`), so it is unsupported at runtime.  
TODO: The `String` type is recognized at the static semantics layer but the lexer does not yet support string literals, so string values cannot currently be written.

### 3.2 Function Signatures

- `update(state: S, event: E) -> (S, List<Action>)`
- `query(state: S, q: Q) -> Result<R, Err>`
- `fn name(...) -> T`

The number of arguments and return type shape for `update` / `query` are fixed contracts; declarations that do not conform are invalid.

Successful example:

```cino
update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) {
  (state, [])
}
```

Failing example (return type contract violation):

```cino
update(state: BillingState, event: BillingEvent) -> BillingState {
  state
}
```

### 3.3 Block Expression Return Value Rules

- The body of `fn` / `update` / `query` is treated as an expression block.
- The last expression in the block is the return value.
- `return` statements are not allowed in MVP.

Successful example:

```cino
fn score(base: Int, bonus: Int) -> Int {
  let doubled = base * 2
  doubled + bonus
}
```

Failing example (`return` not supported):

```cino
fn score(base: Int, bonus: Int) -> Int {
  return base + bonus
}
```

## 4. Purity Rules

### 4.1 Prohibited Operations

The following are prohibited in `fn` / `update` / `query`:

- I/O
- Clock / wall time access
- Random number generation
- Mutable global state
- Exception throwing / catching
- Direct calls to external libraries or external functions

### 4.2 Permitted Operations

- Immutable local bindings
- Pure expression evaluation
- Calls to pure `fn` functions
- Construction of `Action` values (not execution)

Successful example:

```cino
fn can_issue(balance: Decimal, limit: Decimal) -> Bool {
  balance <= limit
}
```

Failing example (impure operation):

```cino
fn should_retry() -> Bool {
  now() > 0
}
```

## 5. `fn` Rules (MVP)

- `fn` declarations are only allowed at the top level (nested functions are prohibited).
- `fn` must be pure and deterministic.
- Only verified `fn` functions may be called from `update` / `query`.
- Recursive calls (direct or indirect) are permitted.
- Recursion depth must not exceed the runtime limit `max_recursion_depth`.

Notes:

- The limit value itself is defined in the execution configuration (`docs/specs/runtime-memory-rust.md`).
- The static semantics contract specifies that "recursion is permitted" and that "exceeding the limit is a runtime error".

Successful example (recursion):

```cino
fn fact(n: Int) -> Int {
  match n {
    0 => 1
    _ => n * fact(n - 1)
  }
}
```

Failing example (nested `fn`):

```cino
fn outer(x: Int) -> Int {
  fn inner(y: Int) -> Int { y + 1 }
  inner(x)
}
```

## 6. `match` Rules

- `match` on `event` / `query` / `enum` types must be exhaustive.
- The wildcard `_` covers all remaining cases.
- Subsequent arms that are already covered are reported as unreachable errors.
- Guard patterns (`if`-guarded arms) are not supported in MVP.

Successful example (exhaustive):

```cino
match event {
  InvoiceIssued { id, amount } => ...
  PaymentReceived { id, amount } => ...
}
```

Failing example (non-exhaustive):

```cino
match event {
  InvoiceIssued { id, amount } => ...
}
```

Failing example (unreachable arm):

```cino
match status {
  _ => 0
  Closed => 1
}
```

## 7. Features Not Supported in MVP

The following are out of specification in MVP (compile error if used):

- Closure expressions
- `return` statements
- `match` guards (`if`-guarded arms)

## 8. Diagnostic Code System

### 8.1 `E-TYPE-*` (Type)

- `E-TYPE-001`: Type mismatch
- `E-TYPE-002`: Unresolved symbol
- `E-TYPE-003`: Wrong number of generic arguments (TODO: not yet implemented)
- `E-TYPE-004`: Invalid `update/query` signature
- `E-TYPE-005`: `K` in `Map<K, V>` is not comparable (TODO: not yet implemented)

### 8.2 `E-PURE-*` (Purity)

- `E-PURE-001`: Use of a prohibited side-effectful operation
- `E-PURE-002`: Call to an external function or external library

### 8.3 `E-FN-*` (Function Rules)

- `E-FN-001`: `fn` declared outside the top level (TODO: not yet implemented)
- `E-FN-002`: `fn` body violates purity rules
- `E-FN-003`: Use of a `return` statement (not supported in MVP)
- `E-FN-004`: Recursion depth limit exceeded (runtime error code)

### 8.4 `E-MATCH-*` (Pattern Matching)

- `E-MATCH-001`: Non-exhaustive `match`
- `E-MATCH-002`: Unreachable arm
- `E-MATCH-003`: Use of a guard arm (not supported in MVP)

### 8.5 `E-UNSUPPORTED-*` (Not Supported in MVP)

- `E-UNSUPPORTED-001`: Use of a closure expression
