# professor-rs

Discord bot in Rust (poise 0.6 / serenity). See global `~/.claude/CLAUDE.md` for full stack and style conventions.

## Module Layout

| File | Purpose |
|---|---|
| `main.rs` | Bot setup, 4 background tasks: voice rewards, maintenance, professor, pending orders |
| `data.rs` | `Data`, `UserData`, `Portfolio`, `ProfessorMemory` — all shared state |
| `basic.rs` | Core commands: `/uwu`, `/wallet`, `/claim_bonus`, `/leaderboard`, `/buy_tickets`, `/voice_status` |
| `stock.rs` | Full trading system: `/buy`, `/sell`, `/search` (~2500 lines) |
| `trader.rs` | `/portfolio`, `/watchlist`, `/trades` |
| `options.rs` | `/options_quote`, `/options_buy`, `/options_sell`, `/options_write`, `/options_cover` |
| `professor.rs` | Claude-powered AI daily trading session |
| `api.rs` | Yahoo Finance, FRED, FMP, Finnhub, health checks, rate limit guard |
| `helper.rs` | Shared utilities |
| `mods.rs` | Mod commands: `/give_creds`, `/take_creds` |
| `clips.rs` | Clip submission/voting system |
| `reminder.rs` | Birthday reminders (reads `TZ_OFFSET_HOURS` from env) |

## Key Invariants

- `/buy` and `/sell` are registered but **hidden from the command list** — users enter trades through `/search`
- `MEMORY.txt` at repo root is Professor's core behavior prompt — read on every bot startup
- Professor fires daily at **19:00 UTC** (testing) — restore to **17:00 UTC** (1:00 PM EDT) for production
- Professor portfolio is always named `ProfessorPort`; funded once at 100k creds on first init
- Options use intrinsic value only (no Black-Scholes) — intentional, keeps it gamified

## Testing Guards — Must Re-enable Before Production

These are commented out for active testing. Uncomment all before merging to main or deploying:

| Location | What it guards |
|---|---|
| `src/stock.rs:644` | Market hours check for `/buy` |
| `src/stock.rs:789` | Market hours check for `/sell` |
| `src/professor.rs:257` | `let market_open = is_market_open().await;` |
| `src/professor.rs:327` | `if !market_open { ... }` |
| `src/basic.rs:994-995` | Daily cooldown for `/uwu` |
| `src/main.rs` | Professor trigger time (19:00 → 17:00 UTC) |

## Sensitive Files — Never Commit

- `.env` — bot token and channel IDs
- `.eventdb` — birthday data
- `data.json` — live user state

## Future: Website API

Planned axum HTTP layer. Two patterns:
1. **Mutation** — POST → bot acquires RwLock write on `DashMap` entry → calls existing methods → 200 OK
2. **Discord profile forwarding** — GET `/user/{id}/profile` → bot calls `guild.member(&http, id)` → returns avatar URL + display name

All mutation endpoints need `X-Secret` auth header. Bot is single source of truth — website never touches `data.json` directly.
