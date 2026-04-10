---
name: rust-audit
description: Audit Rust code against the Microsoft Pragmatic Rust Guidelines checklist and report violations with file:line references
---

Audit the specified file(s) or the entire `src/` directory against the Microsoft Pragmatic Rust Guidelines (https://microsoft.github.io/rust-guidelines/guidelines/checklist/index.html).

If the user passes a path argument, audit that file. Otherwise audit all `.rs` files in `src/`.

For each violation found, output:
- The guideline ID (e.g. `M-UNSAFE`)
- The rule name
- The file:line location
- A one-line explanation of what's wrong
- A concrete fix suggestion (targeted — don't rewrite the whole file)

Only flag real violations — don't invent issues. If a section is clean, say so in one line.

---

## Checklist Reference

### Universal
- **M-UPSTREAM-GUIDELINES** — Follow Rust API Guidelines and std conventions (naming, trait impls, etc.)
- **M-STATIC-VERIFICATION** — Use `cargo check`, `clippy`, and `cargo audit`; no suppressed warnings without `#[expect]`
- **M-LINT-OVERRIDE-EXPECT** — Use `#[expect(lint_name)]` instead of `#[allow(lint_name)]` so it errors when the lint is no longer triggered
- **M-PUBLIC-DEBUG** — All `pub` types must `derive(Debug)` or implement `Debug`
- **M-PUBLIC-DISPLAY** — `pub` types meant for end-user display should implement `Display`
- **M-SMALLER-CRATES** — If a module is getting large or has orthogonal concerns, flag it as a candidate to split
- **M-CONCISE-NAMES** — No weasel words: `Manager`, `Handler`, `Helper`, `Util`, `Data`, `Info`, `Processor` in type/function names without clear justification
- **M-REGULAR-FN** — Prefer free functions over associated functions when `self` isn't needed
- **M-PANIC-IS-STOP** — `panic!` must mean unrecoverable; don't use it for expected error paths
- **M-PANIC-ON-BUG** — Use `panic!`/`assert!` for programmer bugs (invariant violations), not for runtime errors that should be `Result`
- **M-DOCUMENTED-MAGIC** — Every magic constant, timeout, limit, or default behavior must have a doc comment explaining the value
- **M-LOG-STRUCTURED** — Use structured logging with message templates (e.g. `tracing::info!(user_id = %id, "action")`) — no string interpolation in log messages

### Library / Interoperability
- **M-TYPES-SEND** — Public types should be `Send + Sync` unless there's an explicit reason not to be
- **M-ESCAPE-HATCHES** — Provide `as_raw` / `into_raw` / `from_raw` escape hatches for types wrapping OS or FFI handles
- **M-DONT-LEAK-TYPES** — Don't expose external/third-party types in your public API surface

### Library / UX
- **M-SIMPLE-ABSTRACTIONS** — Abstractions shouldn't visibly nest (e.g. `Arc<Mutex<HashMap<K, Vec<V>>>>` in a public API)
- **M-AVOID-WRAPPERS** — Avoid `Arc`, `Mutex`, `Box` etc. in public API signatures; internalize them
- **M-DI-HIERARCHY** — Prefer concrete types > generics > `dyn Trait`; use `dyn` only when necessary
- **M-ERRORS-CANONICAL-STRUCTS** — Errors should be structs with named fields, not tuple structs or plain strings
- **M-INIT-BUILDER** — Complex types (>3 optional fields) should use the builder pattern
- **M-INIT-CASCADED** — Builder hierarchies should cascade: parent builder creates child builders
- **M-SERVICES-CLONE** — Service types (shared across tasks/threads) should implement `Clone` cheaply (e.g. wrap in `Arc` internally)
- **M-IMPL-ASREF** — Accept `impl AsRef<str>` / `impl AsRef<Path>` instead of `&str` / `&Path` where callers might hold owned types
- **M-IMPL-RANGEBOUNDS** — Accept `impl RangeBounds<T>` for any function taking a range
- **M-IMPL-IO** — Accept `impl Read` / `impl Write` instead of concrete I/O types (sans-IO design)
- **M-ESSENTIAL-FN-INHERENT** — Core functionality should be inherent methods, not only available via a trait

### Library / Resilience
- **M-MOCKABLE-SYSCALLS** — I/O and system calls should be behind a trait so they can be mocked in tests
- **M-TEST-UTIL** — Test helpers and fixtures must be behind `#[cfg(test)]` or a `test-utils` feature flag — never compiled into release
- **M-STRONG-TYPES** — Use the proper type family; don't use `String` where a newtype or enum is more appropriate
- **M-NO-GLOB-REEXPORTS** — No `pub use foo::*`; re-export items explicitly
- **M-AVOID-STATICS** — Avoid `static` and `lazy_static!`/`OnceLock` for mutable shared state; prefer passing state explicitly

### Library / Building
- **M-OOBE** — Libraries should work with `cargo add` and no extra setup
- **M-FEATURES-ADDITIVE** — Cargo features must be purely additive; enabling a feature must not remove or break existing behavior

### Applications
- **M-APP-ERROR** — Apps may use `anyhow` or similar for error propagation; libraries should not

### Safety
- **M-UNSAFE** — Every `unsafe` block must have a `// SAFETY:` comment explaining why it's sound
- **M-UNSAFE-IMPLIES-UB** — Flag any `unsafe` that could trigger undefined behavior without a clear soundness argument
- **M-UNSOUND** — All code must be sound; flag any pattern that could cause unsoundness (e.g. transmuting unvalidated data, incorrect lifetime annotations)

### Performance
- **M-THROUGHPUT** — No busy-wait loops or empty `tokio::spawn` cycles; every task should do real work
- **M-YIELD-POINTS** — Long-running async tasks must have yield points (`.await`) to avoid starving the executor

### Documentation
- **M-FIRST-DOC-SENTENCE** — First doc comment sentence must be a single line, ~15 words max
- **M-MODULE-DOCS** — Every module (`mod foo`) should have a `//!` module-level doc comment
- **M-CANONICAL-DOCS** — Doc comments should follow canonical sections: summary, Errors, Panics, Examples
- **M-DOC-INLINE** — `pub use` re-exports must use `#[doc(inline)]`

---

## Output Format

For each file, group findings by guideline ID. Example:

```
src/main.rs
  M-UNSAFE (line 42): `unsafe` block has no SAFETY comment — add `// SAFETY: <reason>`
  M-PUBLIC-DEBUG (line 10): `pub struct Foo` does not derive Debug

src/data.rs — clean
```

At the end, print a summary: total violations by category (Universal, Safety, Docs, etc.) and the top 3 highest-priority fixes.
