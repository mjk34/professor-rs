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
#![expect(clippy::format_push_string,          reason = "push_str(&format!(...)) is clearer than write!() for embed building")]
#![allow(clippy::redundant_else)]              // explicit else after always-continuing if is intentional for readability
#![allow(clippy::redundant_pub_crate)]         // pub(crate) documents intent even in private modules
#![allow(clippy::single_match_else)]           // match with one arm + else is clearer than if-let for early-return error patterns
mod basic;
mod clips;
mod data;
mod helper;
mod mods;
mod reminder;

use chrono::Datelike;
use dashmap::DashMap;
use data::VoiceUser;
use std::{env, sync::Arc};

#[doc(inline)] pub use poise::serenity_prelude as serenity;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, data::Data, Error>;

/// How often the maintenance task runs (12 h): saves data, checks birthdays.
const MAINTENANCE_INTERVAL_SECS: u64 = 60 * 60 * 12;
/// How often voice-channel members are checked for the time-in-voice reward.
const VOICE_REWARD_TICK_SECS: u64 = 30;

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
    let data = data::Data::load();

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
                    if let Err(e) = data::Data::check_or_create_user(ctx).await {
                        tracing::error!(error = %e, "pre_command: failed to send welcome message");
                    }
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
                background_task(users.clone(), voice_users);
                maintenance_task(users, http);
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

    let mut client = client.expect("failed to build Discord client");
    if let Err(e) = client.start().await {
        tracing::error!(error = %e, "Discord client stopped with an error");
        std::process::exit(1);
    }
}

#[expect(clippy::unused_async, reason = "poise requires async fn signature for event handlers")]
async fn event_handler(
    ctx: &serenity::Context,
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
            tracing::info!(
                user = %data_about_bot.user.name,
                utc = %now.format("%Y-%m-%d %H:%M:%S"),
                weekday = ?now.weekday(),
                "bot ready",
            );
        }

        serenity::FullEvent::Message { new_message } => {
            if new_message.author.id == ctx.cache.current_user().id {
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
    users: data::UsersMap,
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

                let now = chrono::Utc::now();

                // Decide who is due and stamp `last_reward` synchronously, then drop the
                // voice_users iterator — its shard locks must not be held across the awaits below.
                let due: Vec<_> = voice_users
                    .iter_mut()
                    .filter_map(|mut x| {
                        let (id, vu) = x.pair_mut();
                        let since = vu.last_reward.unwrap_or(vu.joined);
                        if (now - since).num_minutes() < CRED_TIME {
                            return None;
                        }
                        let user_arc = users.get(id).map(|e| Arc::clone(e.value()))?;
                        vu.last_reward = Some(now);
                        Some(user_arc)
                    })
                    .collect();

                for user_arc in due {
                    let mut user_data = user_arc.write().await;
                    user_data.add_creds(REWARD_CREDITS);
                    user_data.update_xp(REWARD_XP);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(VOICE_REWARD_TICK_SECS)).await;
        }
    });
}

fn maintenance_task(
    users: data::UsersMap,
    http: Arc<serenity::Http>,
) {
    tokio::spawn(async move {
        loop {
            reminder::check_birthday(&http).await;
            data::save_users(&users).await;
            tokio::time::sleep(std::time::Duration::from_secs(MAINTENANCE_INTERVAL_SECS)).await;
        }
    });
}
