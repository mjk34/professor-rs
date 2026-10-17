//! Core commands — ping, wallet, `voice_status`, info, and the economy/leaderboard sub-modules.

mod economy;
mod leaderboard;

#[doc(inline)] pub use economy::{buy_tickets, claim_bonus, uwu};
#[doc(inline)] pub use leaderboard::leaderboard;

use crate::{data, serenity, Context, Error};
use crate::helper::{creds_to_price, default_footer, fetch_users, user_name};
use poise::serenity_prelude::UserId;
use std::collections::HashMap;
use std::sync::Arc;

/// Cuts `text` to at most `max` bytes at the last space (on a char boundary), adding "...".
fn truncate_at_space(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let cut = text[..end].rfind(' ').unwrap_or(end);
    format!("{}...", &text[..cut])
}

fn fmt_hhmm(d: chrono::TimeDelta) -> String {
    let total_min = d.num_seconds().max(0) / 60; // clamp clock skew (now < joined)
    format!("{:0>2}:{:0>2}", total_min / 60, total_min % 60)
}

/// ping the bot to see if its alive or to play ping pong
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    use rand::seq::SliceRandom;
    use rand::thread_rng;
    use chrono::prelude::Utc;
    let pong_image = ctx.data().pong.choose(&mut thread_rng()).map_or("", std::string::String::as_str);
    let latency = (Utc::now() - *ctx.created_at()).num_milliseconds() as f32 / 1000.0;

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Pong!")
            .description(format!("Right back at you <@{}>! ProfessorBot is live! ({}s)", author.id, latency))
            .color(data::EMBED_CYAN)
            .image(pong_image)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

/// check your creds, tickets, and submits
#[poise::command(slash_command)]
pub async fn wallet(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let u = Arc::clone(ctx.data().users.get(&user.id).unwrap().value());
    // Scoped so the read guard is released before the Discord send below.
    let desc = {
        let user_data = u.read().await;
        let daily = if user_data.check_daily() { "Available".to_string() } else { "Not Available".to_string() };
        let claim = if user_data.check_claim() { "Available".to_string() } else { format!("{} / {}", user_data.get_bonus(), data::BONUS_CLAIM_THRESHOLD) };
        let creds = user_data.get_creds();

        format!(
            "**Level {}**  -  {}/{}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\nDaily UwU........... . . . **{}**\nAverage Luck..... . . . **{}**\nClaim Bonus....... . . . **{}**\n\nTotal Creds: **{}** (${:.2}) \u{3000}\u{3000}\u{2000}Tickets: **{}**\n",
            user_data.get_level(), user_data.get_xp(), user_data.get_next_level(), daily, user_data.get_luck(), claim,
            creds, creds_to_price(f64::from(creds)), user_data.get_tickets()
        )
    };

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Wallet")
            .description(desc)
            .thumbnail(user.avatar_url().unwrap_or_default())
            .color(data::EMBED_GOLD)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

/// see who is in voice and for how long
#[poise::command(slash_command, guild_only)]
pub async fn voice_status(ctx: Context<'_>) -> Result<(), Error> {
    use crate::data::VoiceUser;
    let data = &ctx.data().voice_users;

    let mut out: Vec<(UserId, VoiceUser)> = data.iter().map(|x| (*x.key(), x.value().clone())).collect();
    out.sort_by_key(|(_, vu)| vu.joined);

    let now = chrono::Utc::now();

    let embed = if out.is_empty() {
        serenity::CreateEmbed::new()
            .title("Voice Status")
            .description("No one in voice")
            .color(data::EMBED_ERROR)
    } else {
        let users = fetch_users(ctx.serenity_context(), out.iter().map(|(id, _)| *id)).await;

        let mut embed = serenity::CreateEmbed::new()
            .title("Voice Status")
            .color(data::EMBED_GOLD)
            .thumbnail(ctx.guild().and_then(|g| g.icon_url()).unwrap_or_default());

        for ((_, vu), user_result) in out.iter().zip(users) {
            let name = user_name(user_result.as_ref());
            let mut user_info = fmt_hhmm(now - vu.joined);
            if let Some(mute_time) = vu.mute {
                user_info += &format!(" | Mute: {}", fmt_hhmm(now - mute_time));
            }
            if let Some(deaf_time) = vu.deaf {
                user_info += &format!(" | Deaf: {}", fmt_hhmm(now - deaf_time));
            }

            embed = embed.field(name, user_info, false);
        }

        embed
    };

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// show info about this server
#[poise::command(slash_command, guild_only)]
pub async fn info(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild) = ctx.guild().map(|g| g.clone()) else {
        ctx.say("Server info isn't available right now — try again in a moment.").await?;
        return Ok(());
    };

    let guild_name = guild.name.clone();
    let icon_url = guild.icon_url().unwrap_or_default();
    let banner_url = guild.banner_url().unwrap_or_default();
    let member_count = guild.member_count;
    let creation_date = guild.id.created_at().format("%Y-%m-%d").to_string();
    let num_roles = guild.roles.len();
    let pub_channels: HashMap<&serenity::ChannelId, &serenity::GuildChannel> = guild
        .channels
        .iter()
        .filter(|(_, b)| b.permission_overwrites.is_empty())
        .collect();
    let num_channels = pub_channels.len();
    let verification_level = format!("{:?}", guild.verification_level);
    let boost_level = format!("{:?}", guild.premium_tier);
    let num_boosts = guild.premium_subscription_count.unwrap_or(0);
    let emojis = {
        let raw = guild.emojis.values()
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(" ");
        const MAX_EMOJI_LEN: usize = 3700;
        truncate_at_space(&raw, MAX_EMOJI_LEN)
    };

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::default()
            .title(&guild.name)
            .thumbnail(&icon_url)
            .image(&banner_url)
            .description(format!(
                "Welcome to **{guild_name}**!\n\n**Member Count:** {member_count}\n**Created On:** {creation_date}\n**Roles:** {num_roles}\n**Channels:** {num_channels}\n**Verification Level:** {verification_level}\n**Boost Level:** {boost_level}\n**Number of Boosts:** {num_boosts}\n\n**Emojis:**\n{emojis}"
            ))
            .colour(data::EMBED_DEFAULT)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_hhmm_formats_and_clamps() {
        assert_eq!(fmt_hhmm(chrono::TimeDelta::minutes(5)), "00:05");
        assert_eq!(fmt_hhmm(chrono::TimeDelta::minutes(61)), "01:01");
        assert_eq!(fmt_hhmm(chrono::TimeDelta::hours(100)), "100:00");
        assert_eq!(fmt_hhmm(chrono::TimeDelta::minutes(-3)), "00:00"); // clock skew
    }

    #[test]
    fn truncate_at_space_cuts_on_word_and_char_boundaries() {
        assert_eq!(truncate_at_space("short", 10), "short");
        assert_eq!(truncate_at_space("aaa bbb ccc", 9), "aaa bbb...");
        assert_eq!(truncate_at_space("ééééé", 3), "é..."); // byte 3 is mid-char
    }
}
