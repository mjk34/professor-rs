//!---------------------------------------------------------------------!
//! This file contains a collection of clip related commands to allow   !
//! the organization, submission and facilitation of clip night         !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - `submit_clip`                                               !
//!     [x] - `server_clips`                                              !
//!     [x] - `my_clips`                                                  !
//!     [x] - `next_clip`                                                 !
//!---------------------------------------------------------------------!

use crate::data::{self, ClipData};
use crate::helper::default_footer;
use crate::{serenity, Context, Error};
use poise::serenity_prelude::{futures, EditMessage, ReactionType};
use rand::seq::SliceRandom;
use rand::thread_rng;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

static YOUTUBE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(https?://)?(www\.)?(youtube\.com/watch\?v=|youtu\.be/).+").expect("literal regex")
});
static MEDAL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://medal\.tv/clips/.+").expect("literal regex"));

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
    YOUTUBE_REGEX.is_match(url) || MEDAL_REGEX.is_match(url)
}

/// submit a youtube or medal clip for clip night!
#[poise::command(slash_command)]
pub async fn submit_clip(
    ctx: Context<'_>,
    #[description = "the name of your clip"] title: String,
    #[description = "the youtube or medal link of your clip"] link: String,
) -> Result<(), Error> {
    if ctx.channel_id().get().to_string() != ctx.data().sub_chat {
        ctx.send(poise::CreateReply::default()
            .ephemeral(true)
            .embed(serenity::CreateEmbed::default()
                .title("Submit Clip")
                .description("Clips can only be submitted in the designated clip submission channel.")
                .color(data::EMBED_ERROR)
                .footer(default_footer())))
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
                    .footer(default_footer()),
            ),
        )
        .await?;
        return Ok(());
    }

    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get_mut(&user.id).unwrap();
    let mut user_data = u.write().await;

    let avatar = user.avatar_url().unwrap_or_default();

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
                    .footer(default_footer()),
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
                .footer(default_footer()),
        ),
    )
    .await?;
    Ok(())
}

/// [!] MODERATOR - view all submitted clips sorted by users
#[poise::command(slash_command, track_edits, check = "check_mod")]
pub async fn server_clips(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() { Some(g) => g.clone(), None => return Ok(()) };
    let icon_url = guild.icon_url().unwrap_or_default();
    let serenity_ctx = ctx.serenity_context().clone();

    // Snapshot Arc refs — no shard locks held across any await point.
    let user_arcs: Vec<_> = ctx.data().users.iter()
        .map(|e| (*e.key(), Arc::clone(e.value())))
        .collect();

    // Parallel: read clips + resolve username for every user concurrently.
    let resolved: Vec<(String, Vec<ClipData>, Vec<String>)> =
        futures::future::join_all(user_arcs.iter().map(|(id, u)| {
            let ctx = serenity_ctx.clone();
            let id = *id;
            async move {
                let ud = u.read().await;
                let clips: Vec<ClipData> = ud.submits.iter().flatten().cloned().collect();
                let submissions = ud.get_submissions(true, false);
                drop(ud);
                let name = match id.to_user(&ctx).await {
                    Ok(u) => u.name,
                    Err(_) => "Unknown".to_string(),
                };
                (name, clips, submissions)
            }
        })).await;

    // (author_name, clip) for sorting/top-10 — no second to_user call needed.
    let mut all_clips: Vec<(String, ClipData)> = Vec::new();
    let mut desc = String::new();

    for (name, clips, submissions) in &resolved {
        for clip in clips {
            all_clips.push((name.clone(), clip.clone()));
        }
        if !submissions.is_empty() {
            desc += &format!(
                "\n**{}:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n",
                name.replace('_', "")
            );
            desc += &submissions.join("\n ");
            desc += "\n";
        }
    }

    if all_clips.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title("Server Clips")
                .description("Where are the clips...")
                .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354794156429362/tenor_2.gif?ex=65d81121&is=65c59c21&hm=c402afb9f3a578f018657cd60a4b8ec1cefccc09e26b0830701037593852b65d&")
                .color(data::EMBED_ERROR)
                .footer(default_footer()),
        )).await?;
        return Ok(());
    }

    let mut rated: Vec<&(String, ClipData)> =
        all_clips.iter().filter(|(_, c)| c.rating.is_some()).collect();
    rated.sort_by(|a, b| {
        b.1.rating.unwrap_or(0.0)
            .partial_cmp(&a.1.rating.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if !rated.is_empty() {
        let mut top_desc = "\n**Top Rated Clips:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n".to_string();
        for (name, c) in rated.iter().take(10) {
            top_desc += &format!(
                "[{}/5] **[{}]({})** - {}\n",
                c.rating.unwrap_or(0.0),
                c.title,
                c.link,
                name.replace('_', "")
            );
        }
        desc = top_desc + "-\n" + &desc;
    }

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::default()
            .title("Server Clips")
            .thumbnail(&icon_url)
            .description(desc)
            .color(data::EMBED_MOD)
            .footer(default_footer()),
    )).await?;

    Ok(())
}

/// [!] view and edit your submitted clips
#[poise::command(slash_command, track_edits)]
pub async fn my_clips(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let avatar = author.avatar_url().unwrap_or_default();
    let u = Arc::clone(ctx.data().users.get(&author.id).unwrap().value());
    let serenity_ctx = ctx.serenity_context().clone();

    let (desc, button_count) = {
        let ud = u.read().await;
        if ud.submits.is_empty() {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::default()
                    .title("My Clips")
                    .description("You have not submitted a clip yet, submit your first clip with /submit-clip!!")
                    .thumbnail(&avatar)
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354793753903104/tenor_3.gif?ex=65d81121&is=65c59c21&hm=32b8b0926677e68d225a2085b4a99ac63d0356b5cb4d05d54e13f5013b9a8664&")
                    .color(data::EMBED_ERROR)
                    .footer(default_footer()),
            )).await?;
            return Ok(());
        }
        (ud.get_submissions(false, true).join("\n"), ud.submits.len())
    };

    let buttons: Vec<_> = (0..button_count).map(|i| {
        let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
        serenity::CreateButton::new(format!("delete-clip-{i}"))
            .label("")
            .emoji(emoji)
            .style(serenity::ButtonStyle::Secondary)
    }).collect();

    let reply = ctx.send(poise::CreateReply::default()
        .embed(serenity::CreateEmbed::default()
            .title("My Clips")
            .description(format!("Ta-da!! Your carefully crafted clips!! (*If you wish to remove a clip, use the emojis below*)\n\n{desc}"))
            .thumbnail(&avatar)
            .color(data::EMBED_DEFAULT)
            .footer(default_footer()))
        .components(vec![serenity::CreateActionRow::Buttons(buttons)])
    ).await?;

    let mut msg = reply.into_message().await?;

    let Some(press) = msg
        .await_component_interaction(&serenity_ctx)
        .author_id(author.id)
        .timeout(Duration::from_secs(30))
        .await
    else {
        msg.edit(&serenity_ctx, EditMessage::default()
            .embed(serenity::CreateEmbed::default()
                .title("My Clips")
                .thumbnail(&avatar)
                .description(format!("Edit timed out...\n\n{desc}"))
                .colour(data::EMBED_ERROR)
                .footer(default_footer()))
            .components(vec![]))
            .await.ok();
        return Ok(());
    };

    press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

    let Some(i) = press.data.custom_id.strip_prefix("delete-clip-")
        .and_then(|s| s.parse::<usize>().ok())
    else { return Ok(()); };

    u.write().await.remove_submit(i);

    msg.edit(&serenity_ctx, EditMessage::default()
        .embed(serenity::CreateEmbed::new()
            .title("My Clips")
            .description("Clip Removed!")
            .thumbnail(&avatar)
            .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354795012071424/tenor.gif?ex=65d81121&is=65c59c21&hm=e283dc1b9ffdeb45b85d8caabfdc68dedbf18faef0bdf84967f7d242749476cd&")
            .color(data::EMBED_CYAN)
            .footer(default_footer()))
        .components(vec![]))
        .await.ok();

    Ok(())
}

/// [!] MODERATOR - CLIP NIGHT ONLY - get the next clip to view and rate
#[poise::command(slash_command, track_edits, check = "check_mod")]
pub async fn next_clip(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() { Some(g) => g.clone(), None => return Ok(()) };
    let icon_url = guild.icon_url().unwrap_or_default();
    let serenity_ctx = ctx.serenity_context().clone();
    let mod_id = ctx.data().mod_id;

    // Snapshot Arc refs — no shard locks held across await points.
    let user_arcs: Vec<_> = ctx.data().users.iter()
        .map(|e| Arc::clone(e.value()))
        .collect();

    // Collect unrated clip candidates (sequential read, but no DashMap locks held).
    let mut candidates: Vec<(Arc<tokio::sync::RwLock<crate::data::UserData>>, usize)> = Vec::new();
    for u in &user_arcs {
        let ud = u.read().await;
        for (idx, slot) in ud.submits.iter().enumerate() {
            if matches!(slot, Some(c) if c.rating.is_none()) {
                candidates.push((Arc::clone(u), idx));
            }
        }
    }

    if candidates.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title("Next Clip")
                .description("No more clips!")
                .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354793246138408/tenor_4.gif?ex=65d81121&is=65c59c21&hm=752fa8c3dbc4ef91ec632f3988261c0e7628fb6ca54170ffdd6439a5de9a3a9b&")
                .thumbnail(&icon_url)
                .colour(data::EMBED_FAIL)
                .footer(default_footer()),
        )).await?;
        return Ok(());
    }

    let (user_arc, clip_idx) = {
        let choice = candidates.choose(&mut thread_rng()).unwrap(); // safe: non-empty
        (Arc::clone(&choice.0), choice.1)
    };

    let (clip_title, clip_link) = {
        let ud = user_arc.read().await;
        let Some(Some(clip)) = ud.submits.get(clip_idx) else {
            ctx.say("That clip is no longer available.").await?;
            return Ok(());
        };
        (clip.title.clone(), clip.link.clone())
    };

    let vote_buttons: Vec<_> = (1..=5).map(|i| {
        let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
        serenity::CreateButton::new(format!("vote-clip-{i}"))
            .label("")
            .emoji(emoji)
            .style(serenity::ButtonStyle::Secondary)
    }).collect();

    let components = vec![
        serenity::CreateActionRow::Buttons(vote_buttons),
        serenity::CreateActionRow::Buttons(vec![
            serenity::CreateButton::new("vote-done")
                .label("Done")
                .style(serenity::ButtonStyle::Danger),
        ]),
    ];

    ctx.send(poise::CreateReply::default().content(format!("**{clip_title}**\n{clip_link}"))).await?;
    let reply = ctx.send(poise::CreateReply::default()
        .embed(serenity::CreateEmbed::default()
            .title("Next Clip")
            .description("Rate this clip!")
            .thumbnail(&icon_url)
            .colour(data::EMBED_DEFAULT)
            .footer(default_footer()))
        .components(components)
    ).await?;

    let mut msg = reply.into_message().await?;

    // Voting loop — no spawn, no AtomicF64, no DashMap for voted_users.
    let mut score = 0.0_f64;
    let mut voted_users: HashMap<serenity::UserId, u8> = HashMap::new();

    let final_score = loop {
        let Some(press) = msg
            .await_component_interaction(&serenity_ctx)
            .timeout(Duration::from_secs(10 * 60))
            .await
        else {
            break score; // timeout — finalize with current score
        };

        press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

        if press.data.custom_id == "vote-done" {
            let is_mod = press.member.as_ref()
                .is_some_and(|m| m.roles.contains(&mod_id));
            if !is_mod { continue; }
            break score;
        }

        let Some(i) = press.data.custom_id.strip_prefix("vote-clip-")
            .and_then(|s| s.parse::<u8>().ok())
        else { continue; };

        voted_users.insert(press.user.id, i);
        let total: u8 = voted_users.values().sum();
        score = f64::from(total) / voted_users.len() as f64;

        msg.edit(&serenity_ctx, EditMessage::default()
            .embed(serenity::CreateEmbed::new()
                .title("Next Clip")
                .thumbnail(&icon_url)
                .description(format!("Rate this clip!\n\nScore: {score:.2}"))
                .colour(data::EMBED_DEFAULT)
                .footer(default_footer())))
            .await.ok();
    };

    if let Some(Some(clip)) = user_arc.write().await.submits.get_mut(clip_idx) {
        clip.rating = Some(final_score);
    }

    msg.edit(&serenity_ctx, EditMessage::default()
        .embed(serenity::CreateEmbed::new()
            .title("Next Clip")
            .description(format!("Final Score: **{final_score:.2}**"))
            .thumbnail(&icon_url)
            .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354792621309972/tenor_5.gif?ex=65d81120&is=65c59c20&hm=b7661397c96231060492b909d1d7f2025bcfa91c166618611f612e95551be35a&")
            .colour(data::EMBED_MOD)
            .footer(default_footer()))
        .components(vec![]))
        .await.ok();

    Ok(())
}
