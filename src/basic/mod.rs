//! Core commands — ping, wallet, `voice_status`, info, and the economy/leaderboard sub-modules.

mod economy;
mod leaderboard;

#[doc(inline)] pub use economy::{buy_tickets, claim_bonus, simulate_claim, simulate_uwu, uwu};
#[doc(inline)] pub use leaderboard::leaderboard;

use crate::{data, serenity, Context, Error};
use crate::helper::{creds_to_price, default_footer};
use poise::serenity_prelude::UserId;
use serenity::Color;
use std::collections::HashMap;

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
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let luck: String = if user_data.get_luck().is_empty() { "N/A".to_string() } else { user_data.get_luck() };
    let daily = if user_data.check_daily() { "Available".to_string() } else { "Not Available".to_string() };
    let claim = if user_data.check_claim() { "Available".to_string() } else { format!("{} / 3", user_data.get_bonus()) };

    let level: i32 = user_data.get_level();
    let xp: i32 = user_data.get_xp();
    let next_level = user_data.get_next_level();
    let creds: i32 = user_data.get_creds();
    let tickets: i32 = user_data.get_tickets();
    let gold_badge = if level >= data::GOLD_LEVEL_THRESHOLD { "  ⭐ **Gold Status**" } else { "" };

    let desc = format!(
        "**Level {}**{}  -  {}/{}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\nDaily UwU........... . . . **{}**\nAverage Luck..... . . . **{}**\nClaim Bonus....... . . . **{}**\n\nTotal Creds: **{}** (${:.2}) \u{3000}\u{3000}\u{2000}Tickets: **{}**\n",
        level, gold_badge, xp, next_level, daily, luck, claim, creds, creds_to_price(f64::from(creds)), tickets
    );

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Wallet")
            .description(desc)
            .thumbnail(user.avatar_url().unwrap_or_default().clone())
            .color(data::EMBED_GOLD)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

#[poise::command(slash_command, guild_only)]
pub async fn voice_status(ctx: Context<'_>) -> Result<(), Error> {
    use crate::data::VoiceUser;
    let data = &ctx.data().voice_users;

    let mut out: Vec<(UserId, VoiceUser)> = data.iter().map(|x| (*x.key(), x.value().clone())).collect();
    out.sort_by(|a, b| a.1.joined.cmp(&b.1.joined));

    let now = chrono::Utc::now();

    let embed = if out.is_empty() {
        serenity::CreateEmbed::new()
            .title("Voice Status")
            .description("No one in voice")
            .color(data::EMBED_ERROR)
    } else {
        let mut embed = serenity::CreateEmbed::new()
            .title("Voice Status")
            .color(Color::GOLD)
            .thumbnail(ctx.guild().unwrap().icon_url().unwrap_or_default());

        for (a, b) in &out {
            let u = a.to_user(&ctx).await?;
            let diff = now - b.joined;
            let minutes = ((diff.num_seconds()) / 60) % 60;
            let hours = (diff.num_seconds() / 60) / 60;

            let mut user_info = format!("{hours:0>2}:{minutes:0>2}");

            if let Some(mute_time) = b.mute {
                let mute_duration = now - mute_time;
                let mute_minutes = ((mute_duration.num_seconds()) / 60) % 60;
                let mute_hours = (mute_duration.num_seconds() / 60) / 60;
                user_info += &format!(" | Mute: {mute_hours:0>2}:{mute_minutes:0>2}");
            }
            if let Some(deaf_time) = b.deaf {
                let deaf_duration = now - deaf_time;
                let deaf_minutes = ((deaf_duration.num_seconds()) / 60) % 60;
                let deaf_hours = (deaf_duration.num_seconds() / 60) / 60;
                user_info += &format!(" | Deaf: {deaf_hours:0>2}:{deaf_minutes:0>2}");
            }

            embed = embed.field(u.name, user_info, false);
        }

        embed
    };

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
pub async fn info(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()),
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
        if raw.len() > MAX_EMOJI_LEN {
            let cut = raw[..MAX_EMOJI_LEN].rfind(' ').unwrap_or(MAX_EMOJI_LEN);
            format!("{}...", &raw[..cut])
        } else {
            raw
        }
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
