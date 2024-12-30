//!---------------------------------------------------------------------!
//! This file contains a collection of clip related commands to allow   !
//! the organization, submission and facilitation of clip night         !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - submit_clip                                               !
//!     [x] - server_clips                                              !
//!     [x] - my_clips                                                  !
//!     [x] - next_clip                                                 !
//!---------------------------------------------------------------------!

use crate::data::{self, ClipData};
use crate::{serenity, Context, Error};
use dashmap::DashMap;
use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::{EditMessage, ReactionType};
use rand::seq::IteratorRandom;
use rand::thread_rng;
use regex::Regex;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
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
    let medal_regex = Regex::new(r"https?://medal\.tv/.+").unwrap();

    youtube_regex.is_match(url) || medal_regex.is_match(url)
}

/// submit a youtube or medal clip for clip night!
#[poise::command(slash_command)]
pub async fn submit_clip(
    ctx: Context<'_>,
    #[description = "the name of your clip"] title: String,
    #[description = "the youtube or medal link of your clip"] link: String,
) -> Result<(), Error> {
    let sub_chat = env::var("SUBMIT").expect("Failed to load SUBMIT channel id");

    if ctx.channel_id().get().to_string() != sub_chat {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Submit Clip")
                    .description("Wrong Channel - Please resolve clip activity here: ")
                    .color(data::EMBED_ERROR)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    if !is_youtube_or_medal_url(&link) {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Submit Clip")
                    .description("Invalid link - Link must either be youtube or medal")
                    .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1197004718631833650/tenor.gif?ex=65b9b084&is=65a73b84&hm=0368979e5bdf0c258f6b344ec2b79826459b3ec4c937374e05ec77f131adf37f&")
                    .color(data::EMBED_ERROR)
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

    let avatar = user.avatar_url().unwrap_or_default().to_string();

    let desc = format!(
        "Title: \u{3000}**{}**\nLink: \u{3000}**{}**\n",
        &title, &link
    );

    let clip = ClipData::new(title, link);

    let check = user_data.add_submit(clip);

    if !check {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Submit Clip")
                    .thumbnail(&avatar)
                    .description("Max clips reached...")
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354794588307456/tenor_1.gif?ex=65d81121&is=65c59c21&hm=35114062e5a4516b69da081842189520df9b846bce5b8547f83ad39c91c2d1cd&")
                    .color(data::EMBED_FAIL)
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
                .title("Submission")
                .thumbnail(&avatar)
                .description(desc)
                .color(data::EMBED_CYAN)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

/// [!] MODERATOR - view all submitted clips sorted by users
#[poise::command(slash_command, track_edits, check = "check_mod")]
pub async fn server_clips(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()), // Exit if not in a guild
    };

    let icon_url = guild.icon_url().unwrap_or_default();
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
        let clips = u.get_submissions(true, false);

        if !clips.is_empty() {
            desc += &format!(
                "\n**{}:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n",
                author.name.replace('_', "")
            );
            desc += &clips.join("\n ");
            desc += "\n";
        }
    }

    println!("{:}", &desc);

    if all_clips.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("Server Clips")
                    .description("Where are the clips...")
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354794156429362/tenor_2.gif?ex=65d81121&is=65c59c21&hm=c402afb9f3a578f018657cd60a4b8ec1cefccc09e26b0830701037593852b65d&")
                    .color(data::EMBED_ERROR)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
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
    let top_len = &top_ten.len();
    let mut top_ten_desc = String::new();
    top_ten_desc += &format!(
        "\n**{}:** \n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n",
        "Top Rated Clips"
    );

    for (id, c) in top_ten {
        let author = id.to_user(ctx).await.unwrap();
        let rating = c.rating.unwrap();

        top_ten_desc += &format!(
            "[{}/5] **[{}]({})** - {}\n",
            rating,
            c.title,
            c.link,
            author.name.replace('_', "")
        );
    }

    if *top_len > 0 {
        desc = top_ten_desc + "-\n" + &desc;
    }

    println!("{:}", &desc);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title("Server Clips")
                .thumbnail(&icon_url)
                .description(desc)
                .color(data::EMBED_MOD)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

/// [!] view and edit your submitted clips
#[poise::command(slash_command, track_edits)]
pub async fn my_clips(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let id = author.id;
    let avatar = author.avatar_url().unwrap_or_default().to_string();

    let data = &ctx.data().users;
    let u = data.get(&id).unwrap();
    let clips = u.read().await;

    let desc = clips.get_submissions(false, true).join("\n");

    if clips.submits.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("My Clips")
                    .description("You have not submitted a clip yet, submit your first clip with /submit-clip!!")
                    .thumbnail(&avatar)
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354793753903104/tenor_3.gif?ex=65d81121&is=65c59c21&hm=32b8b0926677e68d225a2085b4a99ac63d0356b5cb4d05d54e13f5013b9a8664&")
                    .color(data::EMBED_ERROR)
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
                        .title("My Clips")
                        .description(format!("Ta-da!! Your carefully crafted clips!! (*If you wish to remove a clip, use the emojis below*)\n\n{}", desc))
                        .thumbnail(&avatar)
                        .color(data::EMBED_DEFAULT)
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
        .timeout(Duration::new(30, 0))
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
                        .embed(
                            serenity::CreateEmbed::new()
                                .title("My Clips")
                                .description("Clip Removed!")
                                .thumbnail(&avatar)
                                .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354795012071424/tenor.gif?ex=65d81121&is=65c59c21&hm=e283dc1b9ffdeb45b85d8caabfdc68dedbf18faef0bdf84967f7d242749476cd&")
                                .color(data::EMBED_CYAN)
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
        msg.write()
            .await
            .edit(
                ctx,
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::default()
                            .title("My Clips")
                            .thumbnail(&avatar)
                            .description(format!("Edit timed out...\n\n{}", desc))
                            .colour(data::EMBED_ERROR)
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

/// [!] MODERATOR - CLIP NIGHT ONLY - get the next clip to view and rate
#[poise::command(slash_command, track_edits, check = "check_mod")]
pub async fn next_clip(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()), // Exit if not in a guild
    };

    let icon_url = guild.icon_url().unwrap_or_default();

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
                    .title("Next Clip")
                    .description("No more clips!")
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354793246138408/tenor_4.gif?ex=65d81121&is=65c59c21&hm=752fa8c3dbc4ef91ec632f3988261c0e7628fb6ca54170ffdd6439a5de9a3a9b&")
                    .thumbnail(&icon_url)
                    .colour(data::EMBED_FAIL)
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
                        .title("Next Clip")
                        .description("Rate this clip!")
                        .thumbnail(&icon_url)
                        .colour(data::EMBED_DEFAULT)
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
                                    .title("Next Clip")
                                    .description(format!(
                                        "Final Score: **{}**",
                                        score.load(Ordering::Relaxed)
                                    ))
                                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354792621309972/tenor_5.gif?ex=65d81120&is=65c59c20&hm=b7661397c96231060492b909d1d7f2025bcfa91c166618611f612e95551be35a&")
                                    .thumbnail(&icon_url)
                                    .colour(data::EMBED_MOD)
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
                            .title("Next Clip")
                            .thumbnail(&icon_url)
                            .description(format!(
                                "Rate this clip!\n\nScore: {}",
                                score.load(Ordering::Relaxed)
                            ))
                            .colour(data::EMBED_DEFAULT)
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
                            .title("Next Clip")
                            .description(format!(
                                "Final Score: **{}**",
                                score.load(Ordering::Relaxed)
                            ))
                            .thumbnail(&icon_url)
                            .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354792621309972/tenor_5.gif?ex=65d81120&is=65c59c20&hm=b7661397c96231060492b909d1d7f2025bcfa91c166618611f612e95551be35a&")
                            .colour(data::EMBED_MOD)
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
