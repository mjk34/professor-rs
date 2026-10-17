# professor-rs

Discord bot in Rust. Stack: poise 0.6 over serenity, tokio, DashMap, tracing.

## Conventions
- Commands: `poise::Context<'a, data::Data, Error>` aliased `Context<'a>`; slash commands preferred
- State in `data::Data`; commands in own module files
- `pre_command`/`post_command` handle user init/persistence — don't duplicate inside commands
- New command? Register in `commands: vec![...]` in `main.rs`
- `.env` holds token/channel IDs; build target: `x86_64-unknown-linux-musl`

## Production Guards
| Location | Guard |
|---|---|
| `src/basic/economy.rs` | `check_daily()` cooldown in `/uwu` |

Disable temporarily for testing; re-enable before pushing.

## Gotchas
- `data.json` is deserialized into `SaveData`; unknown fields are ignored, so removing a `UserData` field drops that data on the next save

## Tests
Unit tests in `data.rs`, `helper.rs`. Run `cargo test` before changes to economy/level logic.

## Workflow
- Before pushing: run `/sanity` at minimum, `/preflight` for a full gate
- Skill ladder: `/sanity` → `/rust-polish` → `/preflight`

## Sensitive Files — Never Commit
`.env`, `.eventdb`, `data.json`
