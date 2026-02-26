# cino Core Language Specification (Draft)

## 1. Scope

This document defines the core language model of cino.
cino is a deterministic domain definition language that has no direct side effects.

## 2. Core Model

- `State`: opaque domain state
- `Event`: input that triggers a state transition
- `Update`: `State x Event -> (State, List<Action>)`
- `Query`: request to read state
- `Action`: side-effect request passed to the host runtime

## 3. Purity and Forbidden Operations

The following are forbidden in user programs.

- I/O
- Time access
- Random number generation
- Global mutable state
- Throwing/catching exceptions
- Direct calls to external libraries

The only permitted side-effect boundary is constructing `Action` values.

## 4. Declarations

Top-level declarations are limited to the following.

- `state` (immutable record)
- `event` (tagged union)
- `query` (tagged union)
- `result` / domain `enum` / `record`
- `update` function
- `query` function

Example:

```cino
state BillingState {
  invoices: Map<InvoiceId, Invoice>
  balance: Decimal
}

event BillingEvent =
  | InvoiceIssued { id: InvoiceId, amount: Decimal }
  | PaymentReceived { id: InvoiceId, amount: Decimal }

query BillingQuery =
  | CurrentBalance
  | InvoiceStatus { id: InvoiceId }

update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) = ...
query(state: BillingState, q: BillingQuery) -> Result<QueryResult, DomainError> = ...
```

## 5. Minimal Syntax EBNF (MVP)

### 5.1 Notation

- Items enclosed in `"` are reserved words or terminal symbols
- `A | B` denotes alternation
- `{ X }` denotes zero or more repetitions
- `[ X ]` denotes zero or one occurrence

### 5.2 Grammar

```ebnf
program         = { top_decl } ;
top_decl        = state_decl
                | event_decl
                | query_decl
                | enum_decl
                | record_decl
                | user_fn_decl
                | update_fn_decl
                | query_fn_decl ;

state_decl      = "state" type_name record_body ;
event_decl      = "event" type_name "=" variant_list ;
query_decl      = "query" type_name "=" variant_list ;
enum_decl       = "enum" type_name "=" variant_list ;
record_decl     = "record" type_name record_body ;

update_fn_decl  = "update" "(" param "," param ")" "->" tuple_type block ;
query_fn_decl   = "query" "(" param "," param ")" "->" type_expr block ;
user_fn_decl    = "fn" ident "(" [ param { "," param } ] ")" "->" type_expr block ;

variant_list    = variant { variant } ;
variant         = "|" ctor_name [ record_payload ] ;
record_body     = "{" { field_decl } "}" ;
record_payload  = "{" { field_decl } "}" ;
field_decl      = ident ":" type_expr ;
param           = ident ":" type_expr ;

tuple_type      = "(" type_expr "," type_expr ")" ;
type_expr       = simple_type | generic_type ;
simple_type     = type_name ;
generic_type    = type_name "<" type_expr { "," type_expr } ">" ;

match_expr      = "match" expr "{" { match_arm } "}" ;
match_arm       = pattern "=>" expr ;
pattern         = ctor_name [ "{" { pat_field } "}" ] | "_" ;
pat_field       = ident [ ":" pattern ] ;

result_type     = "Result" "<" type_expr "," type_expr ">" ;
option_type     = "Option" "<" type_expr ">" ;

type_name       = ident ;
ctor_name       = ident ;
ident           = IDENT ;
expr            = EXPR ;
block           = "{" { stmt } expr "}" ;
stmt            = let_stmt ;
let_stmt        = "let" ident "=" expr ;
```

### 5.3 Open Questions (TODO)

- TODO: Lexical rules including `IDENT` and reserved word conflict avoidance (including Unicode ranges)
- TODO: Operator precedence/associativity rules and the minimal operator set for `EXPR`
- TODO: Line separators in `variant_list` (whether newline is required or `;` is permitted)
- TODO: Whether to separate the `query` keyword (type declaration vs. function declaration) in the future
- TODO: Whether `Map<K, V>` `K` comparability constraints are expressed in syntax or static semantics

## 6. Syntax Examples (MVP)

### 6.1 `state`

```cino
state BillingState {
  balance: Decimal
  last_invoice_id: Option<InvoiceId>
}
```

### 6.2 `event`

```cino
event BillingEvent =
  | InvoiceIssued { id: InvoiceId, amount: Decimal }
  | PaymentReceived { id: InvoiceId, amount: Decimal }
```

### 6.3 `query` (declaration)

```cino
query BillingQuery =
  | CurrentBalance
  | InvoiceStatus { id: InvoiceId }
```

### 6.4 `update` function declaration

```cino
update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) {
  ...
}
```

### 6.5 `query` function declaration

```cino
query(state: BillingState, q: BillingQuery) -> Result<QueryResult, DomainError> {
  ...
}
```

### 6.6 `enum`

```cino
enum InvoiceStatus =
  | Draft
  | Open
  | Closed
```

### 6.7 `record`

```cino
record Invoice {
  id: InvoiceId
  amount: Decimal
}
```

### 6.8 `match`

```cino
match event {
  InvoiceIssued { id, amount } => ...
  PaymentReceived { id, amount } => ...
}
```

### 6.9 `Result`

```cino
query(state: BillingState, q: BillingQuery) -> Result<QueryResult, DomainError> {
  ...
}
```

### 6.10 `Option`

```cino
record BillingState {
  last_invoice_id: Option<InvoiceId>
}
```

### 6.11 User-defined function `fn`

```cino
fn can_issue(balance: Decimal, limit: Decimal) -> Bool {
  balance <= limit
}
```

### 6.12 Multi-statement expression block

```cino
fn score(base: Int, bonus: Int) -> Int {
  let doubled = base * 2
  let total = doubled + bonus
  total
}
```

## 7. Data Types

Built-in types:

- `Int`, `Decimal`, `Bool`, `String`
- `List<T>`, `Map<K, V>`
- `Option<T>`, `Result<T, E>`
- User-defined `enum` / `record`

TODO: `Decimal` is recognized by the type-checking layer (`cino-sema`) but has no corresponding variant in the VM value representation (`VmValue`), so it is not supported at runtime.  
TODO: `String` is recognized by the type-checking layer, but the lexer (`cino-syntax`) does not support string literals, so string values cannot actually be written.

## 8. Pattern Matching

- `match` on `event` / `query` / any `enum` must be exhaustive
- Non-exhaustive matches are compile errors
- Fallthrough is not permitted

## 9. Evaluation Contract

- `fn` (user-defined functions) must be pure and deterministic
- `update` must be pure and deterministic
- `query` must be pure and deterministic
- Identical inputs must produce identical outputs
- Changes from the outside world are expressed only through `Event` injection

## 10. Error Model

- Exceptions are not used
- Domain failures are expressed explicitly via `Result`

## 11. Stability Rule

Future language extensions must not break determinism or the prohibition on side effects.
