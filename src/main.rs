mod basic;
mod clips;
mod data;
mod event;

use std::env;
use tracing::error;

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
    dotenv::dotenv().expect("Failed to read .env file");
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
                basic::leaderboard(),
                clips::submit_clip(),
                clips::submit_list(),
                clips::edit_list(),
                event::search_pokemon(),
                event::test_matchup(),
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
        .setup(|_, _ready, _| {
            //|ctx, _ready, framework| {
            Box::pin(async move {
                // poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(data)
            })
        })
        .build();

    let client = serenity::Client::builder(&token, intents)
        .activity(serenity::ActivityData {
            name: "Coding Rust".to_string(),
            kind: serenity::ActivityType::Custom,
            state: Some("Test - Ping".to_string()),
            url: None,
        })
        .status(serenity::OnlineStatus::Online)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, data::Data, Error>,
    data: &data::Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            println!("Logged in as {}\n\n", data_about_bot.user.name);
        }
        serenity::FullEvent::Message { new_message } => {
            if new_message.content == "yo" {
                if let Err(e) = new_message.channel_id.say(&ctx.http, "mama").await {
                    error!("Error sending message: {:?}", e);
                }
            }
        }
        serenity::FullEvent::VoiceStateUpdate { old: _, new } => {
            let mut voice_users = data.voice_users.lock().await;

            // Someone left the channel
            if new.channel_id.is_none() {
                voice_users.remove(&new.user_id);
                return Ok(());
            }

            let user = voice_users
                .entry(new.user_id)
                .or_insert(data::VoiceUser::new());
            user.update_mute(new.self_mute || new.mute);
            user.update_deaf(new.self_deaf || new.deaf);
        }
        _ => {}
    }
    Ok(())
}
