---
name: cargo
description: Run Rust quality checks (check, clippy, test, audit) on the current project and report results
---

Note: `cargo check` runs automatically via hook on every file edit — skip straight to clippy.

Run the following cargo commands in the project root, in order. Stop and report if any step fails — don't silently continue past errors.

1. `cargo clippy -- -D warnings` — lints as errors; report each lint with file:line and the suggestion
2. `cargo test -- --nocapture` — run all tests; show output
3. `cargo audit` — scan dependencies for known CVEs via RustSec advisory DB; if not installed, note `cargo install cargo-audit` and skip rather than failing

After each step, briefly summarize what passed or failed. If clippy or tests fail, show the specific errors and suggest targeted fixes — don't rewrite code I didn't ask about.

**Security checks (always run, even on partial runs)**
- Flag any use of `unsafe` blocks introduced or modified in the current diff — note the file:line and ask if it's intentional
- Flag `.unwrap()` calls on types that cross a trust boundary (user input, external API responses, env vars) — these can panic in prod
- If `Cargo.toml` was modified, diff the dependency list and call out any new crates or version unpins; check `cargo audit` output specifically for those
- Never suggest disabling `cargo audit` or suppressing advisories without a documented reason

If the user passes an argument (e.g. `/cargo build`), run that specific cargo subcommand instead of the full suite, but still run the security checks above.

Cross-compile note: if the user asks to build for musl, use `cargo build --target x86_64-unknown-linux-musl --features vendored`.
