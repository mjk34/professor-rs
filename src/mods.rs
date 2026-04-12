//! Moderator commands: `give_creds`, `take_creds`, and test-seeding utilities.

use crate::clips::check_mod;
use crate::data::{self, Portfolio, StockProfile, TradeAction, TradeRecord, UserData};
use crate::helper::{default_footer, parse_user_mention, price_to_creds};
use crate::{serenity, Context, Error};
use chrono::Utc;
use poise::serenity_prelude::UserId;
use rand::Rng;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum cred amount a moderator may transfer in a single give/take operation.
const MAX_CRED_TRANSFER: u32 = 100_000;

#[derive(Copy, Clone)]
enum CreditOp { Give, Take }

async fn modify_creds(
    ctx: Context<'_>,
    mentioned: String,
    dollars: u32,
    op: CreditOp,
) -> Result<(), Error> {
    let title = match op {
        CreditOp::Give => "Give Creds",
        CreditOp::Take => "Take Creds",
    };

    if dollars == 0 || dollars > MAX_CRED_TRANSFER {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(title)
                    .description("Amount must be between $1 and $100,000.")
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205685838877433866/tenor_2.gif?ex=65d94570&is=65c6d070&hm=be06433cb7dd2c592468560dfffbc5ce6c294582db38f177028ba80a46f67a43&")
                    .color(data::EMBED_ERROR)
                    .footer(default_footer()),
            ),
        )
        .await?;
        return Ok(());
    }

    let amount = (dollars * 100) as i32;

    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("This command can only be used in a server.").await?;
        return Ok(());
    };
    let guild_members = guild_id.members(ctx.http(), None, None).await;

    let mut guild_ids: Vec<UserId> = Vec::new();
    if let Ok(members) = &guild_members {
        for profile in members {
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
            data.insert(user_id, Arc::default());

            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{user_id}>"))
                    .embed(
                        serenity::CreateEmbed::new()
                            .title("Account Created!")
                            .description(format!("Welcome <@{user_id}>! You are now registered with ProfessorBot, feel free to checkout Professors Commands in https://discord.com/channels/1194668798830194850/1194700756306108437"))
                            .image("https://gifdb.com/images/high/anime-girl-okay-sign-b5zlye5h8mnjhdg2.gif")
                            .color(data::EMBED_DEFAULT),
                    ),
            )
            .await?;
        }

        let u = data.get(&user_id).unwrap();
        let mut user_data = u.write().await;

        match op {
            CreditOp::Give => { user_data.add_creds(amount); }
            CreditOp::Take => { user_data.sub_creds(amount); }
        }
        processed_list.push(parsed_id);
    }

    let process_size = processed_list.len();
    let mut pre_text = String::new();
    let mut desc = String::new();

    if processed_list.is_empty() {
        desc += match op {
            CreditOp::Give => "No one got creds...",
            CreditOp::Take => "No one lost creds...",
        };
    } else {
        let action = match op {
            CreditOp::Give => format!("Moderator <@{}> gave ${} to ", ctx.author().id, dollars),
            CreditOp::Take => format!("Moderator <@{}> took ${} from ", ctx.author().id, dollars),
        };
        desc += &action;
        for id in processed_list {
            pre_text += &format!("<@{id}> ");
            desc += &format!("<@{id}> ");
        }
    }

    let image = match op {
        CreditOp::Give => "https://cdn.discordapp.com/attachments/1196582162057662484/1205685388157653022/zVdLFbp.gif?ex=65d94505&is=65c6d005&hm=690faecbed4018602cc94a5f7a9db1ff6527d4202a71ba80f27d912d36de3c7e&",
        CreditOp::Take => "https://cdn.discordapp.com/attachments/1196582162057662484/1205689656268824596/7Z7b-ezgif.com-video-to-gif-converter.gif?ex=65d948fe&is=65c6d3fe&hm=d3ac81f31552010a87cb5bb894ebef6af6f2e3fc73c223abff1b19ab712c0ae8&",
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
                    .footer(default_footer()),
            ),
    )
    .await?;

    if process_size != mentioned_size {
        let verb = match op {
            CreditOp::Give => "get",
            CreditOp::Take => "lose",
        };
        let note = format!(
            "**(!) NOTE** <@{}>\n     **{}** @Mentions did not get processed... double check who did \n     not {} creds.\n",
            ctx.author().id, mentioned_size - process_size, verb
        );
        ctx.send(poise::CreateReply::default().content(note)).await?;
    }

    Ok(())
}

/// [!] MODERATOR - reward a user with creds
#[poise::command(slash_command, check = "check_mod")]
pub async fn give_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "dollar amount to give (max: $100,000)"] dollars: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, dollars, CreditOp::Give).await
}

/// [!] MODERATOR - take creds from a user
#[poise::command(slash_command, check = "check_mod")]
pub async fn take_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "dollar amount to take (max: $100,000)"] dollars: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, dollars, CreditOp::Take).await
}


/// [!] MODERATOR - seed fake users for testing leaderboard and portfolio features
#[poise::command(slash_command, check = "check_mod")]
pub async fn test_seed_data(
    ctx: Context<'_>,
    #[description = "number of members to seed (max 20)"] count: u8,
) -> Result<(), Error> {
    let count = count.min(20) as usize;

    let guild_id = if let Some(id) = ctx.guild_id() { id } else { ctx.say("Must be run in a server.").await?; return Ok(()); };

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
            for port_name in PORT_NAMES.iter().take(port_count) {
                let mut port = Portfolio::new(port_name.to_string());
                port.cash = rng.gen_range(5_000.0f64..200_000.0);
                stock.portfolios.push(port);

                let trade_count = rng.gen_range(3usize..=6);
                for _ in 0..trade_count {
                    let (ticker, price_usd) = TICKERS[rng.gen_range(0..TICKERS.len())];
                    let price_creds = price_to_creds(price_usd);
                    let quantity: f64 = rng.gen_range(1.0f64..=10.0);
                    let proceeds = price_creds * quantity;
                    let pnl_pct: f64 = rng.gen_range(-0.30f64..=0.50);
                    let pnl = proceeds * pnl_pct;
                    if proceeds - pnl <= 0.0 { continue; }
                    stock.trade_history.push_back(TradeRecord {
                        portfolio: port_name.to_string(),
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
        tracing::info!(user_id = %id, "seed_test_data: seeding member");
        users.insert(id, Arc::new(RwLock::new(ud)));
    }

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Test Data Seeded")
            .description(format!("Seeded **{inserted}** guild members with randomized data."))
            .color(data::EMBED_MOD)
            .footer(default_footer()),
    )).await?;

    Ok(())
}
