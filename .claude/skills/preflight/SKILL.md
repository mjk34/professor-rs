---
name: preflight
description: Ultimate pre-push gate — rust-polish, then parallel specialist subagents across async safety, API surface, refactoring signals, and test strategy, then rust-audit
---

## Phase 1 — Polish

Run `/rust-polish` first. If it fails at any step, stop and report — do not proceed to specialist phases.

**Security checks (always run after polish passes):**
- Run `git diff` and flag any `unsafe` blocks introduced or modified — note file:line, ask if intentional
- Flag `.unwrap()` on user input, external API responses, or env vars
- If `Cargo.toml` was modified, diff the dependency list and call out new crates or version unpins

---

## Phase 2 — Parallel Specialist Subagents

Once Phase 1 passes:

1. Use the Glob tool to list all `.rs` files in `src/`. Capture this as `FILE_LIST`.
2. Read each specialist file from `specialists/`. Prepend the following block to each prompt before spawning:

```
## Files to audit
Read only these files (already discovered — do not glob):
<FILE_LIST>
```

3. Spawn all four agents **simultaneously** in a single message using the Agent tool:
   - `specialists/async-runtime.md` (with FILE_LIST prepended)
   - `specialists/api-surface.md` (with FILE_LIST prepended)
   - `specialists/refactoring.md` (with FILE_LIST prepended)
   - `specialists/test-strategy.md` (with FILE_LIST prepended)

4. Wait for all four to complete.

---

## Phase 2.5 — Resolve Specialist Findings

Before running Phase 3, attempt to fix all `[critical]` and `[warning]` findings from Phase 2:

- Apply targeted fixes using the Edit tool — don't rewrite files, don't fix `[style]` findings at this stage
- After each fix, note the finding as resolved with the file:line it was at
- If a finding cannot be safely auto-fixed (e.g. requires design decisions), mark it as **deferred** with a reason
- Build a **resolved set**: all file:line locations that were fixed or intentionally deferred

---

## Phase 3 — Pragmatic Rust Audit

Run `/rust-audit src/`. Pass the resolved set as context and skip any finding whose file:line is already in it.

---

## Phase 3.5 — Resolve Audit Findings

Apply the same resolution process as Phase 2.5 to all `[critical]` and `[warning]` findings from Phase 3:

- Apply targeted fixes using the Edit tool
- Mark anything requiring design decisions as **deferred** with a reason
- Add Phase 3 resolutions to the resolved set

---

## Final Summary

```
### Async/Runtime
  [critical] src/professor.rs (line 88): blocking sleep inside async fn — FIXED

### API Surface ✓

### Refactoring
  [style] src/data.rs (line 210): 5-arg function — consider a config struct

### Test Strategy
  [warning] src/stock/orders.rs: no integration test for order queue — DEFERRED: requires mock design

### Pragmatic Audit
  [warning] M-LOG-STRUCTURED src/basic/economy.rs (line 44): string interpolation — FIXED
```

Then:
- Total findings by severity (`[critical]` / `[warning]` / `[style]`)
- All `[critical]` findings with their resolution status (fixed / deferred)
- Any deferred items with reasons — these are the remaining work before push

---

## Gotchas

**Phase 3.5 is not optional — do not wait for user input.**
After Phase 3 completes, run Phase 3.5 automatically. The skill prescribes it. Pausing to show Phase 3 results and asking "should I fix these?" is wrong — fix them, then show the final summary.

**Do not re-flag findings already in the resolved set.**
Phase 3 (rust-audit) must skip file:line locations already fixed or deferred in Phase 2.5. Forgetting to pass the resolved set causes duplicate findings in the final summary and redundant fix attempts.
