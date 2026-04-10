---
name: rust-polish
description: Full ordered Rust code quality pipeline — clippy all → clippy pedantic+nursery → simplify agents → test + audit → verify
---

Run the full Rust polish pipeline in order. Each phase must complete before the next starts. Do not skip phases even if a phase looks clean.

## Phase 1: `cargo clippy -- -W clippy::all`

Run `cargo clippy -- -W clippy::all 2>&1` and fix every warning. Prefer targeted edits; do not rewrite whole functions.

After fixing, re-run until the output is warning-free.

## Phase 2: `cargo clippy -- -W clippy::pedantic -W clippy::nursery`

Run `cargo clippy -- -W clippy::pedantic -W clippy::nursery 2>&1`.

For each warning, decide:
- **Fix it** if it's a real correctness or clarity issue.
- **Suppress it** with `#![allow(clippy::lint_name)]` at the crate root if it's intentional style noise. Every suppression must have an inline comment explaining *why* (e.g., concurrency model, intentional truncation, poise API requirement).

Common suppressions for this project (already in main.rs — do not re-add):
- `significant_drop_tightening` — DashMap Ref shard locks; acceptable contention in Discord bot
- `cast_precision_loss`, `cast_possible_truncation`, `cast_possible_wrap`, `cast_sign_loss` — bounded game-math casts
- `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate` — not a public API
- `wildcard_imports`, `items_after_statements`, `too_many_lines`, `manual_let_else` — project style
- `string_add`, `format_push_string` — embed building idiom
- `option_if_let_else`, `similar_names`, `redundant_else` — readability preference

Re-run until the output contains only expected/suppressed lints.

## Phase 3: Simplify / Agent Review

Invoke the `simplify` skill. This launches three parallel agents:
1. **Code Reuse** — find duplicate logic that could use existing helpers
2. **Code Quality** — flag hacky patterns: redundant state, parameter sprawl, copy-paste blocks, stringly-typed code, unnecessary comments
3. **Efficiency** — flag redundant work, missed concurrency, unbounded structures, unnecessary existence checks

Fix all real findings. Skip false positives with a one-line note.

## Phase 4: Test & Audit

1. `cargo test -- --nocapture` — all tests must pass; any failure is a hard stop
2. `cargo audit` — scan for CVEs; if not installed, note `cargo install cargo-audit` and skip

## Phase 5: Verify

Run `cargo build 2>&1` (or `cargo run` if the user wants a live smoke test).

The build must be warning-free and error-free. If warnings remain, return to the relevant phase and fix them before marking done.

---

Report at the end: one line per phase — what was found and fixed, or "clean".
