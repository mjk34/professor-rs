use std::collections::HashMap;

use chrono::prelude::Utc;
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};

use crate::serenity;
use crate::{Context, Error};
use serenity::Color;

// Ping the bot to see if its alive or to play ping pong
#[poise::command(prefix_command, slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let pong_image = ctx.data().pong.choose(&mut thread_rng()).unwrap();
    let latency: f32 =
        (ctx.created_at().time() - Utc::now().time()).num_milliseconds() as f32 / 1000.0;

    ctx.send(
        poise::CreateReply::default()
            // .content("Pong!")
            .embed(
                serenity::CreateEmbed::new()
                    .title("Pong!")
                    .description(format!(
                        "Right back at you <@{}>! ProfessorBot is live! ({}s)",
                        author.id, latency
                    ))
                    .image(pong_image),
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

#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let mut data = ctx.data().users.lock().await;
    let user_data = data.get_mut(&user.id).unwrap();

    let ponder_image = ctx.data().ponder.choose(&mut thread_rng()).unwrap();

    //TODO: match original
    let num = thread_rng().gen_range(0..100);

    if !user_data.check_daily() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description("Your next **/uwu** is tomorrow")
                    .thumbnail(format!("{}", user.avatar_url().unwrap_or_default())),
            ),
        )
        .await?;
        return Ok(());
    }
    user_data.add_creds(num);
    user_data.update_daily();

    let pog_str = if num > 70 {
        "Super Pog!"
    } else if num > 50 {
        "Pog!"
    } else {
        "Sadge..."
    };

    let desc = format!("**{} +{}** added to your Wallet!", pog_str, num);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Daily")
                .description(desc)
                .thumbnail(format!("{}", user.avatar_url().unwrap_or_default()))
                .color(Color::GOLD)
                .image(ponder_image),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn wallet(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = ctx.data().users.lock().await;
    let user_data = data.get(&user.id).unwrap();

    let desc = format!("Total Creds: **{}**", user_data.get_creds());

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wallet")
                .description(desc)
                .thumbnail(format!("{}", user.avatar_url().unwrap_or_default()))
                .color(Color::GOLD),
        ),
    )
    .await?;

    Ok(())
}
#[poise::command(slash_command)]
pub async fn voice_status(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data().voice_users.lock().await;

    let mut out: Vec<_> = data.iter().collect();
    out.sort_by(|a, b| a.1.joined.cmp(&b.1.joined));

    let now = chrono::Utc::now();

    let embed = if !out.is_empty() {
        let mut embed = serenity::CreateEmbed::new()
            .title("Voice Status")
            .color(Color::GOLD)
            .thumbnail(ctx.guild().unwrap().icon_url().unwrap_or_default());

        for (a, b) in out.iter() {
            let u = a.to_user(&ctx).await?;
            let diff = now - b.joined;
            let minutes = ((diff.num_seconds()) / 60) % 60;
            let hours = (diff.num_seconds() / 60) / 60;

            let mut user_info = format!("{:0>2}:{:0>2}", hours, minutes);

            if let Some(mute_time) = b.mute {
                let mute_duration = now - mute_time;
                let mute_minutes = ((mute_duration.num_seconds()) / 60) % 60;
                let mute_hours = (mute_duration.num_seconds() / 60) / 60;
                user_info += &format!(" | Mute: {:0>2}:{:0>2}", mute_hours, mute_minutes);
            }
            if let Some(deaf_time) = b.deaf {
                let deaf_duration = now - deaf_time;
                let deaf_minutes = ((deaf_duration.num_seconds()) / 60) % 60;
                let deaf_hours = (deaf_duration.num_seconds() / 60) / 60;
                user_info += &format!(" | Deaf: {:0>2}:{:0>2}", deaf_hours, deaf_minutes);
            }

            embed = embed.field(u.name, user_info, false);
        }

        embed
    } else {
        serenity::CreateEmbed::new()
            .title("Voice Status")
            .description("No one in voice")
            .color(Color::GOLD)
    };

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn info(ctx: Context<'_>) -> Result<(), Error> {
    let guild = match ctx.guild() {
        Some(guild) => guild.clone(),
        None => return Ok(()), // Exit if not in a guild
    };

    // Extract necessary data
    let guild_name = guild.name.clone();
    let icon_url = guild.icon_url().unwrap_or_default();
    let banner_url = guild.banner_url().unwrap_or_default();
    let member_count = guild.member_count;
    let creation_date = guild.id.created_at().format("%Y-%m-%d").to_string();
    let num_roles = guild.roles.len();
    let pub_channels: HashMap<&serenity::ChannelId, &serenity::GuildChannel> = guild
        .channels
        .iter()
        .filter(|(_, b)| b.permission_overwrites.len() == 0)
        .collect();
    let num_channels = pub_channels.len();
    let verification_level = format!("{:?}", guild.verification_level);
    let boost_level = format!("{:?}", guild.premium_tier);
    let num_boosts = guild.premium_subscription_count.unwrap_or(0);
    let emojis = guild
        .emojis
        .values()
        .map(|e| e.to_string())
        .collect::<Vec<String>>()
        .join(" ");

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title(&guild.name)
                .thumbnail(&icon_url)
                .image(&banner_url)
                .description(format!(
                    "Welcome to **{}**!\n\n**Member Count:** {}\n**Created On:** {}\n**Roles:** {}\n**Channels:** {}\n**Verification Level:** {}\n**Boost Level:** {}\n**Number of Boosts:** {}\n\n**Emojis:**\n{}",
                    guild_name,
                    member_count,
                    creation_date,
                    num_roles,
                    num_channels,
                    verification_level,
                    boost_level,
                    num_boosts,
                    emojis
                ))
                .colour(Color::DARK_BLUE),
        ),
    )
    .await?;

    Ok(())
}
