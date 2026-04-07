mod basic;
mod clips;
mod data;
mod helper;
mod mods;
mod reminder;
mod stock;

use chrono::{Datelike, Timelike, Utc, Weekday};
use dashmap::DashMap;
use data::{UserData, VoiceUser};
use std::{env, sync::Arc};
use tokio::sync::RwLock;

pub use poise::serenity_prelude as serenity;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, data::Data, Error>;

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
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
                mods::test_seed_data(),
                mods::test_set_level(),
                stock::portfolio(),
                stock::search(),
                stock::buy(),
                stock::sell(),
                stock::watchlist(),
                stock::trades(),
                stock::options_quote(),
                stock::options_buy(),
                stock::options_sell(),
                stock::options_write(),
                stock::options_cover(),
                stock::professor(),
                stock::test_professor(),
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
                stock::refresh_market_rate(&data.hysa_fed_rate).await;
                stock::api_health_check().await;
                maintenance_task(users.clone(), http.clone(), hysa_rate, bot_chat.clone());

                // Seed Professor's UserData and start the daily AI trading task
                let bot_user_id = ctx.cache.current_user().id;
                data.bot_user_id = bot_user_id;

                let core_behavior = std::fs::read_to_string("MEMORY.txt").unwrap_or_else(|_| {
                    tracing::warn!("MEMORY.txt not found — using default core behavior");
                    "You are Professor, a Discord bot managing your own investment portfolio. \
                     Prefer diversified long-term holds. Only make HIGH conviction trades. \
                     Never exceed 30% of cash per trade. Maximum 3 trades per session.".to_string()
                });

                if !data.users.contains_key(&bot_user_id) {
                    let mut prof = data::UserData::default();
                    prof.add_creds(100_000);
                    prof.professor_memory = Some(data::ProfessorMemory {
                        core_behavior: core_behavior.clone(),
                        entries: std::collections::VecDeque::new(),
                    });
                    let port = data::Portfolio::new("ProfessorPort".to_string());
                    prof.stock.portfolios.push(port);
                    data.users.insert(bot_user_id, Arc::new(RwLock::new(prof)));
                } else {
                    // Refresh core_behavior from file on each restart
                    let u = data.users.get(&bot_user_id).unwrap();
                    let mut prof = u.write().await;
                    if let Some(mem) = prof.professor_memory.as_mut() {
                        mem.core_behavior = core_behavior;
                    }
                }

                professor_task(users, http, bot_chat, bot_user_id);
                Ok(data)
            })
        })
        .build();

    let client = serenity::Client::builder(&token, intents)
        .activity(serenity::ActivityData {
            name: "Coding Rust".to_string(),
            kind: serenity::ActivityType::Custom,
            state: Some("StonkBot - Testing".to_string()),
            url: None,
        })
        .status(serenity::OnlineStatus::Online)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}

async fn event_handler(
    _ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, data::Data, Error>,
    data: &data::Data,
) -> Result<(), Error> {
    let gen_chat = &data.gen_chat;
    let bot_chat = &data.bot_chat;
    let sub_chat = &data.sub_chat;
    let prof_id = &data.prof_id;

    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            let now = chrono::Utc::now();
            let next_run = next_professor_run(now);
            tracing::info!(
                "Logged in as {} | startup UTC: {} ({:?}) | professor next run: {} ({:?})",
                data_about_bot.user.name,
                now.format("%Y-%m-%d %H:%M:%S"),
                now.weekday(),
                next_run.format("%Y-%m-%d %H:%M:%S"),
                next_run.weekday(),
            );
        }

        serenity::FullEvent::Message { new_message } => {
            if new_message.author.id.to_string() == *prof_id {
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

                    let user_data = users.get_mut(id);
                    if user_data.is_none() {
                        continue;
                    }
                    let user_data = user_data.unwrap();

                    let should_reward = if let Some(last) = vu.last_reward {
                        (now - last).num_minutes() >= CRED_TIME
                    } else {
                        (now - joined).num_minutes() >= CRED_TIME
                    };

                    if should_reward {
                        let mut user_data = user_data.write().await;
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
            if today == 1 || today == 16 {
                stock::refresh_market_rate(&hysa_rate).await;
            }
            stock::apply_monthly_interest(&users, &hysa_rate).await;
            stock::sweep_expired_options(&users, &http, &bot_chat).await;
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 12)).await;
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
            let now = chrono::Utc::now();

            // Skip weekends
            if matches!(now.weekday(), Weekday::Sat | Weekday::Sun) {
                let days_until_mon: u64 = if now.weekday() == Weekday::Sat { 2 } else { 1 };
                let secs = days_until_mon * 86_400 + secs_until_hm(now.hour(), now.minute(), 19, 0);
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                continue;
            }

            // Fire at 19:00 UTC on weekdays (3:00 PM EDT)
            let secs_to_fire = secs_until_hm(now.hour(), now.minute(), 19, 0);
            if secs_to_fire == 0 {
                // Already past 17:00 today — sleep until tomorrow 17:00 and loop
                let elapsed = now.hour() as u64 * 3600 + now.minute() as u64 * 60 + now.second() as u64;
                let secs_to_next = 86_400 - elapsed + 19 * 3600;
                tokio::time::sleep(std::time::Duration::from_secs(secs_to_next)).await;
                continue;
            }
            tokio::time::sleep(std::time::Duration::from_secs(secs_to_fire)).await;

            if stock::is_market_open().await {
                stock::professor_daily_session(&users, &http, &bot_chat, bot_user_id).await;
            }

            // Sleep until tomorrow 19:00 UTC (3:00 PM EDT)
            let now2 = chrono::Utc::now();
            let elapsed = now2.hour() as u64 * 3600 + now2.minute() as u64 * 60 + now2.second() as u64;
            let secs_to_next = 86_400 - elapsed + 19 * 3600;
            tokio::time::sleep(std::time::Duration::from_secs(secs_to_next)).await;
        }
    });
}

/// Next weekday (Mon–Fri) at 17:00 UTC after `now`.
fn next_professor_run(now: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    let mut day = now.date_naive();
    if now.hour() >= 19 {
        day += chrono::Duration::days(1);
    }
    loop {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            break;
        }
        day += chrono::Duration::days(1);
    }
    day.and_hms_opt(19, 0, 0).unwrap().and_utc()
}

/// Seconds until the next occurrence of hour:minute UTC (0 if already past).
fn secs_until_hm(cur_h: u32, cur_m: u32, target_h: u32, target_m: u32) -> u64 {
    let cur_secs = cur_h as u64 * 3600 + cur_m as u64 * 60;
    let target_secs = target_h as u64 * 3600 + target_m as u64 * 60;
    if target_secs > cur_secs {
        target_secs - cur_secs
    } else {
        0
    }
}