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

use crate::data::{self, clip_title_display, ClipData, SubmissionStyle, MAX_CLIP_RATING};
use crate::helper::{check_mod, default_footer, escape_markdown, fetch_users, user_name};
use crate::{serenity, Context, Error};
use poise::serenity_prelude::{EditMessage, ReactionType};
use rand::seq::SliceRandom;
use rand::thread_rng;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// How long `/my_clips` waits for a delete-button press before disarming.
const MY_CLIPS_TIMEOUT: Duration = Duration::from_secs(30);
/// Max idle time between votes in `/next_clip` before the current score is finalized.
const VOTE_IDLE_TIMEOUT: Duration = Duration::from_mins(10);

// Button custom ids, shared by the button builders and the press handlers.
/// `/my_clips` delete button for slot N: `delete-clip-N`.
const DELETE_CLIP_PREFIX: &str = "delete-clip-";
/// `/next_clip` vote button for N stars: `vote-clip-N`.
const VOTE_CLIP_PREFIX: &str = "vote-clip-";
/// `/next_clip` button a moderator presses to end voting.
const VOTE_DONE_ID: &str = "vote-done";
/// Rated clips listed at the top of `/server_clips`.
const TOP_RATED_COUNT: usize = 10;

/// Embed descriptions are capped at 4096 chars by Discord; leave headroom for page headers.
const EMBED_PAGE_LIMIT: usize = 3900;

/// Accepted `YouTube` links. Video ids, path segments and query strings are restricted to
/// URL-safe characters with no markdown meaning — `\S+` let `)[text](https://evil)` ride
/// along and form a masked link.
static YOUTUBE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://)?(www\.)?(youtube\.com/watch\?v=[\w-]+(&[\w=&.%-]*)?|youtu\.be/[\w-]+(\?[\w=&.%-]*)?)$")
        .expect("literal regex")
});
/// Accepted Medal clip links (same character restrictions as [`YOUTUBE_REGEX`]).
static MEDAL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https?://medal\.tv/clips/[\w-]+(/[\w-]+)*/?(\?[\w=&.%-]*)?$").expect("literal regex")
});

/// Mean of all votes. Summed as u32 — a u8 sum overflows past 51 five-star votes.
fn average_vote(votes: &HashMap<serenity::UserId, u8>) -> f64 {
    if votes.is_empty() {
        return 0.0;
    }
    let total: u32 = votes.values().map(|&v| u32::from(v)).sum();
    f64::from(total) / votes.len() as f64
}

fn is_youtube_or_medal_url(url: &str) -> bool {
    YOUTUBE_REGEX.is_match(url) || MEDAL_REGEX.is_match(url)
}

/// Star rating from a vote button's custom id; `None` for anything outside the star range.
fn parse_vote(custom_id: &str) -> Option<u8> {
    custom_id
        .strip_prefix(VOTE_CLIP_PREFIX)?
        .parse::<u8>()
        .ok()
        .filter(|v| (1..=MAX_CLIP_RATING).contains(v))
}

/// Packs sections into embed-sized pages, keeping each section whole where possible and
/// truncating (on a char boundary) any single section longer than `limit`.
fn paginate(sections: Vec<String>, limit: usize) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;
    for mut section in sections {
        let mut chars = section.chars().count();
        if chars > limit {
            section = section.chars().take(limit.saturating_sub(1)).chain(std::iter::once('…')).collect();
            chars = limit;
        }
        if current_chars > 0 && current_chars + chars > limit {
            pages.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        current.push_str(&section);
        current_chars += chars;
    }
    if !current.is_empty() {
        pages.push(current);
    }
    pages
}

/// submit a youtube or medal clip for clip night!
#[poise::command(slash_command)]
pub async fn submit_clip(
    ctx: Context<'_>,
    #[description = "the name of your clip"]
    #[max_length = 100] // keep in sync with data::MAX_CLIP_TITLE_LEN
    title: String,
    #[description = "the youtube or medal link of your clip"]
    #[max_length = 200] // generous for real YouTube/Medal URLs; caps embed length
    link: String,
) -> Result<(), Error> {
    if ctx.channel_id() != ctx.data().sub_chat {
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
    let u = Arc::clone(ctx.data().users.get(&user.id).unwrap().value());

    let avatar = user.avatar_url().unwrap_or_default();

    let desc = format!("Title: \u{3000}**{}**\nLink: \u{3000}**{link}**\n", clip_title_display(&title));

    let clip = ClipData::new(title, link);

    // Temporary guard: released before the Discord sends below.
    let check = u.write().await.add_submit(clip);

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
    let icon_url = ctx.guild().and_then(|g| g.icon_url()).unwrap_or_default();
    let serenity_ctx = ctx.serenity_context().clone();

    // Snapshot Arc refs — no shard locks held across any await point.
    let user_arcs: Vec<_> = ctx.data().users.iter()
        .map(|e| (*e.key(), Arc::clone(e.value())))
        .collect();

    struct Submitter {
        id: serenity::UserId,
        /// Rated clips only — candidates for the top-rated list.
        rated: Vec<ClipData>,
        listing: Vec<String>,
    }

    // Read clips first so usernames are only resolved (possibly over HTTP) for submitters.
    let mut submitters: Vec<Submitter> = Vec::new();
    for (id, u) in &user_arcs {
        let ud = u.read().await;
        if ud.has_submissions() {
            submitters.push(Submitter {
                id: *id,
                rated: ud.submits.iter().flatten().filter(|c| c.rating.is_some()).cloned().collect(),
                listing: ud.get_submissions(SubmissionStyle::Scored),
            });
        }
    }

    if submitters.is_empty() {
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

    let names: Vec<String> = fetch_users(&serenity_ctx, submitters.iter().map(|s| s.id)).await
        .iter()
        .map(|u| escape_markdown(&user_name(u.as_ref())))
        .collect();

    // (author_name, clip) for the top-10 list.
    let mut rated: Vec<(&str, &ClipData)> = submitters.iter().zip(&names)
        .flat_map(|(s, name)| s.rated.iter().map(move |c| (name.as_str(), c)))
        .collect();
    rated.sort_by(|a, b| {
        b.1.rating.unwrap_or(0.0)
            .partial_cmp(&a.1.rating.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut sections: Vec<String> = Vec::new();
    if !rated.is_empty() {
        let mut top_desc = "\n**Top Rated Clips:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n".to_string();
        for (name, c) in rated.iter().take(TOP_RATED_COUNT) {
            top_desc += &format!("[{}/{MAX_CLIP_RATING}] **{}** - {name}\n", c.rating.unwrap_or(0.0), c.markdown_link());
        }
        top_desc += "-\n";
        sections.push(top_desc);
    }
    for (s, name) in submitters.iter().zip(&names) {
        sections.push(format!(
            "\n**{name}:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n{}\n",
            s.listing.join("\n ")
        ));
    }

    let pages = paginate(sections, EMBED_PAGE_LIMIT);
    let page_count = pages.len();
    for (i, page) in pages.into_iter().enumerate() {
        let title = if page_count > 1 { format!("Server Clips ({}/{page_count})", i + 1) } else { "Server Clips".to_string() };
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title(title)
                .thumbnail(&icon_url)
                .description(page)
                .color(data::EMBED_MOD)
                .footer(default_footer()),
        )).await?;
    }

    Ok(())
}

/// [!] view and edit your submitted clips
#[poise::command(slash_command, track_edits)]
pub async fn my_clips(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let avatar = author.avatar_url().unwrap_or_default();
    let u = Arc::clone(ctx.data().users.get(&author.id).unwrap().value());
    let serenity_ctx = ctx.serenity_context().clone();

    // (slot, submit date) per filled slot: buttons are labelled by slot, deletion is by date.
    let (desc, filled_slots) = {
        let ud = u.read().await;
        if !ud.has_submissions() {
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
        let filled: Vec<(usize, chrono::DateTime<chrono::Utc>)> = ud.submits.iter().enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|c| (i, c.date)))
            .collect();
        (ud.get_submissions(SubmissionStyle::Numbered).join("\n"), filled)
    };

    // One button per filled slot, labelled with that slot's keycap (matches the list above).
    let buttons: Vec<_> = filled_slots.iter().filter_map(|&(i, _)| {
        let emoji = ReactionType::Unicode((*data::NUMBER_EMOJS.get(i)?).to_string());
        Some(
        serenity::CreateButton::new(format!("{DELETE_CLIP_PREFIX}{i}"))
            .label("")
            .emoji(emoji)
            .style(serenity::ButtonStyle::Secondary))
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
        .timeout(MY_CLIPS_TIMEOUT)
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

    let Some(i) = press.data.custom_id.strip_prefix(DELETE_CLIP_PREFIX)
        .and_then(|s| s.parse::<usize>().ok())
    else { return Ok(()); };

    let Some(&(_, date)) = filled_slots.iter().find(|(slot, _)| *slot == i) else { return Ok(()); };
    u.write().await.remove_submit(date);

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
    let icon_url = ctx.guild().and_then(|g| g.icon_url()).unwrap_or_default();
    let serenity_ctx = ctx.serenity_context().clone();
    let mod_id = ctx.data().mod_id;

    // Snapshot Arc refs — no shard locks held across await points.
    let user_arcs: Vec<_> = ctx.data().users.iter()
        .map(|e| Arc::clone(e.value()))
        .collect();

    // Collect unrated clip candidates (sequential read, but no DashMap locks held).
    // (owner, clip) — the clip is a snapshot; its date identifies it when the rating is saved.
    let mut candidates: Vec<(Arc<tokio::sync::RwLock<crate::data::UserData>>, ClipData)> = Vec::new();
    for u in &user_arcs {
        let ud = u.read().await;
        for clip in ud.submits.iter().flatten().filter(|c| c.rating.is_none()) {
            candidates.push((Arc::clone(u), clip.clone()));
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

    let (user_arc, clip) = candidates.choose(&mut thread_rng()).unwrap(); // safe: non-empty

    let vote_buttons: Vec<_> = (1..=MAX_CLIP_RATING).map(|i| {
        let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[usize::from(i)].to_string());
        serenity::CreateButton::new(format!("{VOTE_CLIP_PREFIX}{i}"))
            .label("")
            .emoji(emoji)
            .style(serenity::ButtonStyle::Secondary)
    }).collect();

    let components = vec![
        serenity::CreateActionRow::Buttons(vote_buttons),
        serenity::CreateActionRow::Buttons(vec![
            serenity::CreateButton::new(VOTE_DONE_ID)
                .label("Done")
                .style(serenity::ButtonStyle::Danger),
        ]),
    ];

    // Plain message content (not an embed) so Discord previews the video. Mentions are
    // disabled — the title is user text and could otherwise ping @everyone.
    ctx.send(poise::CreateReply::default()
        .content(format!("**{}**\n{}", clip_title_display(&clip.title), clip.link))
        .allowed_mentions(serenity::CreateAllowedMentions::new())
    ).await?;
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
    let mut voted_users: HashMap<serenity::UserId, u8> = HashMap::new();

    loop {
        let Some(press) = msg
            .await_component_interaction(&serenity_ctx)
            .timeout(VOTE_IDLE_TIMEOUT)
            .await
        else {
            break; // idle timeout — finalize with the votes so far
        };

        press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

        if press.data.custom_id == VOTE_DONE_ID {
            let is_mod = press.member.as_ref()
                .is_some_and(|m| m.roles.contains(&mod_id));
            if !is_mod { continue; }
            break;
        }

        let Some(vote) = parse_vote(&press.data.custom_id) else { continue; };

        voted_users.insert(press.user.id, vote);
        let score = average_vote(&voted_users);

        msg.edit(&serenity_ctx, EditMessage::default()
            .embed(serenity::CreateEmbed::new()
                .title("Next Clip")
                .thumbnail(&icon_url)
                .description(format!("Rate this clip!\n\nScore: {score:.2}"))
                .colour(data::EMBED_DEFAULT)
                .footer(default_footer())))
            .await.ok();
    }

    // No votes (idle timeout or Done pressed straight away): leave the clip unrated so it
    // stays in the clip-night queue instead of being filed as a 0/5.
    let description = if voted_users.is_empty() {
        "No votes — this clip stays in the queue.".to_string()
    } else {
        let final_score = average_vote(&voted_users);
        // Voting can run for a long time; the owner may have deleted the clip meanwhile.
        if !user_arc.write().await.rate_submit(clip.date, final_score) {
            tracing::warn!("next_clip: clip was removed during voting — score discarded");
        }
        format!("Final Score: **{final_score:.2}**")
    };

    msg.edit(&serenity_ctx, EditMessage::default()
        .embed(serenity::CreateEmbed::new()
            .title("Next Clip")
            .description(description)
            .thumbnail(&icon_url)
            .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205354792621309972/tenor_5.gif?ex=65d81120&is=65c59c20&hm=b7661397c96231060492b909d1d7f2025bcfa91c166618611f612e95551be35a&")
            .colour(data::EMBED_MOD)
            .footer(default_footer()))
        .components(vec![]))
        .await.ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_url_accepts_youtube_and_medal() {
        assert!(is_youtube_or_medal_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(is_youtube_or_medal_url("youtube.com/watch?v=abc"));
        assert!(is_youtube_or_medal_url("https://youtu.be/abc123"));
        assert!(is_youtube_or_medal_url("https://medal.tv/clips/abc123"));
    }

    #[test]
    fn clip_url_rejects_lookalikes_and_injection() {
        assert!(!is_youtube_or_medal_url("https://evil.example/?youtu.be/x"));
        assert!(!is_youtube_or_medal_url("notyoutube.com/watch?v=x"));
        assert!(!is_youtube_or_medal_url("[click](https://evil)youtu.be/a"));
        assert!(!is_youtube_or_medal_url("https://youtu.be/a b"));
        assert!(!is_youtube_or_medal_url("https://medal.tv/clips/"));
    }

    #[test]
    fn clip_url_rejects_markdown_suffix() {
        assert!(!is_youtube_or_medal_url("https://youtu.be/x)**[FreeNitro](https://evil.example)"));
        assert!(!is_youtube_or_medal_url("https://www.youtube.com/watch?v=x)[a](https://evil)"));
        assert!(!is_youtube_or_medal_url("https://medal.tv/clips/a)[a](https://evil)"));
    }

    #[test]
    fn clip_url_accepts_common_query_and_path_forms() {
        assert!(is_youtube_or_medal_url("https://youtu.be/dQw4w9WgXcQ?si=AbC-12_x&t=30"));
        assert!(is_youtube_or_medal_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=30s"));
        assert!(is_youtube_or_medal_url("https://medal.tv/clips/abc123/d1337xyz?invite=cr-MSx"));
    }

    #[test]
    fn paginate_respects_limit_and_keeps_order() {
        let sections: Vec<String> = ["aaaa", "bbbb", "cccc"].iter().map(ToString::to_string).collect();
        assert_eq!(paginate(sections.clone(), 8), vec!["aaaabbbb", "cccc"]);
        assert_eq!(paginate(sections, 100), vec!["aaaabbbbcccc"]);
        // An oversized section is truncated rather than producing an unsendable page.
        let pages = paginate(vec!["x".repeat(20)], 10);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].chars().count(), 10);
        assert_eq!(paginate(Vec::new(), 10), Vec::<String>::new());
    }

    #[test]
    fn parse_vote_accepts_only_star_range() {
        assert_eq!(parse_vote("vote-clip-1"), Some(1));
        assert_eq!(parse_vote("vote-clip-5"), Some(5));
        assert_eq!(parse_vote("vote-clip-0"), None);
        assert_eq!(parse_vote("vote-clip-9"), None);
        assert_eq!(parse_vote("vote-clip-255"), None);
        assert_eq!(parse_vote(VOTE_DONE_ID), None);
        assert_eq!(parse_vote("delete-clip-1"), None);
    }

    #[test]
    fn paginate_handles_multibyte_and_zero_limit() {
        // Limits count chars, not bytes.
        let pages = paginate(vec!["ﾟ".repeat(5), "é".repeat(5)], 5);
        assert_eq!(pages, vec!["ﾟ".repeat(5), "é".repeat(5)]);
        // A zero limit must not underflow.
        assert_eq!(paginate(vec!["abc".to_string()], 0), vec!["…"]);
    }

    #[test]
    fn average_vote_handles_empty_and_many_voters() {
        let mut votes = HashMap::new();
        assert_eq!(average_vote(&votes), 0.0);
        for id in 1..=60u64 {
            votes.insert(serenity::UserId::new(id), 5);
        }
        assert_eq!(average_vote(&votes), 5.0); // 300 total would overflow a u8 sum
        votes.insert(serenity::UserId::new(1), 1); // re-vote replaces the earlier vote
        assert!((average_vote(&votes) - 296.0 / 60.0).abs() < 5e-5);
    }
}
