# professorBot

## Summary

This is the code behind Uwuversity's Professor bot.

Professor bot is a developing agent used to incentivise members with rewarding and interactive features to improve participation. Members can accumulate creds daily, submit game clips for Clip Night, and buy tickets for ongoing raffles.

---

## Build

Static bin:

One of the easier ways is to install `zig` and `cargo zigbuild`

```
pacman -S zig
```

```
cargo install --locked cargo-zigbuild
```


```
cargo zigbuild build --release --target x86_64-unknown-linux-musl --features vendored
```

---

## Ascent

A three-stage quality gate that carries code from build verification through idiomatic 
cleanup to a full pre-merge audit, applying fixes inline and leaving the working tree in 
a shippable state.

Claude Code Run Skills: `/sanity` → `/rust-polish` → `/preflight` → push code

---

## Features

### Economy
- `/uwu` — claim your daily creds roll (21h cooldown, d20-based rewards)
- `/claim` — bonus creds every 3 daily rolls
- `/wallet` — view your creds, level, XP, and luck stats
- Voice activity rewards — earn creds passively for time spent in voice channels

### Stock & Portfolio Trading
Members can build and manage investment portfolios using uwu creds as currency.

- `/portfolio` — create, view, fund, withdraw from, and delete portfolios
- `/buy` / `/sell` — buy and sell stocks, ETFs, and crypto by share count or dollar amount
- `/search` — look up any ticker with live price data and market info
- `/watchlist` — track tickers you're watching
- `/trades` — view your recent trade history
- HYSA interest — uninvested cash earns interest; Gold Status (Level 10+) earns a higher rate

### Options Trading
Full simulated options system with covered calls and cash-secured puts.

- `/options_quote` — check the current premium for any contract
- `/options_buy` / `/options_sell` — open and close long positions
- `/options_write` / `/options_cover` — write and close short positions
- Automatic expiry settlement via daily sweep — ITM positions pay out, OTM expire worthless

### Professor Portfolio
Professor (the bot itself) manages its own portfolio using Claude AI as its trading brain.

- Runs a daily session Mon–Fri: market news briefing → position scoring → trade execution
- Trades using a macro-first, sector-rotation strategy informed by live Finnhub news headlines
- Maintains a rolling 7-day memory of market observations and trade rationale
- Posts a daily summary embed to the bot channel with income, trades made, and portfolio state
- `/professor` — view Professor's current portfolio and latest market thoughts

### Clips & Submissions
- `/submit` — submit a game clip for Clip Night
- `/my_clips` — view your submitted clips
- `/server_clips` — browse all server submissions
- `/next_clip` — pull a random unrated clip for review
