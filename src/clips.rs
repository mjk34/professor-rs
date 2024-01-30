use crate::data;

use std::sync::Arc;
use std::time::Duration;

use crate::data::ClipData;
use crate::{Context, Error};

use crate::serenity;
use poise::serenity_prelude::{EditMessage, ReactionType};
use regex::Regex;
use tokio::sync::RwLock;

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
    let data = &ctx.data().users;
    let u = data.get_mut(&user.id).unwrap();
    let mut user_data = u.write().await;

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
    let data = &ctx.data().users;

    let mut desc = "".to_string();
    for x in data.iter() {
        let (id, u) = x.pair();
        let u = u.read().await;
        let author = id.to_user(ctx).await.unwrap();
        let clips = u.get_submissions();
        desc += &format!("\n**{}:**\n", author.name);
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

    let data = &ctx.data().users;
    let u = data.get(&id).unwrap();
    let clips = u.read().await;

    let desc = clips.get_submissions().join("\n");

    if clips.submits.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("No clips?")
                    .description("Submit a clip using `/submit_clip`!".to_string())
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    let mut buttons = Vec::new();
    for i in 0..clips.submits.len() {
        let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
        let button = serenity::CreateButton::new("open_modal")
            .label("")
            .custom_id(format!("delete-clip-{}", i))
            .emoji(emoji)
            .style(poise::serenity_prelude::ButtonStyle::Secondary);
        buttons.push(button);
    }

    let components = vec![serenity::CreateActionRow::Buttons(buttons)];

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(
                    serenity::CreateEmbed::default()
                        .title("Clip Submissions")
                        .description(format!("*Click on the react emoji to delete*\n\n{}", desc))
                        .colour(serenity::Color::DARK_GREEN)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                )
                .components(components),
        )
        .await?;

    drop(clips);

    let msg_og = Arc::new(RwLock::new(reply.into_message().await?));

    let msg = Arc::clone(&msg_og);

    let reactions = msg
        .read()
        .await
        .await_component_interactions(ctx)
        .timeout(Duration::new(15, 0))
        .author_id(author.id);

    let u = Arc::clone(&u);
    let ctx = ctx.serenity_context().clone();

    tokio::spawn(async move {
        if let Some(reaction) = reactions.await {
            let id = reaction.data.custom_id.chars().last().unwrap();
            let i = id.to_digit(10);
            if let Some(i) = i {
                let mut clip = u.write().await;

                clip.remove_submit(i as usize);
            }
            reaction
                .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                .await
                .unwrap();
            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default()
                        .embed(serenity::CreateEmbed::new().description("Deleted!").footer(
                            serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo"),
                        ))
                        .components(Vec::new()),
                )
                .await
                .unwrap();
            return;
        }
        msg.write()
            .await
            .edit(
                ctx,
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::new()
                            .description("Edit timed out...")
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    )
                    .components(Vec::new()),
            )
            .await
            .unwrap();
    });

    Ok(())
}
