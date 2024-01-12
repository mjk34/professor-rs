use crate::{Context, Error};
// use serenity::utils::Colour;
use crate::serenity;

// Ping the bot to see if its alive or to play ping pong
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let user_id = author.id;

    {
        let mut data = ctx.data().users.lock().await;

        data.insert(user_id, Default::default());
        data.get_mut(&user_id)
            .unwrap()
            .update_name(author.name.to_string());
    }

    ctx.data().save().await;
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
pub async fn gpt_string(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}
