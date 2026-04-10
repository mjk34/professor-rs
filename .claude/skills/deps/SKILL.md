---
name: deps
description: Audit Cargo dependencies for security vulnerabilities, maintenance health, and better alternatives
---

Audit the dependencies in `Cargo.toml` (and `Cargo.lock` if present). Run this for the project at the current path unless the user specifies otherwise.

**Step 1 — CVE scan**
Run `cargo audit`. Report each advisory with:
- Crate name and version
- CVE/RUSTSEC ID
- Severity and a one-line description of the vulnerability
- Whether a patched version exists and what it is

If `cargo audit` is not installed, note `cargo install cargo-audit` and skip to Step 2.

**Step 2 — Yanked or outdated versions**
Run `cargo outdated` if available (install: `cargo install cargo-outdated`). Flag:
- Any yanked crate versions in `Cargo.lock`
- Dependencies more than 2 major versions behind (potential security or compat debt)

**Step 3 — Health check (read Cargo.toml)**
For each direct dependency, evaluate:
- **Unpinned wildcards** (`version = "*"`) — flag these; they can silently pull in breaking or vulnerable versions
- **Unmaintained crates** — known unmaintained crates to flag: `openssl` (prefer `rustls`), `time` 0.1.x (superseded by `time` 0.3 or `chrono`), `failure` (superseded by `thiserror`/`anyhow`), `serde_cbor` (unmaintained, use `ciborium`)
- **Overly broad features** — if a crate is pulled in with `features = ["full"]` or similar and only a subset is needed, note it

**Step 4 — Alternatives (only flag if there's a clear win)**
Suggest a replacement only if:
- The current crate has an active CVE with no patch
- The current crate is officially unmaintained with a named successor
- There's a significantly safer or better-maintained alternative in the Rust ecosystem

Don't suggest rewrites or ecosystem-wide changes — one targeted suggestion per crate max.

**Output format:**
```
CVEs:        N found (X critical, Y moderate)
Yanked:      N crates
Outdated:    N crates flagged
Health:      N issues (unpinned versions, unmaintained crates)

Details:
[grouped by severity, with crate:version and action for each]
```

End with a prioritized action list: fix CVEs first, then yanked, then health issues.
