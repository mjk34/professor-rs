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
