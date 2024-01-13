mod basic;
mod data;

use std::env;
use tokio;
use tracing::{error, info};

use poise::async_trait;
pub use poise::serenity_prelude as serenity;

struct Bot;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, data::Data, Error>;

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[async_trait]
impl serenity::EventHandler for Bot {
    async fn message(&self, ctx: serenity::Context, msg: serenity::Message) {
        //same as on_message function in main.py
        // println!("{:?}", msg);
        if msg.content == "yo" {
            if let Err(e) = msg.channel_id.say(&ctx.http, "mama").await {
                error!("Error sending message: {:?}", e);
            }
        }
    }

    async fn ready(&self, _: serenity::Context, ready: serenity::Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to read .env file");
    let token = env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");

    let data = data::Data::load();

    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILDS;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            // Check and create a user account before each command
            pre_command: |ctx: Context<'_>| {
                Box::pin(async move {
                    ctx.data().check_or_create_user(ctx).await.unwrap();
                })
            },
            // Save all data after running a command
            post_command: |ctx: Context<'_>| {
                Box::pin(async move {
                    ctx.data().save().await;
                })
            },
            commands: vec![register(), basic::ping(), basic::gpt_string()],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                ..Default::default()
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
            state: Some("Coding".to_string()),
            url: None,
        })
        .status(serenity::OnlineStatus::Online)
        .event_handler(Bot)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}
