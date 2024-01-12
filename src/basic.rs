use crate::{Context, Error};

// Ping the bot to see if its alive or to play ping pong
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    // let username = ctx.author().id;
    let response = format!("Pong!");

    ctx.say(response).await?;

    Ok(())
}

// Use gpt-3.5-turbo to generate fun responses to user prompts
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn gpt_string(_ctx: Context<'_>) -> Result<(), Error> {

    Ok(())
} 