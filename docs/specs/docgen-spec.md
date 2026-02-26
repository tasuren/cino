# cino Documentation Generation Specification (Draft)

## 1. Scope

This document defines the rules for deterministically generating specification documents from cino source.
LLMs are not used during generation.

## 2. Input

- Parsed AST
- Validated symbol / type information
- Metadata annotations

Required metadata for each domain symbol:

- `name_ja`
- `name_en`
- Short description

Optional items:

- Constraints
- Category tags (`state`, `event`, `query`, `rule`, `term`)

## 3. Output Formats

- Japanese specification: Markdown / HTML
- English specification: Markdown / HTML

The output must produce stable diffs for the same input.

## 4. Generation Rules

- Section order is fixed.
- Generation is template-based.
- No implicit inference beyond AST and metadata.

## 5. Minimum Generated Sections

- Domain overview
- State model
- Event list
- Query list
- Update transition rules
- Action list
- Constraints and invariants

## 6. Traceability

Each generated section retains traceability information:

- Source symbol ID
- Source location (when available)

This enables round-trip verification between the specification document and the code.

## 7. Multi-language Consistency

- Missing `name_ja` / `name_en` is a warning (error in strict mode).
- A glossary is generated in both Japanese and English.
- Section IDs are stabilized across languages.

## 8. CLI Contract (Proposed)

```text
cino docgen --in <source-or-package> --lang ja --out ./docs/ja
cino docgen --in <source-or-package> --lang en --out ./docs/en
```

If metadata is incomplete in strict mode, the process exits with a non-zero exit code.
