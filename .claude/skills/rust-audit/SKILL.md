---
name: rust-audit
description: Audit Rust code against the Microsoft Pragmatic Rust Guidelines checklist and report violations with file:line references
---

Audit Rust source files against the Microsoft Pragmatic Rust Guidelines.
Full checklist with severity tiers is in `checklist.md` alongside this file — read it before starting.

## Determining scope

- If the user passes a path argument, audit that file only.
- If the user passes a directory, audit `.rs` files in that directory (non-recursive).
- If no argument is given, ask the user whether they want to audit a specific file or the full `src/` tree before proceeding. Do not silently audit all files.

## Reading files

Use the Read tool to load each file before auditing it. Do not audit from memory or partial context.

## Crate type

Check `Cargo.toml` for `[lib]` vs `[[bin]]` to determine if this is a library or application crate.

- **Application crate**: skip all `Library /` rules (M-ESCAPE-HATCHES, M-DONT-LEAK-TYPES, M-SIMPLE-ABSTRACTIONS, M-AVOID-WRAPPERS, M-DI-HIERARCHY, M-ERRORS-CANONICAL-STRUCTS, M-INIT-BUILDER, M-INIT-CASCADED, M-SERVICES-CLONE, M-IMPL-ASREF, M-IMPL-RANGEBOUNDS, M-IMPL-IO, M-ESSENTIAL-FN-INHERENT, M-MOCKABLE-SYSCALLS, M-NO-GLOB-REEXPORTS, M-OOBE, M-FEATURES-ADDITIVE, M-TYPES-SEND). These only apply to published library APIs.
- **Library crate**: apply all rules.

## Per-violation output

For each violation:
- The guideline ID and severity tier (e.g. `[critical] M-UNSAFE`)
- The file:line location
- A one-line explanation of what's wrong
- A concrete fix suggestion (targeted — don't rewrite the whole file)

Only flag real violations — don't invent issues. If a file is clean, say so in one line.

## Output format

Group findings by file, then by guideline ID:

```
src/main.rs
  [critical] M-UNSAFE (line 42): `unsafe` block has no SAFETY comment — add `// SAFETY: <reason>`
  [warning]  M-PUBLIC-DEBUG (line 10): `pub struct Foo` does not derive Debug

src/data.rs — clean
```

## Summary

At the end, print:
1. Violation counts by severity tier (`critical` / `warning` / `style`)
2. Violation counts by category (Universal, Safety, Docs, etc.)
3. All violations ranked by severity tier first, then frequency — no artificial cap
