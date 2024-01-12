use crate::{Context, Error};
// use serenity::utils::Colour;
use crate::serenity;

// Ping the bot to see if its alive or to play ping pong
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    ctx.data().check_or_create_user(ctx).await?;
    ctx.send(
        poise::CreateReply::default()
            // .content("Pong!")
            .embed(
                serenity::CreateEmbed::new()
                    .title("Pong!")
                    .description(format!("{}", author.name)),
            ),
    )
    .await?;

    Ok(())
}

// Use gpt-3.5-turbo to generate fun responses to user prompts
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn gpt_string(ctx: Context<'_>) -> Result<(), Error> {
    ctx.data().check_or_create_user(ctx).await?;

    Ok(())
}
