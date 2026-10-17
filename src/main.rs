//! Entry point: bot setup, task scheduling, and event dispatch.
// Pedantic/nursery lints that are either intentional or not worth the churn:
#![expect(clippy::significant_drop_tightening, reason = "DashMap Ref shard locks; contention is acceptable in this bot's concurrency model")]
#![expect(clippy::cast_precision_loss,         reason = "integer → f64 casts in game math are fine")]
#![expect(clippy::cast_possible_truncation,    reason = "f64 → i32 truncation in cred calculations is intentional")]
#![expect(clippy::cast_possible_wrap,          reason = "small bounded values cast to i32 are safe")]
#![allow(clippy::missing_errors_doc)]          // not publishing a public API
#![allow(clippy::missing_panics_doc)]          // not publishing a public API
#![allow(clippy::must_use_candidate)]          // Discord command return values are always awaited by poise
#![allow(clippy::wildcard_imports)]            // poise/serenity re-exports are intentionally glob-imported
#![expect(clippy::items_after_statements,      reason = "nested helper fns after setup statements are intentional")]
#![expect(clippy::too_many_lines,              reason = "command dispatch functions are inherently long")]
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
use std::{env, sync::Arc, time::Duration};

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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,serenity::gateway=off")),
        )
        .init();
    // Config may also come straight from the process environment, so a missing .env is fine.
    if let Err(e) = dotenvy::dotenv() {
        tracing::warn!(error = %e, "no .env file loaded — using process environment");
    }
    let token = env::var("DISCORD_TOKEN").unwrap_or_else(|e| startup_failure("read DISCORD_TOKEN", e));
    let data = data::Data::load().unwrap_or_else(|e| startup_failure("load data", e));
    let users = data.users.clone();

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
            // post_command only runs on success — also persist whatever a failed command body
            // changed. Checks, argument parsing and cooldowns fail before the body runs, so they
            // skip the save. Reply first so the save doesn't delay the error message.
            on_error: |error| {
                Box::pin(async move {
                    let failed_body = match &error {
                        poise::FrameworkError::Command { ctx, .. }
                        | poise::FrameworkError::CommandPanic { ctx, .. } => Some(*ctx),
                        _ => None,
                    };
                    if let Err(e) = poise::builtins::on_error(error).await {
                        tracing::error!(error = %e, "failed to handle command error");
                    }
                    if let Some(ctx) = failed_body {
                        ctx.data().save().await;
                    }
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
            // Prefix commands exist only for the owner-only `~register`; everything else is slash.
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
                maintenance_task(users, http, data.mod_id, data.gen_chat);
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

    let mut client = client.unwrap_or_else(|e| startup_failure("build Discord client", e));

    // Stop cleanly on Ctrl-C / SIGTERM; `start()` then returns and the save below runs, so
    // voice rewards earned since the last command aren't lost.
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received — stopping");
        shard_manager.shutdown_all().await;
    });

    let result = client.start().await;
    data::save_users(&users).await;
    if let Err(e) = result {
        tracing::error!(error = %e, "Discord client stopped with an error");
        std::process::exit(1);
    }
}

/// Logs an unrecoverable startup failure (missing config, unreadable data) and exits non-zero.
fn startup_failure(stage: &'static str, error: impl std::fmt::Display) -> ! {
    tracing::error!(stage, error = %error, "startup failed");
    std::process::exit(1);
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
                return;
            }
            Err(e) => tracing::warn!(error = %e, "could not install SIGTERM handler — Ctrl-C only"),
        }
    }
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %e, "could not listen for Ctrl-C — shutdown save disabled");
        std::future::pending::<()>().await;
    }
}

#[expect(clippy::unused_async, reason = "poise requires async fn signature for event handlers")]
async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, data::Data, Error>,
    data: &data::Data,
) -> Result<(), Error> {
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

        // Members already in voice when the bot (re)connects never send a VoiceStateUpdate, and
        // members who left while it was offline never send their leave — resync this guild's
        // entries from the snapshot. Members still in voice keep their join time.
        serenity::FullEvent::GuildCreate { guild, .. } => {
            resync_guild_voice(&data.voice_users, guild.id, &guild.voice_states);
        }

        serenity::FullEvent::Message { new_message } => {
            if new_message.author.id == ctx.cache.current_user().id {
                return Ok(());
            }

            if ![data.gen_chat, data.bot_chat, data.sub_chat].contains(&new_message.channel_id) {
                return Ok(());
            }

            // GPT reply and doodle disabled until local LLM is ready
        }

        serenity::FullEvent::VoiceStateUpdate { old: _, new } => {
            apply_voice_state(&data.voice_users, new.guild_id, new);
        }
        _ => {}
    }
    Ok(())
}

/// Replaces a guild's voice entries with its snapshot: drops members no longer in voice,
/// adds new ones, and keeps the join time of those still there. Other guilds are untouched.
fn resync_guild_voice(
    voice_users: &DashMap<serenity::UserId, VoiceUser>,
    guild_id: serenity::GuildId,
    voice_states: &std::collections::HashMap<serenity::UserId, serenity::VoiceState>,
) {
    voice_users.retain(|id, vu| vu.guild_id != Some(guild_id) || voice_states.contains_key(id));
    for vs in voice_states.values() {
        apply_voice_state(voice_users, Some(guild_id), vs);
    }
}

/// Mirrors one member's voice state: no channel means they left, otherwise upsert (keeping
/// their join time and any running mute/deaf timers).
fn apply_voice_state(
    voice_users: &DashMap<serenity::UserId, VoiceUser>,
    guild_id: Option<serenity::GuildId>,
    vs: &serenity::VoiceState,
) {
    if vs.channel_id.is_none() {
        voice_users.remove(&vs.user_id);
        return;
    }
    let mut user = voice_users.entry(vs.user_id).or_default();
    user.guild_id = guild_id;
    user.update_mute(vs.self_mute || vs.mute);
    user.update_deaf(vs.self_deaf || vs.deaf);
}

/// Runs `tick` every `interval` for the life of the bot. Each tick is its own task, so a
/// panic is logged and the next tick still runs — otherwise one bad `.eventdb` line or a
/// reward bug would silently stop periodic saves and voice rewards until restart.
fn spawn_periodic<F, Fut>(name: &'static str, interval: Duration, mut tick: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            if let Err(e) = tokio::spawn(tick()).await {
                tracing::error!(task = name, error = %e, "periodic task failed — continuing");
            }
            tokio::time::sleep(interval).await;
        }
    });
}

fn background_task(
    users: data::UsersMap,
    voice_users: Arc<DashMap<serenity::UserId, VoiceUser>>,
) {
    // How much creds to award
    const REWARD_CREDITS: i32 = 50;
    // How much xp to award
    const REWARD_XP: i32 = 30;

    spawn_periodic("voice rewards", Duration::from_secs(VOICE_REWARD_TICK_SECS), move || {
        let users = Arc::clone(&users);
        let voice_users = Arc::clone(&voice_users);
        async move {
            let now = chrono::Utc::now();
            // Decide who is due (stamping `last_reward`) and release the voice_users shard locks
            // before touching `users` or awaiting — never hold both maps' locks at once.
            let due: Vec<serenity::UserId> = voice_users
                .iter_mut()
                .filter_map(|mut x| x.value_mut().take_reward_due(now).then(|| *x.key()))
                .collect();

            for id in due {
                let Some(user_arc) = users.get(&id).map(|e| Arc::clone(e.value())) else { continue };
                let mut user_data = user_arc.write().await;
                user_data.add_creds(REWARD_CREDITS);
                user_data.update_xp(REWARD_XP);
            }
        }
    });
}

fn maintenance_task(
    users: data::UsersMap,
    http: Arc<serenity::Http>,
    mod_id: serenity::RoleId,
    gen_chat: serenity::ChannelId,
) {
    spawn_periodic("maintenance", Duration::from_secs(MAINTENANCE_INTERVAL_SECS), move || {
        let users = Arc::clone(&users);
        let http = Arc::clone(&http);
        async move {
            reminder::check_birthday(&http, mod_id, gen_chat).await;
            data::save_users(&users).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn voice_state(user: u64, channel: Option<u64>, self_deaf: bool) -> serenity::VoiceState {
        serde_json::from_value(serde_json::json!({
            "channel_id": channel.map(|c| c.to_string()),
            "deaf": false, "mute": false, "self_deaf": self_deaf, "self_mute": false,
            "self_video": false, "session_id": "s", "suppress": false,
            "user_id": user.to_string(), "request_to_speak_timestamp": null,
        }))
        .expect("valid VoiceState JSON")
    }

    #[test]
    fn apply_voice_state_join_update_leave() {
        let users = DashMap::new();
        let guild = Some(serenity::GuildId::new(9));
        apply_voice_state(&users, guild, &voice_state(1, Some(5), false));
        let joined = users.get(&serenity::UserId::new(1)).unwrap().joined;
        // Deafening keeps the join time and starts the deaf timer.
        apply_voice_state(&users, guild, &voice_state(1, Some(5), true));
        let entry = users.get(&serenity::UserId::new(1)).unwrap().clone();
        assert_eq!(entry.joined, joined);
        assert!(entry.deaf.is_some());
        assert_eq!(entry.guild_id, guild);
        // Leaving removes the entry.
        apply_voice_state(&users, guild, &voice_state(1, None, false));
        assert!(users.is_empty());
    }

    #[test]
    fn resync_prunes_only_this_guild() {
        let users = DashMap::new();
        let (g1, g2) = (serenity::GuildId::new(1), serenity::GuildId::new(2));
        apply_voice_state(&users, Some(g1), &voice_state(10, Some(5), false)); // left while offline
        apply_voice_state(&users, Some(g1), &voice_state(11, Some(5), false)); // still in voice
        apply_voice_state(&users, Some(g2), &voice_state(20, Some(6), false)); // other guild
        let kept_join = users.get(&serenity::UserId::new(11)).unwrap().joined;

        let snapshot: HashMap<_, _> = [11, 12]
            .into_iter()
            .map(|u| (serenity::UserId::new(u), voice_state(u, Some(5), false)))
            .collect();
        resync_guild_voice(&users, g1, &snapshot);

        assert!(!users.contains_key(&serenity::UserId::new(10)));
        assert_eq!(users.get(&serenity::UserId::new(11)).unwrap().joined, kept_join);
        assert!(users.contains_key(&serenity::UserId::new(12)));
        assert!(users.contains_key(&serenity::UserId::new(20)));
    }
}
