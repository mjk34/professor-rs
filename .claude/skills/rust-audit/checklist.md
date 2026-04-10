# Microsoft Pragmatic Rust Guidelines — Checklist Reference

Source: https://microsoft.github.io/rust-guidelines/guidelines/checklist/index.html

## Severity Tiers

- `[critical]` — Correctness, soundness, or security risk. Fix before merging.
- `[warning]`  — Maintainability or API contract issue. Fix soon.
- `[style]`    — Conventions and polish. Fix opportunistically.

---

## Universal
- `[warning]`  **M-UPSTREAM-GUIDELINES** — Follow Rust API Guidelines and std conventions (naming, trait impls, etc.)
- `[warning]`  **M-STATIC-VERIFICATION** — Use `cargo check`, `clippy`, and `cargo audit`; no suppressed warnings without `#[expect]`
- `[warning]`  **M-LINT-OVERRIDE-EXPECT** — Use `#[expect(lint_name)]` instead of `#[allow(lint_name)]` so it errors when the lint is no longer triggered
- `[style]`    **M-PUBLIC-DEBUG** — All `pub` types must `derive(Debug)` or implement `Debug`
- `[style]`    **M-PUBLIC-DISPLAY** — `pub` types meant for end-user display should implement `Display`
- `[style]`    **M-SMALLER-CRATES** — If a module is getting large or has orthogonal concerns, flag it as a candidate to split
- `[style]`    **M-CONCISE-NAMES** — No weasel words: `Manager`, `Handler`, `Helper`, `Util`, `Data`, `Info`, `Processor` in type/function names without clear justification
- `[style]`    **M-REGULAR-FN** — Prefer free functions over associated functions when `self` isn't needed
- `[critical]` **M-PANIC-IS-STOP** — `panic!` must mean unrecoverable; don't use it for expected error paths
- `[warning]`  **M-PANIC-ON-BUG** — Use `panic!`/`assert!` for programmer bugs (invariant violations), not for runtime errors that should be `Result`
- `[warning]`  **M-DOCUMENTED-MAGIC** — Every magic constant, timeout, limit, or default behavior must have a doc comment explaining the value
- `[warning]`  **M-LOG-STRUCTURED** — Use structured logging with message templates (e.g. `tracing::info!(user_id = %id, "action")`) — no string interpolation in log messages

## Library / Interoperability *(skip for application crates)*
- `[warning]`  **M-TYPES-SEND** — Public types should be `Send + Sync` unless there's an explicit reason not to be
- `[style]`    **M-ESCAPE-HATCHES** — Provide `as_raw` / `into_raw` / `from_raw` escape hatches for types wrapping OS or FFI handles
- `[warning]`  **M-DONT-LEAK-TYPES** — Don't expose external/third-party types in your public API surface

## Library / UX *(skip for application crates)*
- `[warning]`  **M-SIMPLE-ABSTRACTIONS** — Abstractions shouldn't visibly nest (e.g. `Arc<Mutex<HashMap<K, Vec<V>>>>` in a public API)
- `[warning]`  **M-AVOID-WRAPPERS** — Avoid `Arc`, `Mutex`, `Box` etc. in public API signatures; internalize them
- `[style]`    **M-DI-HIERARCHY** — Prefer concrete types > generics > `dyn Trait`; use `dyn` only when necessary
- `[warning]`  **M-ERRORS-CANONICAL-STRUCTS** — Errors should be structs with named fields, not tuple structs or plain strings
- `[style]`    **M-INIT-BUILDER** — Complex types (>3 optional fields) should use the builder pattern
- `[style]`    **M-INIT-CASCADED** — Builder hierarchies should cascade: parent builder creates child builders
- `[style]`    **M-SERVICES-CLONE** — Service types (shared across tasks/threads) should implement `Clone` cheaply (e.g. wrap in `Arc` internally)
- `[style]`    **M-IMPL-ASREF** — Accept `impl AsRef<str>` / `impl AsRef<Path>` instead of `&str` / `&Path` where callers might hold owned types
- `[style]`    **M-IMPL-RANGEBOUNDS** — Accept `impl RangeBounds<T>` for any function taking a range
- `[style]`    **M-IMPL-IO** — Accept `impl Read` / `impl Write` instead of concrete I/O types (sans-IO design)
- `[warning]`  **M-ESSENTIAL-FN-INHERENT** — Core functionality should be inherent methods, not only available via a trait

## Library / Resilience *(skip for application crates)*
- `[warning]`  **M-MOCKABLE-SYSCALLS** — I/O and system calls should be behind a trait so they can be mocked in tests
- `[critical]` **M-TEST-UTIL** — Test helpers and fixtures must be behind `#[cfg(test)]` or a `test-utils` feature flag — never compiled into release
- `[warning]`  **M-STRONG-TYPES** — Use the proper type family; don't use `String` where a newtype or enum is more appropriate
- `[warning]`  **M-NO-GLOB-REEXPORTS** — No `pub use foo::*`; re-export items explicitly
- `[warning]`  **M-AVOID-STATICS** — Avoid `static` and `lazy_static!`/`OnceLock` for mutable shared state; prefer passing state explicitly

## Library / Building *(skip for application crates)*
- `[style]`    **M-OOBE** — Libraries should work with `cargo add` and no extra setup
- `[warning]`  **M-FEATURES-ADDITIVE** — Cargo features must be purely additive; enabling a feature must not remove or break existing behavior

## Applications
- `[style]`    **M-APP-ERROR** — Apps may use `anyhow` or similar for error propagation; libraries should not

## Safety
- `[critical]` **M-UNSAFE** — Every `unsafe` block must have a `// SAFETY:` comment explaining why it's sound
- `[critical]` **M-UNSAFE-IMPLIES-UB** — Flag any `unsafe` that could trigger undefined behavior without a clear soundness argument
- `[critical]` **M-UNSOUND** — All code must be sound; flag any pattern that could cause unsoundness (e.g. transmuting unvalidated data, incorrect lifetime annotations)

## Performance
- `[warning]`  **M-THROUGHPUT** — No busy-wait loops or empty `tokio::spawn` cycles; every task should do real work
- `[critical]` **M-YIELD-POINTS** — Long-running async tasks must have yield points (`.await`) to avoid starving the executor

## Documentation
- `[style]`    **M-FIRST-DOC-SENTENCE** — First doc comment sentence must be a single line, ~15 words max
- `[style]`    **M-MODULE-DOCS** — Every module (`mod foo`) should have a `//!` module-level doc comment
- `[style]`    **M-CANONICAL-DOCS** — Doc comments should follow canonical sections: summary, Errors, Panics, Examples
- `[style]`    **M-DOC-INLINE** — `pub use` re-exports must use `#[doc(inline)]`
