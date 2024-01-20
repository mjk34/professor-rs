use crate::data;
use std::time::Duration;

use crate::data::{ClipData, UserData};
use crate::{Context, Error};

use crate::serenity;
use poise::serenity_prelude::ReactionType;
use regex::Regex;

fn is_youtube_or_medal_url(url: &str) -> bool {
    let youtube_regex =
        Regex::new(r"(https?://)?(www\.)?(youtube\.com/watch\?v=|youtu\.be/).+").unwrap();
    let medal_regex = Regex::new(r"https?://medal\.tv/clips/.+").unwrap();

    youtube_regex.is_match(url) || medal_regex.is_match(url)
}

// Submit game/other clips
#[poise::command(prefix_command, slash_command)]
pub async fn submit_clip(ctx: Context<'_>, title: String, link: String) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()), // Exit if not in a guild
    };

    let icon_url = guild.icon_url().unwrap_or_default();
    let banner_url = guild.banner_url().unwrap_or_default();

    if !is_youtube_or_medal_url(&link) {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Invalid link")
                    .thumbnail(&icon_url)
                    .image(&banner_url)
                    .description("Link must either be youtube or medal")
                    .colour(serenity::Color::RED)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    let user = ctx.author();
    let mut data = ctx.data().users.lock().await;
    let user_data = data.get_mut(&user.id).unwrap();

    let clip = ClipData::new(title, link);

    let check = user_data.add_submit(clip);

    if !check {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Max clips reached")
                    .thumbnail(&icon_url)
                    .image(&banner_url)
                    .description("You've submitted the maximum number of clips")
                    .colour(serenity::Color::RED)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title(&guild.name)
                .thumbnail(&icon_url)
                .image(&banner_url)
                .description("Uploaded!")
                .colour(serenity::Color::DARK_GREEN)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

// View clip submission summary
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn submit_list(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()), // Exit if not in a guild
    };

    let icon_url = guild.icon_url().unwrap_or_default();
    let banner_url = guild.banner_url().unwrap_or_default();
    let data = ctx.data().users.lock().await;

    let mut desc = "".to_string();
    for (id, u) in data.iter() {
        let author = id.to_user(ctx).await.unwrap();
        let clips = u.get_submissions();
        desc += &format!("**{}:**\n", author.name);
        desc += &clips.join("\n ");
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title("Clip Submissions")
                .thumbnail(&icon_url)
                .image(&banner_url)
                .description(desc)
                .colour(serenity::Color::DARK_GREEN)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn edit_list(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let id = author.id;

    let mut data = ctx.data().users.lock().await;

    let clips: &mut UserData = data.get_mut(&id).unwrap();

    let desc = clips.get_submissions().join("\n");

    let reply = ctx
        .send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Clip Submissions")
                    .description(format!("*Click on the react emoji to delete*\n\n{}", desc))
                    .colour(serenity::Color::DARK_GREEN)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    let msg = reply.message().await?;

    //TODO: calculate non-none
    for i in 0..clips.submits.len() {
        msg.react(
            ctx,
            ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string()),
        )
        .await?;
    }

    let reactions = msg
        .await_reaction(ctx)
        .timeout(Duration::new(60, 0))
        .author_id(author.id);

    if let Some(reaction) = reactions.await {
        let i = data::NUMBER_EMOJS
            .iter()
            .position(|x| reaction.emoji.unicode_eq(x));

        if let Some(i) = i {
            if !clips.remove_submit(i) {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::default()
                            .title("Could not delete...")
                            .colour(serenity::Color::RED)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await?;
            }
        }
    }

    reply
        .edit(
            ctx,
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().description("Deleted!").footer(
                    serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo"),
                ),
            ),
        )
        .await?;
    Ok(())
}
