use crate::{Context, Error};

// use poise::serenity_prelude as serenity;
// use serenity::{AttachmentType, CreateEmbed};

#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let username = ctx.author().id;
    let response = format!("Heya {}, Pong!", username);

    ctx.say(response).await?;

    Ok(())
}