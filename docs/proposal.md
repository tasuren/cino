# cino Proposal (Draft)

## 1. Background & Problem

Modern software development repeatedly encounters the following problems:

- Domain logic and side effects (I/O, DB, network, UI) are mixed together, making testing and change difficult.
- Responsibilities that are supposed to be separated by design gradually erode over time in production.
- Programs and their specification documents drift apart, making it impossible to guarantee the correctness of the specification.
- The same domain logic is re-implemented across multiple languages and platforms.

These problems must be prevented not by human discipline, but by the language and compiler.

---

## 2. Goals

cino aims to:

- Provide a **domain definition language that is structurally incapable of side effects**.
- Constrain domain logic to **state transition (update) and state read (query)**.
- Make domain logic **safely usable from Rust / C / C++ / Python / JavaScript**.
- Enable **automatic generation of Japanese and English specification documents** from the program itself.

---

## 3. Design Principles (Key)

### 3.1 Complete Prohibition of Side Effects

cino programs **prohibit the following at the syntax and type level**:

- I/O
- Clock / wall time access
- Random number generation
- Global state
- Exception throwing
- External library calls

All interaction with the outside world is expressed **only by returning Action values**.

---

### 3.2 Core Model (Minimal Computation Model)

The domain model provided by cino is limited to the following:

#### State

- The state of the domain.
- Opaque (not directly accessible from host languages).

#### Event

- Input that triggers a state transition.
- Injected from outside (time and other external inputs are expressed as Events).

#### Update

```
update : State × Event -> (State, List<Action>)
```

- State transitions are always pure functions.
- Side effects can only be "requested" as Actions.

#### Query

```
query : State × Query -> Result
```

- Read-only API for state.
- Includes derived values, judgments, and display computations.
- No side effects.

#### Action

- An abstract request for a side effect.
- Execution is the responsibility of the host language.
- Execution results are returned to cino as Events.

---

### 3.3 Role of Query

Query serves the following purposes:

- Does not expose the internal structure of state to the outside.
- Allows the domain to explicitly define "what it exposes externally".
- Safely generates UI display, business judgments, and API responses.

---

## 4. Type System & Expressiveness

### 4.1 Primitive Data Types

- Numbers (integer, fixed-point decimal)
- String
- Boolean
- List / Map
- Option / Result

### 4.2 Algebraic Data Types

- Enum (tagged union)
- Record (immutable struct)

### 4.3 Pattern Matching

- Exhaustiveness checking is required for Event / Query / Enum.

---

## 5. Testing

### 5.1 Unit Tests

- `update` / `query` are completely deterministic.
- Same input → same output is guaranteed.
- Can be written with the cino standard test DSL.

### 5.2 Testability Guarantee

- Time and randomness are injected as Events, so no mocking is needed.
- Actions are not executed; only whether they were generated is verified.

---

## 6. Translation & Specification Generation

### 6.1 Fundamental Policy

- LLMs are not used.
- Semantic metadata is embedded in the AST.
- Text is generated using templates.

### 6.2 Metadata Examples

- Japanese name / English name
- Description
- Constraints
- Term category (state / operation / judgment)

### 6.3 Output Artifacts

- Japanese specification document (Markdown / HTML)
- English specification document (Markdown / HTML)
- Diff-friendly (Git-friendly)

---

## 7. Host Language Integration (FFI)

### 7.1 Delivery Form (Not Yet Final, Needs Discussion)

- The cino compiler generates:
    - Binary (static / shared)
    - WASM (under consideration)
- Alternatively, an interpreter implementation is provided.

### 7.2 External API

- State is an opaque handle.
- Available operations:
    - Create initial State
    - Call update
    - Call query

### 7.3 Error Handling

- No exceptions.
- Everything is returned as a Result type.

---

## 8. Target Use Cases

- Billing and invoicing rules
- Workflows (request, approval)
- Authorization and policy evaluation
- Business rule engines
- State-machine-style domains

---

## 9. Non-Goals (What cino Will Not Do)

- UI construction
- Database access
- High-frequency real-time computation
- General-purpose scripting language

---

## 10. Development Steps (Instructions for AI)

1. Define the minimal syntax (State / Event / update / query)
2. Design the AST
3. Type checking
4. Test DSL
5. Specification document generator
6. Rust backend implementation
7. Multi-language FFI extensions

---

## Note

cino's highest priority is not to be a "convenient language" —  
it is to be **a language whose domain boundaries cannot be broken**.
