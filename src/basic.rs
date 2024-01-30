use std::collections::HashMap;

use chrono::prelude::Utc;
use openai_api_rs::v1::error::APIError;
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};

use crate::serenity;
use crate::{Context, Error};
use serenity::Color;

use openai_api_rs::v1::api::Client;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use openai_api_rs::v1::common::GPT3_5_TURBO_16K;

/// ping the bot to see if its alive or to play ping pong
#[poise::command(prefix_command, slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let pong_image = ctx.data().pong.choose(&mut thread_rng()).unwrap();
    let latency: f32 =
        (ctx.created_at().time() - Utc::now().time()).num_milliseconds() as f32 / 1000.0;

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Pong!")
                .description(format!(
                    "Right back at you <@{}>! ProfessorBot is live! ({}s)",
                    author.id, latency
                ))
                .color(Color::new(16119285))
                .image(pong_image)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

// use gpt-3.5-turbo to generate fun responses to user prompts
async fn gpt_string(ctx: Context<'_>, prompt: String) -> Result<String, APIError> {
    let api_key = &ctx.data().gpt_key;
    let client = Client::new(api_key.to_string());

    let req = ChatCompletionRequest::new(
        GPT3_5_TURBO_16K.to_string(),
        vec![chat_completion::ChatCompletionMessage {
            role: chat_completion::MessageRole::user,
            content: chat_completion::Content::Text(prompt),
            name: None,
        }],
    );

    let result = client.chat_completion(req)?;
    let desc = format!(
        "{:?}",
        result.choices[0]
            .message
            .content
            .as_ref()
            .unwrap()
            .to_string()
    );

    Ok(desc.replace(['\"', '\\'], ""))
}

/// claim your daily, 500xp, and 2 wishes (Once a day)
#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let mut user_data = u.write().await;

    // check if daily is available
    if !user_data.check_daily() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description("Your next **/uwu** is tomorrow")
                    .thumbnail(user.avatar_url().unwrap_or_default()),
            ),
        )
        .await?;
        return Ok(());
    }

    let d20 = thread_rng().gen_range(1..21);
    let check = thread_rng().gen_range(6..15);

    let bonus = 0; // change this to scale with level

    let low = (check - 1) * 50;
    let high = check * 50;
    let fortune = thread_rng().gen_range(low..high);

    let total: i32;
    let roll_str: String;
    let roll_context: String;
    let roll_color: Color;

    if d20 == 20 {
        total = 1200;
        roll_str = "**Critical Success!!**".to_string();
        roll_context = "+".to_string();
        roll_color = Color::GOLD;
    } else if d20 == 1 {
        total = fortune;
        roll_str = "**Critical Failure!**".to_string();
        roll_context = "-".to_string();
        roll_color = Color::RED;
    } else if d20 >= check {
        total = fortune;
        roll_str = "Yippee, you passed.".to_string();
        roll_context = "+".to_string();
        roll_color = Color::new(65280);
    } else {
        total = fortune / 2;
        roll_str = "*oof*, you failed...".to_string();
        roll_context = "+".to_string();
        roll_color = Color::new(6053215);
    };

    let base_ref = ctx.data().d20f.get(28);
    let roll_ref = if d20 == 20 || d20 == 1 {
        ctx.data().d20f.get((d20 - 1) as usize)
    } else {
        ctx.data().d20f.get((d20 + bonus - 1) as usize)
    };

    // generate daily orb/animeme
    let random_meme = thread_rng().gen_range(0..100);
    let ponder_image = if random_meme < 50 {
        "https://cdn.discordapp.com/attachments/1196582162057662484/1196877964642623509/pondering-my-orb-header-art.png?ex=65b93a77&is=65a6c577&hm=9dcde7ef0ecd61463f39f2077311bbb52db20b4416609cbbe2c5028510f2047c&"
    } else if (50..75).contains(&random_meme) {
        ctx.data().ponder.choose(&mut thread_rng()).unwrap()
    } else {
        ctx.data().meme.choose(&mut thread_rng()).unwrap()
    };

    // temporary message to roll the dice
    let desc = format!("---\nYou needed a **{}** to pass...\n\n---\n---", check);
    let reply = ctx
        .send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description(&desc)
                    .thumbnail(base_ref.unwrap().to_string())
                    .color(Color::new(16119285))
                    .image(ponder_image)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    // generate fortune readings with gpt3.5
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    let prompt = if d20 == 1 {
        "give me a bad fortune that's funny, only the fortune, no quotes, like a fortune cookie, less than 20 words"
    } else {
        "give me a good fortune that's funny, only the fortune, no quotes, like a fortune cookie, less than 20 words"
    };

    let mut tries = 0;
    let reading;
    loop {
        match gpt_string(ctx, prompt.to_string()).await {
            Ok(result) => {
                reading = result;
                break;
            }
            Err(e) => {
                println!("An error occurred: {:?}, retrying...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                if tries > 5 {
                    return Err(Box::new(e));
                }
            }
        }
        tries += 1;
    }

    // final message with updated dice roll, creds earned and fortune reading
    let desc = format!(
        "{} **{}{}** creds.\nYou needed a **{}** to pass, you rolled a **{}**.\n\n{:?}",
        roll_str, roll_context, total, check, d20, reading,
    );

    reply
        .edit(
            ctx,
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description(&desc)
                    .thumbnail(roll_ref.unwrap().to_string())
                    .color(roll_color)
                    .image(ponder_image)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    if d20 == 1 {
        user_data.sub_creds(total);
    } else {
        user_data.add_creds(total);
    }

    user_data.update_xp(500);
    user_data.add_rolls(d20);
    user_data.add_bonus();
    user_data.update_daily();

    Ok(())
}

/// claim bonus creds for every three dailies
#[poise::command(slash_command)]
pub async fn claim_bonus(ctx: Context<'_>) -> Result<(), Error> {
    // update this to implement a d20 dice roll + bonus from level

    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get_mut(&user.id).unwrap();
    let mut user_data = u.write().await;

    let bonus = user_data.get_bonus();
    if user_data.check_claim() {
        let d20 = thread_rng().gen_range(1..21);
        let proficiency = 2 + user_data.get_level() / 8;
        let base_ref = ctx.data().d20f.get(28);

        // temporary message to roll the dice
        let desc = format!(
            "Rolling for Bonus loot, you get a **+{}** fortune modifier.\n---\n",
            proficiency
        );
        let reply = ctx
            .send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Claim Bonus")
                        .description(&desc)
                        .thumbnail(base_ref.unwrap().to_string())
                        .color(Color::new(16119285))
                        .image("https://cdn.discordapp.com/attachments/1196582162057662484/1197008145868918854/de6b5df29abaf7124387b9c86ca46a29.gif?ex=65b9b3b5&is=65a73eb5&hm=b36eb6f0e235b2ca8d37339cd541e55ea397cdf4be5cc080da4bd37cd99c6c3d&")
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let low = (d20 + proficiency - 1) * 40;
        let high = (d20 + proficiency) * 40;
        let fortune = thread_rng().gen_range(low..high);
        let roll_ref = ctx.data().d20f.get((d20 + proficiency - 1) as usize); // make more dice face

        // final message with updated dice roll and creds
        let desc = format!(
            "You rolled a **{}** and obtained **+{}** creds.",
            d20 + proficiency,
            fortune
        );

        reply
            .edit(
                ctx,
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Claim Bonus")
                        .description(&desc)
                        .thumbnail(roll_ref.unwrap().to_string())
                        .color(Color::new(6943230))
                        .image("https://cdn.discordapp.com/attachments/1196582162057662484/1197008145868918854/de6b5df29abaf7124387b9c86ca46a29.gif?ex=65b9b3b5&is=65a73eb5&hm=b36eb6f0e235b2ca8d37339cd541e55ea397cdf4be5cc080da4bd37cd99c6c3d&")
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;

        user_data.add_creds(fortune);
        user_data.reset_bonus();
    } else {
        let desc: String = match bonus {
            2 => {
                format!(
                    "The ***Bonus*** will be ready after your next `/uwu`! (Claim Bonus: {})",
                    bonus
                )
            }
            _ => {
                format!("The ***Bonus*** is not ready! (Claim Bonus: {})", bonus)
            }
        };

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Claim Bonus")
                    .description(desc)
                    .color(Color::new(6053215))
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    ))
                    .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1197004718631833650/tenor.gif?ex=65b9b084&is=65a73b84&hm=0368979e5bdf0c258f6b344ec2b79826459b3ec4c937374e05ec77f131adf37f&"),
            ),
        )
        .await?;
    }
    Ok(())
}

/// check how many creds, wishes, or submits you have
#[poise::command(slash_command)]
pub async fn wallet(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    // get user info
    let user_luck: String = if user_data.get_luck() == "" {
        "---".to_string()
    } else {
        user_data.get_luck()
    };

    let has_daily: String = if user_data.check_daily() {
        "Available".to_string()
    } else {
        "Not Available".to_string()
    };

    let has_claim: String = if user_data.check_claim() {
        "Available".to_string()
    } else {
        format!("{} / 3", user_data.get_bonus())
    };

    let user_creds: i32 = user_data.get_creds();

    let desc = format!(
        "Daily UwU........... . . **{}**\nAverage Luck..... . . **{}**\nClaim Bonus....... . . **{}**\n\nTotal Creds: **{}**\n", 
        has_daily, user_luck, has_claim, user_creds
    );

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wallet")
                .description(desc)
                .thumbnail(user.avatar_url().unwrap_or_default().to_string())
                .color(Color::new(16119285))
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

/// show the top wealthiest users in the server
#[poise::command(slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let data = &ctx.data().users;

    // let info = data.iter().map(|x| {
    // let (id, u) = x.pair();
    // let u = u.read().await;
    // });

    let mut info = Vec::new();

    for x in data.iter() {
        let (id, u) = x.pair();
        let u = u.read().await;
        info.push((*id, u.get_creds()));
    }

    info.sort_by(|a, b| b.1.cmp(&a.1));

    let mut leaderboard_text = String::new();
    for (index, (id, u)) in info.iter().enumerate().take(10) {
        let user_name = id.to_user(ctx).await?.name;
        let score = if index == 0 {
            format!("- {}", u)
        } else {
            "".to_string()
        };
        leaderboard_text.push_str(&format!("**#{}**: {} {}\n", index + 1, user_name, score));
    }

    let embed = serenity::CreateEmbed::new()
        .title("Leaderboard")
        .color(Color::TEAL)
        .thumbnail(
            info[0]
                .0
                .to_user(ctx)
                .await?
                .avatar_url()
                .unwrap_or_default(),
        )
        .description("Here lists the most accomplished in UwUversity!")
        .field("Rankings", leaderboard_text, false)
        .footer(serenity::CreateEmbedFooter::new(
            "@~ powered by UwUntu & RustyBamboo",
        ));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

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
        .filter(|(_, b)| b.permission_overwrites.is_empty())
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
                .colour(Color::DARK_BLUE)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}
