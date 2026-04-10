# professor-rs

Discord bot in Rust. Stack: poise 0.6 over serenity, tokio, DashMap, tracing.

## Conventions
- Commands: `poise::Context<'a, data::Data, Error>` aliased `Context<'a>`; slash commands preferred
- State in `data::Data`; commands in own module files
- `pre_command`/`post_command` handle user init/persistence — don't duplicate inside commands
- `/buy`/`/sell` hidden from command list — enter via `/search`
- New command? Register in `commands: vec![...]` in `main.rs`
- `.env` holds token/channel IDs; build target: `x86_64-unknown-linux-musl` (vendored OpenSSL)

## Production Checks — Disabled for Testing
| Location | Guard |
|---|---|
| `src/stock/orders.rs:75,299` | `is_market_hours()` for order queuing in `/buy`/`/sell` |
| `src/professor.rs:274` | `is_market_open()` check before Professor trades |
| `src/basic/economy.rs` | `check_daily()` cooldown in `/uwu` command (active) and `simulate_uwu` |
| `src/main.rs:44` | `PROFESSOR_TRIGGER_HOUR_UTC` (19 → 17) |

## Gotchas
- `is_market_hours()` (`api.rs:154`) is sync, checks 9:30–4 PM ET — used for cache TTL and order queuing
- `is_market_open()` (`api.rs:639`) is async, hits Finnhub — used before Professor trades
- Float comparisons use `5e-5` epsilon throughout — don't use `==` on quantities
- Professor caches (`PULSE_CACHE`, `MIDDAY_CACHE`) are UTC-day scoped; `LAST_SESSION_DATE` prevents double-fire on restart
- `BuyModal`/`SellModal` manually implement the `Modal` trait with dynamic field labels — fragile, don't restructure without testing

## Tests
Unit tests in `trader/engine.rs`, `options/engine.rs`, `data.rs`, `helper.rs`. Run `cargo test` before changes to engine logic.

## Workflow
- Before pushing: run `/sanity` at minimum, `/preflight` for a full gate
- Skill ladder: `/sanity` → `/rust-polish` → `/preflight`

## Sensitive Files — Never Commit
`.env`, `.eventdb`, `data.json`
