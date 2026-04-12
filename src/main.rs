//! Entry point: bot setup, task scheduling, and event dispatch.
// Pedantic/nursery lints that are either intentional or not worth the churn:
#![expect(clippy::significant_drop_tightening, reason = "DashMap Ref shard locks; contention is acceptable in this bot's concurrency model")]
#![expect(clippy::cast_precision_loss,         reason = "integer → f64 casts in game math are fine")]
#![expect(clippy::cast_possible_truncation,    reason = "f64 → i32 truncation in cred calculations is intentional")]
#![expect(clippy::cast_possible_wrap,          reason = "small bounded values cast to i32 are safe")]
#![expect(clippy::cast_sign_loss,              reason = "cred/ticket values are always non-negative at cast sites")]
#![allow(clippy::missing_errors_doc)]          // not publishing a public API
#![allow(clippy::missing_panics_doc)]          // not publishing a public API
#![allow(clippy::must_use_candidate)]          // Discord command return values are always awaited by poise
#![allow(clippy::wildcard_imports)]            // poise/serenity re-exports are intentionally glob-imported
#![expect(clippy::items_after_statements,      reason = "nested helper fns after setup statements are intentional")]
#![expect(clippy::too_many_lines,              reason = "command dispatch functions are inherently long")]
#![expect(clippy::manual_let_else,             reason = "let-else is too invasive a restructure for existing match/if patterns")]
#![expect(clippy::string_add,                  reason = "push_str(&format!(...)) is idiomatic for embed building")]
#![expect(clippy::option_if_let_else,          reason = "map_or_else restructure reduces readability in complex closures")]
#![expect(clippy::similar_names,               reason = "short variable names (u, ud, etc.) are conventional in this codebase")]
#![expect(clippy::format_push_string,          reason = "push_str(&format!(...)) is clearer than write!() for embed building")]
#![allow(clippy::redundant_else)]              // explicit else after always-continuing if is intentional for readability
#![allow(clippy::redundant_pub_crate)]         // pub(crate) documents intent even in private modules
#![allow(clippy::single_match_else)]           // match with one arm + else is clearer than if-let for early-return error patterns
mod api;
mod basic;
mod clips;
mod data;
mod helper;
mod mods;
mod options;
mod professor;
mod reminder;
mod stock;
mod trader;
mod web;

use chrono::{Datelike, Timelike, Utc, Weekday};
use dashmap::DashMap;
use data::{UserData, VoiceUser};
use std::{env, sync::Arc};
use tokio::sync::RwLock;

#[doc(inline)] pub use poise::serenity_prelude as serenity;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, data::Data, Error>;

/// Professor daily session trigger time (UTC hour). 17 = 1 PM EDT (production); 19 for testing.
const PROFESSOR_TRIGGER_HOUR_UTC: u32 = 17;
/// Days of month on which the HYSA fed rate is refreshed from FRED (1st and 16th = semi-monthly).
const INTEREST_REFRESH_DAYS: &[u32] = &[1, 16];
/// How often the maintenance task runs (12 h): saves data, checks birthdays, sweeps expired options.
const MAINTENANCE_INTERVAL_SECS: u64 = 60 * 60 * 12;
/// How often pending orders are checked against live prices (30 min — within one market tick cycle).
const ORDER_SWEEP_INTERVAL_SECS: u64 = 60 * 30;

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::new("info,serenity::gateway=off"),
        )
        .init();
    dotenvy::dotenv().expect("Failed to read .env file");
    let token = env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let mut data = data::Data::load();

    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::GUILD_MESSAGE_REACTIONS
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            // Check and create a user account before each command
            pre_command: |ctx: Context<'_>| {
                Box::pin(async move {
                    data::Data::check_or_create_user(ctx).await.unwrap();
                })
            },
            // Save all data after running a command
            post_command: |ctx: Context<'_>| {
                Box::pin(async move {
                    ctx.data().save().await;
                })
            },
            commands: vec![
                register(),
                basic::ping(),
                basic::uwu(),
                basic::wallet(),
                basic::claim_bonus(),
                basic::voice_status(),
                basic::info(),
                basic::buy_tickets(),
                basic::leaderboard(),
                clips::submit_clip(),
                clips::server_clips(),
                clips::my_clips(),
                clips::next_clip(),
                mods::give_creds(),
                mods::take_creds(),
                trader::portfolio(),
                stock::search(),
                // /buy and /sell hidden — users go through /search interface
                // stock::buy(),
                // stock::sell(),
                trader::watchlist(),
                trader::trades(),
                options::options_quote(),
                options::options_buy(),
                options::options_sell(),
                options::options_write(),
                options::options_cover(),
                professor::professor(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                ..Default::default()
            },
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, _framework| {
            let http = ctx.http.clone();
            Box::pin(async move {
                let users = data.users.clone();
                let voice_users = data.voice_users.clone();
                let hysa_rate = data.hysa_fed_rate.clone();
                let bot_chat = data.bot_chat.clone();
                background_task(users.clone(), voice_users);
                api::refresh_market_rate(&data.hysa_fed_rate).await;
                api::api_health_check().await;
                maintenance_task(users.clone(), http.clone(), hysa_rate, bot_chat.clone());

                // Seed Professor's UserData and start the daily AI trading task
                let bot_user_id = ctx.cache.current_user().id;
                data.bot_user_id = bot_user_id;

                let core_behavior = tokio::fs::read_to_string("MEMORY.txt").await.unwrap_or_else(|_| {
                    tracing::warn!("MEMORY.txt not found — using default core behavior");
                    "You are Professor, a Discord bot managing your own investment portfolio. \
                     Prefer diversified long-term holds. Only make HIGH conviction trades. \
                     Never exceed 30% of cash per trade. Maximum 3 trades per session.".to_string()
                });

                if data.users.contains_key(&bot_user_id) {
                    // Refresh core_behavior from file on each restart
                    let u = data.users.get(&bot_user_id).unwrap();
                    let mut prof = u.write().await;
                    if let Some(mem) = prof.professor_memory.as_mut() {
                        mem.core_behavior = core_behavior;
                    }
                } else {
                    let mut prof = data::UserData::default();
                    prof.add_creds(data::NEW_USER_STARTING_CREDS);
                    prof.professor_memory = Some(data::ProfessorMemory {
                        core_behavior: core_behavior.clone(),
                        entries: std::collections::VecDeque::new(),
                    });
                    let port = data::Portfolio::new("ProfessorPort".to_string());
                    prof.stock.portfolios.push(port);
                    data.users.insert(bot_user_id, Arc::new(RwLock::new(prof)));
                }

                professor_task(users.clone(), http.clone(), bot_chat.clone(), bot_user_id);
                pending_orders_task(users.clone(), http, bot_chat);

                // HTTP API for uwuwebu frontend
                let web_port: u16 = env::var("UWUWEBU_BOT_PORT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(4875);
                let web_router = web::router(users);
                tokio::spawn(async move {
                    let listener = tokio::net::TcpListener::bind(("127.0.0.1", web_port))
                        .await
                        .expect("failed to bind web API port");
                    tracing::info!(port = web_port, "web API listening");
                    axum::serve(listener, web_router).await.ok();
                });

                Ok(data)
            })
        })
        .build();

    let client = serenity::Client::builder(&token, intents)
        .activity(serenity::ActivityData {
            name: "Coding Rust".to_string(),
            kind: serenity::ActivityType::Custom,
            state: Some("Use /uwu to claim your daily creds! #gamba".to_string()),
            url: None,
        })
        .status(serenity::OnlineStatus::Online)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}

#[expect(clippy::unused_async, reason = "poise requires async fn signature for event handlers")]
async fn event_handler(
    _ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, data::Data, Error>,
    data: &data::Data,
) -> Result<(), Error> {
    let gen_chat = &data.gen_chat;
    let bot_chat = &data.bot_chat;
    let sub_chat = &data.sub_chat;

    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            let now = chrono::Utc::now();
            let next_run = next_professor_run(now);
            let est_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5); adjust to -4 during EDT
            let next_run_est = next_run.with_timezone(&est_offset);
            tracing::info!(
                user = %data_about_bot.user.name,
                utc = %now.format("%Y-%m-%d %H:%M:%S"),
                weekday = ?now.weekday(),
                "bot ready",
            );
            tracing::info!(
                date = %next_run_est.format("%Y-%m-%d"),
                time_est = %next_run_est.format("%I:%M %p"),
                weekday = ?next_run_est.weekday(),
                "professor next daily run",
            );
        }

        serenity::FullEvent::Message { new_message } => {
            if new_message.author.id == data.bot_user_id {
                return Ok(());
            }

            let channel_id = new_message.channel_id.get().to_string();
            if channel_id != *gen_chat && channel_id != *bot_chat && channel_id != *sub_chat {
                return Ok(());
            }

            // GPT reply and doodle disabled until local LLM is ready
        }

        serenity::FullEvent::VoiceStateUpdate { old: _, new } => {
            let voice_users = &data.voice_users;

            // Someone left the channel
            if new.channel_id.is_none() {
                voice_users.remove(&new.user_id);
                return Ok(());
            }

            let mut user = voice_users
                .entry(new.user_id)
                .or_insert(data::VoiceUser::new());
            user.update_mute(new.self_mute || new.mute);
            user.update_deaf(new.self_deaf || new.deaf);
        }
        _ => {}
    }
    Ok(())
}

fn background_task(
    users: Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
    voice_users: Arc<DashMap<serenity::UserId, VoiceUser>>,
) {
    tokio::spawn(async move {
        loop {
            {
                // How long should someone be in voice for creds
                const CRED_TIME: i64 = 30;
                // How much creds to award
                const REWARD_CREDITS: i32 = 50;
                // How much xp to award
                const REWARD_XP: i32 = 30;

                // Check time
                let now = chrono::Utc::now();

                for mut x in voice_users.iter_mut() {
                    let (id, vu) = x.pair_mut();
                    let joined = vu.joined;

                    // Clone the Arc out of the DashMap so the shard lock is released
                    // before any .await below — prevents stalling the users map.
                    let Some(user_arc) = users.get(id).map(|e| Arc::clone(e.value())) else {
                        continue;
                    };

                    let should_reward = if let Some(last) = vu.last_reward {
                        (now - last).num_minutes() >= CRED_TIME
                    } else {
                        (now - joined).num_minutes() >= CRED_TIME
                    };

                    if should_reward {
                        let mut user_data = user_arc.write().await;
                        user_data.add_creds(REWARD_CREDITS);
                        user_data.update_xp(REWARD_XP);
                        vu.last_reward = Some(now);
                    }
                }
            }
            // Sleep for a while before the next iteration
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

fn maintenance_task(
    users: Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
    http: Arc<serenity::Http>,
    hysa_rate: Arc<RwLock<f64>>,
    bot_chat: String,
) {
    tokio::spawn(async move {
        loop {
            reminder::check_birthday(&http).await;
            data::save_users(&users).await;
            let today = chrono::Utc::now().day();
            if INTEREST_REFRESH_DAYS.contains(&today) {
                api::refresh_market_rate(&hysa_rate).await;
            }
            api::apply_monthly_interest(&users, &hysa_rate).await;
            api::sweep_expired_options(&users, &http, &bot_chat).await;
            tokio::time::sleep(std::time::Duration::from_secs(MAINTENANCE_INTERVAL_SECS)).await;
        }
    });
}

fn professor_task(
    users: Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
    http: Arc<serenity::Http>,
    bot_chat: String,
    bot_user_id: serenity::UserId,
) {
    tokio::spawn(async move {
        loop {
            // Sleep until the next trigger hour — always sleep first to avoid the boundary
            // case where waking at the exact trigger time causes secs_until_hm to return 0 and skip.
            let now = chrono::Utc::now();
            let cur_secs = u64::from(now.hour()) * 3600 + u64::from(now.minute()) * 60 + u64::from(now.second());
            let target_secs = u64::from(PROFESSOR_TRIGGER_HOUR_UTC) * 3600;
            let secs_to_fire = if cur_secs < target_secs {
                target_secs - cur_secs
            } else {
                86_400 - cur_secs + target_secs
            };
            tokio::time::sleep(std::time::Duration::from_secs(secs_to_fire)).await;

            // Skip weekends — loop again so we sleep to the next 19:00
            let now = chrono::Utc::now();
            if matches!(now.weekday(), Weekday::Sat | Weekday::Sun) {
                continue;
            }

            if api::is_market_open().await {
                professor::professor_daily_session(&users, &http, &bot_chat, bot_user_id).await;
            }
        }
    });
}

fn pending_orders_task(
    users: Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
    http: Arc<serenity::Http>,
    bot_chat: String,
) {
    tokio::spawn(async move {
        loop {
            if api::is_market_hours() {
                api::sweep_pending_orders(&users, &http, &bot_chat).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(ORDER_SWEEP_INTERVAL_SECS)).await;
        }
    });
}

/// Next weekday (Mon–Fri) at `PROFESSOR_TRIGGER_HOUR_UTC` after `now`.
fn next_professor_run(now: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    let mut day = now.date_naive();
    if now.hour() >= PROFESSOR_TRIGGER_HOUR_UTC {
        day += chrono::Duration::days(1);
    }
    loop {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            break;
        }
        day += chrono::Duration::days(1);
    }
    day.and_hms_opt(PROFESSOR_TRIGGER_HOUR_UTC, 0, 0).unwrap().and_utc()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, mo: u32, d: u32, h: u32, m: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, m, 0).unwrap()
    }

    const H: u32 = PROFESSOR_TRIGGER_HOUR_UTC;

    #[test]
    fn next_run_same_day_before_19() {
        // Monday 09:00 — should fire same day at trigger hour
        let result = next_professor_run(utc(2026, 4, 6, 9, 0));
        assert_eq!(result, utc(2026, 4, 6, H, 0));
    }

    #[test]
    fn next_run_same_day_after_19() {
        // Monday 20:00 — should fire Tuesday at trigger hour
        let result = next_professor_run(utc(2026, 4, 6, 20, 0));
        assert_eq!(result, utc(2026, 4, 7, H, 0));
    }

    #[test]
    fn next_run_skips_weekend() {
        // Friday 20:00 — should skip Sat/Sun, fire Monday at trigger hour
        let result = next_professor_run(utc(2026, 4, 10, 20, 0));
        assert_eq!(result, utc(2026, 4, 13, H, 0));
    }

    #[test]
    fn next_run_saturday_before_19() {
        // Saturday 10:00 — should fire Monday at trigger hour
        let result = next_professor_run(utc(2026, 4, 11, 10, 0));
        assert_eq!(result, utc(2026, 4, 13, H, 0));
    }
}