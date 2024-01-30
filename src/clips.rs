use crate::data;

use std::time::Duration;

use crate::data::ClipData;
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
    let u = data.get_mut(&id).unwrap();
    let mut clips = u.write().await;
    // let mut data = ctx.data().users.lock().await;

    // let clips: &mut UserData = data.get_mut(&id).unwrap();

    let desc = clips.get_submissions().join("\n");

    if clips.submits.len() == 0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("No clips?")
                    .description(format!("Submit a clip using `/submit_clip`!"))
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

    let msg = reply.message().await?;

    let reactions = msg
        .await_component_interactions(ctx)
        .timeout(Duration::new(10, 0))
        .author_id(author.id);

    if let Some(reaction) = reactions.await {
        let id = reaction.data.custom_id.chars().last().unwrap();
        let i = id.to_digit(10);

        if let Some(i) = i {
            let i = i as usize;
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
                return Ok(());
            }
        }
        reaction
            .create_response(ctx, serenity::CreateInteractionResponse::Acknowledge)
            .await?;
        reply
            .edit(
                ctx,
                poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new().description("Deleted!").footer(
                        serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo"),
                    ))
                    .components(Vec::new()),
            )
            .await?;
    }

    Ok(())
}
