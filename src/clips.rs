use crate::data;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::data::ClipData;
use crate::{Context, Error};

use crate::serenity;
use dashmap::DashMap;
use poise::serenity_prelude::futures::StreamExt;

use poise::serenity_prelude::{EditMessage, ReactionType};
use rand::seq::IteratorRandom;
use rand::thread_rng;
use regex::Regex;
use tokio::sync::RwLock;

pub async fn check_mod(ctx: Context<'_>) -> Result<bool, Error> {
    let mod_id = ctx.data().mod_id;
    let guild_id = ctx.guild_id().unwrap();
    if let Ok(b) = ctx.author().has_role(ctx, guild_id, mod_id).await {
        if b {
            return Ok(true);
        }
    }
    Ok(false)
}

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
#[poise::command(prefix_command, slash_command, track_edits, check = "check_mod")]
pub async fn submit_list(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()), // Exit if not in a guild
    };

    let icon_url = guild.icon_url().unwrap_or_default();
    let banner_url = guild.banner_url().unwrap_or_default();
    let data = &ctx.data().users;

    let mut all_clips = Vec::new();

    let mut desc = "".to_string();
    for x in data.iter() {
        let (id, u) = x.pair();
        let u = u.read().await;
        for c in &u.submits {
            if let Some(c) = c {
                all_clips.push((*id, c.clone()));
            }
        }
        let author = id.to_user(ctx).await.unwrap();
        let clips = u.get_submissions(true);
        desc += &format!("\n**{}:**\n", author.name);
        desc += &clips.join("\n ");
    }

    let mut rated_clips = Vec::new();
    for (id, c) in all_clips {
        if c.rating.is_some() {
            rated_clips.push((id, c));
        }
    }

    rated_clips.sort_by(|a, b| {
        b.1.rating
            .unwrap()
            .partial_cmp(&a.1.rating.unwrap())
            .unwrap()
    });

    let top_ten = rated_clips.iter().take(10);
    let mut top_ten_desc = String::new();
    for (id, c) in top_ten {
        let author = id.to_user(ctx).await.unwrap();
        let rating = c.rating.unwrap();

        top_ten_desc += &format!(
            "**{}** - [{}/5]\n [{}]({})\n",
            author.name, rating, c.title, c.link
        );
    }

    let desc = top_ten_desc + &desc;
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

    let desc = clips.get_submissions(false).join("\n");

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

#[poise::command(prefix_command, slash_command, track_edits, check = "check_mod")]
pub async fn next_clip(ctx: Context<'_>) -> Result<(), Error> {
    let data = &ctx.data().users;

    let mut all_clips = Vec::new();

    for x in data.iter() {
        let (id, user) = x.pair();
        let u = user.read().await;

        let clips = u.clone().submits;
        for (idx, c) in clips.iter().enumerate() {
            if let Some(c) = c {
                if c.rating.is_none() {
                    all_clips.push((*id, Arc::clone(user), idx));
                }
            }
        }
    }

    let rand_clip = all_clips.iter().choose(&mut thread_rng());

    if rand_clip.is_none() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("No clips...")
                    .colour(serenity::Color::RED)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    let rand_clip = rand_clip.unwrap();

    let mut buttons = Vec::new();
    for i in 1..6 {
        let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
        let button = serenity::CreateButton::new("open_modal")
            .label("")
            .custom_id(format!("vote-clip-{}", i))
            .emoji(emoji)
            .style(poise::serenity_prelude::ButtonStyle::Secondary);
        buttons.push(button);
    }

    let button_done = vec![serenity::CreateButton::new("open_modal")
        .label("Done")
        .custom_id("vote-done".to_string())
        // .emoji(emoji)
        .style(poise::serenity_prelude::ButtonStyle::Danger)];

    let components = vec![
        serenity::CreateActionRow::Buttons(buttons),
        serenity::CreateActionRow::Buttons(button_done),
    ];

    let user = rand_clip.1.read().await;
    let clip = user.submits[rand_clip.2].clone().unwrap();

    ctx.send(poise::CreateReply::default().content(format!("**{}**\n{}", clip.title, clip.link)))
        .await?;

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(
                    serenity::CreateEmbed::default()
                        .title("Rate this clip!".to_string())
                        .colour(serenity::Color::BLUE)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                )
                .components(components),
        )
        .await?;

    let msg_og = Arc::new(RwLock::new(reply.into_message().await?));

    let msg = Arc::clone(&msg_og);

    let mut reactions = msg
        .read()
        .await
        .await_component_interactions(ctx)
        .timeout(Duration::new(10 * 60, 0))
        .stream();

    let mod_id = ctx.data().mod_id;

    let ctx = ctx.serenity_context().clone();

    let user = Arc::clone(&rand_clip.1);
    let index = rand_clip.2;

    tokio::spawn(async move {
        let score: AtomicF64 = AtomicF64::new(0.0);
        let voted_users = DashMap::new();
        while let Some(reaction) = reactions.next().await {
            let roles = reaction.member.clone().unwrap_or_default().roles;
            if reaction.data.custom_id == "vote-done" {
                if !roles.contains(&mod_id) {
                    reaction
                        .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                        .await
                        .unwrap();
                    continue;
                }
                let mut user = user.write().await;
                let clip = user.submits[index].as_mut().unwrap();
                clip.rating = Some(score.load(Ordering::Relaxed));

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::new()
                                    .title(format!(
                                        "Final Score: {}",
                                        score.load(Ordering::Relaxed)
                                    ))
                                    .colour(serenity::Color::BLUE)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();
                return;
            }
            let id = reaction.data.custom_id.chars().last().unwrap();
            let i = id.to_digit(10).unwrap() as u8;

            let user = &reaction.user;
            voted_users.insert(user.id, i);

            let s: u8 = voted_users.iter().map(|x| *x.pair().1).sum();
            let s = s as f64 / voted_users.len() as f64;
            score.store(s, Ordering::Relaxed);

            reaction
                .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                .await
                .unwrap();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Rate this clip!".to_string())
                            .description(format!("Score: {}", score.load(Ordering::Relaxed)))
                            .colour(serenity::Color::BLUE)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();
        }
        let mut user = user.write().await;
        let clip = user.submits[index].as_mut().unwrap();
        clip.rating = Some(score.load(Ordering::Relaxed));

        msg.write()
            .await
            .edit(
                &ctx,
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::new()
                            .title(format!("Final Score: {}", score.load(Ordering::Relaxed)))
                            .colour(serenity::Color::BLUE)
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

pub struct AtomicF64 {
    storage: AtomicU64,
}
impl AtomicF64 {
    pub fn new(value: f64) -> Self {
        let as_u64 = value.to_bits();
        Self {
            storage: AtomicU64::new(as_u64),
        }
    }
    pub fn store(&self, value: f64, ordering: Ordering) {
        let as_u64 = value.to_bits();
        self.storage.store(as_u64, ordering)
    }
    pub fn load(&self, ordering: Ordering) -> f64 {
        let as_u64 = self.storage.load(ordering);
        f64::from_bits(as_u64)
    }
}
