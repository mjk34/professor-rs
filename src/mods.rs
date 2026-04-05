//!---------------------------------------------------------------------!
//! This file contains a collection of MODERATOR related commands to    !
//! to better serve the facilitation of professorBot                    !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - give_creds                                                !
//!     [x] - take_creds                                                !
//!     [ ] - give_wishes                                               !
//!     [ ] - refund_tickets                                            !
//!---------------------------------------------------------------------!

use crate::clips::check_mod;
use crate::data::{self, Portfolio, StockProfile, TradeAction, TradeRecord, UserData};
use crate::helper::parse_user_mention;
use crate::{serenity, Context, Error};
use chrono::Utc;
use poise::serenity_prelude::UserId;
use rand::Rng;
use std::sync::Arc;
use tokio::sync::RwLock;

async fn modify_creds(
    ctx: Context<'_>,
    mentioned: String,
    amount: u32,
    is_give: bool,
) -> Result<(), Error> {
    let title = if is_give { "Give Creds" } else { "Take Creds" };

    if amount > 100000 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(title)
                    .description("The max amount allowed is 100000.")
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205685838877433866/tenor_2.gif?ex=65d94570&is=65c6d070&hm=be06433cb7dd2c592468560dfffbc5ce6c294582db38f177028ba80a46f67a43&")
                    .color(data::EMBED_ERROR)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    let guild_members = ctx
        .guild_id()
        .unwrap()
        .members(ctx.http(), None, None)
        .await;

    let mut guild_ids: Vec<UserId> = Vec::new();
    for member in guild_members.iter() {
        for profile in member {
            guild_ids.push(profile.user.id);
        }
    }

    let data = &ctx.data().users;
    let mentioned_list: Vec<&str> = mentioned.split(' ').collect();
    let mentioned_size = mentioned_list.len();

    let mut processed_list: Vec<u64> = Vec::new();
    for mentioned_user in mentioned_list {
        let parsed_id = match parse_user_mention(mentioned_user) {
            Some(id) => id,
            None => continue,
        };
        let user_id = UserId::from(parsed_id);

        if !guild_ids.contains(&user_id) {
            continue;
        }

        if !data.contains_key(&user_id) {
            data.insert(user_id, Default::default());

            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}>", user_id))
                    .embed(
                        serenity::CreateEmbed::new()
                            .title("Account Created!")
                            .description(format!("Welcome <@{}>! You are now registered with ProfessorBot, feel free to checkout Professors Commands in https://discord.com/channels/1194668798830194850/1194700756306108437", user_id))
                            .image("https://gifdb.com/images/high/anime-girl-okay-sign-b5zlye5h8mnjhdg2.gif")
                            .color(data::EMBED_DEFAULT),
                    ),
            )
            .await?;
        }

        let u = data.get(&user_id).unwrap();
        let mut user_data = u.write().await;

        if is_give {
            user_data.add_creds(amount as i32);
        } else {
            user_data.sub_creds(amount as i32);
        }
        processed_list.push(parsed_id);
    }

    let process_size = processed_list.len();
    let mut pre_text = String::new();
    let mut desc = String::new();

    if processed_list.is_empty() {
        desc += if is_give { "No one got creds..." } else { "No one lost creds..." };
    } else {
        let action = if is_give {
            format!("Moderator <@{}> gave {} creds to ", ctx.author().id, amount)
        } else {
            format!("Moderator <@{}> took {} creds from ", ctx.author().id, amount)
        };
        desc += &action;
        for id in processed_list {
            pre_text += &format!("<@{}> ", id);
            desc += &format!("<@{}> ", id);
        }
    }

    let image = if is_give {
        "https://cdn.discordapp.com/attachments/1196582162057662484/1205685388157653022/zVdLFbp.gif?ex=65d94505&is=65c6d005&hm=690faecbed4018602cc94a5f7a9db1ff6527d4202a71ba80f27d912d36de3c7e&"
    } else {
        "https://cdn.discordapp.com/attachments/1196582162057662484/1205689656268824596/7Z7b-ezgif.com-video-to-gif-converter.gif?ex=65d948fe&is=65c6d3fe&hm=d3ac81f31552010a87cb5bb894ebef6af6f2e3fc73c223abff1b19ab712c0ae8&"
    };

    ctx.send(
        poise::CreateReply::default()
            .content(pre_text)
            .embed(
                serenity::CreateEmbed::new()
                    .title(title)
                    .description(desc)
                    .image(image)
                    .color(data::EMBED_MOD)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
    )
    .await?;

    if process_size != mentioned_size {
        let note = if is_give {
            format!(
                "**(!) NOTE** <@{}>\n     **{}** @Mentions did not get processed... double check who did \n     not get creds.\n",
                ctx.author().id, mentioned_size - process_size
            )
        } else {
            format!(
                "**(!) NOTE** <@{}>\n     **{}** @Mentions did not get processed... double check who did \n     not lose creds.\n",
                ctx.author().id, mentioned_size - process_size
            )
        };
        ctx.send(poise::CreateReply::default().content(note)).await?;
    }

    Ok(())
}

/// [!] MODERATOR - reward a user with creds
#[poise::command(slash_command, check = "check_mod")]
pub async fn give_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "amount of creds to give (max: 100000)"] amount: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, amount, true).await
}

/// [!] MODERATOR - take creds from a user
#[poise::command(slash_command, check = "check_mod")]
pub async fn take_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "amount of creds to take (max: 100000)"] amount: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, amount, false).await
}

/// [!] MODERATOR - set a user's level directly
#[poise::command(slash_command, check = "check_mod")]
pub async fn test_set_level(
    ctx: Context<'_>,
    #[description = "@user to update"] mentioned: String,
    #[description = "level to set"] level: i32,
) -> Result<(), Error> {
    let id = match parse_user_mention(&mentioned) {
        Some(id) => UserId::new(id),
        None => {
            ctx.say("Invalid user mention.").await?;
            return Ok(());
        }
    };

    let data = &ctx.data().users;
    match data.get(&id) {
        None => { ctx.say("User not found.").await?; }
        Some(u) => {
            u.write().await.set_level(level);
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Test — Set Level")
                    .description(format!("Set <@{}> to level **{}**.", id, level))
                    .color(data::EMBED_MOD),
            )).await?;
        }
    }

    Ok(())
}

/// [!] MODERATOR - seed fake users for testing leaderboard and portfolio features
#[poise::command(slash_command, check = "check_mod")]
pub async fn test_seed_data(
    ctx: Context<'_>,
    #[description = "number of members to seed (max 20)"] count: u8,
) -> Result<(), Error> {
    let count = count.min(20) as usize;

    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => { ctx.say("Must be run in a server.").await?; return Ok(()); }
    };

    // Fetch real guild members so their IDs resolve in the leaderboard
    let members = ctx.serenity_context().http.get_guild_members(guild_id, Some(100), None).await?;
    let member_ids: Vec<UserId> = members.iter()
        .filter(|m| !m.user.bot)
        .map(|m| m.user.id)
        .take(count)
        .collect();

    if member_ids.is_empty() {
        ctx.say("No members found in this server.").await?;
        return Ok(());
    }

    const TICKERS: &[(&str, f64)] = &[
        ("AAPL", 185.0), ("TSLA", 250.0), ("NVDA", 820.0),
        ("SPY",  520.0), ("AMZN", 185.0), ("MSFT", 415.0),
    ];
    const PORT_NAMES: &[&str] = &["Alpha", "Beta", "Gamma"];

    let seeded: Vec<(UserId, UserData)> = {
        let mut rng = rand::thread_rng();
        member_ids.iter().map(|&id| {
            let mut ud = UserData::default();

            ud.add_creds(rng.gen_range(50_000..2_000_000));

            let roll_count = rng.gen_range(0usize..=7);
            for _ in 0..roll_count {
                let d20 = rng.gen_range(1i32..=20);
                ud.add_rolls(d20);
                ud.push_roll(d20);
                ud.update_daily();
            }

            let port_count = rng.gen_range(1usize..=3);
            let mut stock = StockProfile::default();
            for p in 0..port_count {
                let mut port = Portfolio::new(PORT_NAMES[p].to_string());
                port.cash = rng.gen_range(5_000.0f64..200_000.0);
                stock.portfolios.push(port);

                let trade_count = rng.gen_range(3usize..=6);
                for _ in 0..trade_count {
                    let (ticker, price_usd) = TICKERS[rng.gen_range(0..TICKERS.len())];
                    let price_creds = price_usd * 100.0;
                    let quantity: f64 = rng.gen_range(1.0f64..=10.0);
                    let proceeds = price_creds * quantity;
                    let pnl_pct: f64 = rng.gen_range(-0.30f64..=0.50);
                    let pnl = proceeds * pnl_pct;
                    if proceeds - pnl <= 0.0 { continue; }
                    stock.trade_history.push_back(TradeRecord {
                        portfolio: PORT_NAMES[p].to_string(),
                        ticker: ticker.to_string(),
                        asset_name: ticker.to_string(),
                        action: TradeAction::Sell,
                        quantity,
                        price_per_unit: price_creds,
                        total_creds: proceeds,
                        realized_pnl: Some(pnl),
                        timestamp: Utc::now(),
                    });
                }
            }
            ud.stock = stock;
            (id, ud)
        }).collect()
    }; // rng dropped here

    let users = &ctx.data().users;
    let inserted = seeded.len();
    for (id, ud) in seeded {
        tracing::info!("seed_test_data: seeding member {}", id);
        users.insert(id, Arc::new(RwLock::new(ud)));
    }

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Test Data Seeded")
            .description(format!("Seeded **{}** guild members with randomized data.", inserted))
            .color(data::EMBED_MOD)
            .footer(serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo")),
    )).await?;

    Ok(())
}
