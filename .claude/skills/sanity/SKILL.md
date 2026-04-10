---
name: sanity
description: Quick sanity check — clippy, test, audit. Lightweight alternative to /preflight.
---

Run in order. Stop and report on the first failure — don't continue past errors.

1. `cargo clippy -- -D warnings` — report each lint with file:line and fix suggestion
2. `cargo test -- --nocapture` — show all output; any failure is a hard stop
3. `cargo audit` — scan for CVEs; if not installed, note `cargo install cargo-audit` and skip

If the user passes an argument (e.g. `/cargo build`), run that specific subcommand instead of the suite.

Cross-compile note: if the user asks to build for musl, use `cargo build --target x86_64-unknown-linux-musl --features vendored`.

---

## Gotchas

**`-D warnings` turns all warnings into errors — do not suggest suppressing them here.**
Sanity is a quick pass, not a polish pass. If clippy fails, report the lint and a fix suggestion. Don't add `#![allow(...)]` suppressions; that belongs in `/rust-polish` Phase 2 where suppressions are deliberate and documented.
