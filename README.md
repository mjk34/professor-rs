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
- `/claim` — bonus creds every 3 daily rolls
- `/wallet` — view your creds, level, XP, and luck stats
- Voice activity rewards — earn creds passively for time spent in voice channels

### Clips & Submissions
- `/submit` — submit a game clip for Clip Night
- `/my_clips` — view your submitted clips
- `/server_clips` — browse all server submissions
- `/next_clip` — pull a random unrated clip for review
