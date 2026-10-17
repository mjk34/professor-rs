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
cargo zigbuild --release --target x86_64-unknown-linux-musl
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
- `/claim_bonus` — bonus creds every 3 daily rolls
- `/wallet` — view your creds, level, XP, and luck stats
- `/buy_tickets` — spend creds on Battle Pass raffle tickets
- `/leaderboard` — creds and rolling-fortune rankings
- Voice activity rewards — earn creds passively for time spent in voice channels (not while deafened)

### Clips & Submissions
- `/submit_clip` — submit a YouTube or Medal clip for Clip Night
- `/my_clips` — view and delete your submitted clips
- `/server_clips` — *(mod)* browse all server submissions and top-rated clips
- `/next_clip` — *(mod)* pull a random unrated clip for the room to vote on

### Server
- `/ping` — check the bot is alive
- `/info` — server stats
- `/voice_status` — who is in voice and for how long

### Moderation
- `/give_creds` / `/take_creds` — *(mod)* adjust creds for one or more mentioned users
- `/prune` — *(mod)* delete every message in the current channel (asks for confirmation; needs the bot to have Manage Messages)
- Birthday reminders from `.eventdb` — advance notice to the mod channel, day-of ping in general

Logging defaults to `info`; override with `RUST_LOG`.
