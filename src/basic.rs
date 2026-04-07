//!---------------------------------------------------------------------!
//! This file contains a collection of commands that is fundamental     !
//! to professorBot's functionality and purpose                         !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - ping                                                      !
//!     [x] - uwu                                                       !
//!     [x] - claim_bonus                                               !
//!     [-] - wallet                                                    !
//!     [x] - leaderboard                                               !
//!     [x] - buy_tickets                                               !
//!     [x] - voice_status                                              !
//!     [x] - info                                                      !
//!---------------------------------------------------------------------!

use crate::data::{self, VoiceUser};
use crate::helper::default_footer;
use crate::{serenity, Context, Error};
use chrono::prelude::Utc;
use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::{EditMessage, ReactionType, UserId};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// ping the bot to see if its alive or to play ping pong
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let pong_image = ctx.data().pong.choose(&mut thread_rng()).unwrap();
    let latency = (Utc::now() - *ctx.created_at()).num_milliseconds() as f32 / 1000.0;

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Pong!")
                .description(format!(
                    "Right back at you <@{}>! ProfessorBot is live! ({}s)",
                    author.id, latency
                ))
                .color(data::EMBED_CYAN)
                .image(pong_image)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}


async fn send_gold_unlock(ctx: Context<'_>) -> Result<(), crate::Error> {
    let user = ctx.author();
    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("⭐ Gold Status Unlocked!")
                .description(format!(
                    "Congratulations <@{}>! You've reached **Level 10** and unlocked **Gold Status**!\n\nYou now earn a higher HYSA rate on uninvested portfolio cash.",
                    user.id
                ))
                .thumbnail(user.avatar_url().unwrap_or_default())
                .color(data::EMBED_GOLD)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

/// claim your daily, 500xp (Once a day)
#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();

    // Compute everything before acquiring the write lock
    let d20 = thread_rng().gen_range(1..21);
    let check = thread_rng().gen_range(6..15);

    let bonus = 0; // change this to scale with level

    let low = 2_000 + (check - 1) * 600;
    let high = 2_000 + check * 600;
    let fortune = thread_rng().gen_range(low..high);

    let total: i32;
    let roll_str: String;
    let roll_context: String;
    let roll_color: Color;

    if d20 == 20 {
        total = thread_rng().gen_range(50_000..200_000);
        roll_str = "**Critical Success!!**".to_string();
        roll_context = "+".to_string();
        roll_color = data::EMBED_GOLD;
    } else if d20 == 1 {
        total = thread_rng().gen_range(15_000..40_000);
        roll_str = "**Critical Failure!**".to_string();
        roll_context = "-".to_string();
        roll_color = data::EMBED_FAIL;
    } else if d20 >= check {
        total = fortune;
        roll_str = "Yippee, you passed.".to_string();
        roll_context = "+".to_string();
        roll_color = data::EMBED_SUCCESS;
    } else {
        total = fortune / 2;
        roll_str = "*oof*, you failed...".to_string();
        roll_context = "+".to_string();
        roll_color = data::EMBED_ERROR;
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
        "https://cdn.discordapp.com/attachments/1260223476766343188/1262189235558027274/pondering-my-orb-header-art.png?ex=6695b0d4&is=66945f54&hm=e704148f7bda31c186f2b9385ec81c0c5ab6c631cea0166d9a0bb677b84274a4&"
    } else if (50..75).contains(&random_meme) {
        ctx.data().ponder.choose(&mut thread_rng()).unwrap()
    } else {
        ctx.data().meme.choose(&mut thread_rng()).unwrap()
    };

    let reading = if d20 == 1 {
        ctx.data().bad_fortune.choose(&mut thread_rng()).unwrap().clone()
    } else {
        ctx.data().good_fortune.choose(&mut thread_rng()).unwrap().clone()
    };

    // Send rolling embed, sleep, then edit with result — lock not held during I/O
    let desc = format!("---\nYou needed a **{}** to pass...\n\n---\n---", check);
    let reply = ctx
        .send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description(&desc)
                    .thumbnail(base_ref.unwrap().to_string())
                    .color(data::EMBED_DEFAULT)
                    .image(ponder_image)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let desc = format!(
        "{} **{}{}** creds.\nYou needed a **{}** to pass, you rolled a **{}**.\n\n{}",
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

    // Acquire write lock only for the data mutation, after all Discord I/O is done
    let mut user_data = u.write().await;

    if d20 == 1 {
        user_data.sub_creds(total);
    } else {
        user_data.add_creds(total);
    }

    let levelup = user_data.update_xp(500);
    user_data.add_rolls(d20);
    user_data.push_roll(d20);
    user_data.add_bonus();
    user_data.update_daily();

    if levelup {
        let new_level = user_data.get_level();
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Level Up")
                    .description(format!(
                        "Wowzers, you powered up! <@{}> reached **Level {}**",
                        user.id,
                        new_level
                    ))
                    .thumbnail(user.avatar_url().unwrap_or_default())
                    .color(data::EMBED_LEVEL)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

        if new_level == data::GOLD_LEVEL_THRESHOLD {
            send_gold_unlock(ctx).await?;
        }
    }

    Ok(())
}

/// claim bonus creds for every three dailies
#[poise::command(slash_command)]
pub async fn claim_bonus(ctx: Context<'_>) -> Result<(), Error> {
    // update this to implement a d20 dice roll + bonus from level

    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get_mut(&user.id).unwrap();

    let (bonus, can_claim) = {
        let user_data = u.read().await;
        (user_data.get_bonus(), user_data.check_claim())
    };

    if can_claim {
        let d20: i32 = thread_rng().gen_range(1..21);
        let check: i32 = thread_rng().gen_range(6..15);
        let base_ref = ctx.data().d20f.get(28);

        // temporary message to roll the dice
        let desc = "Rolling for Bonus loot...\n---\n".to_string();
        let reply = ctx
            .send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Claim Bonus")
                        .description(&desc)
                        .thumbnail(base_ref.unwrap().to_string())
                        .color(data::EMBED_DEFAULT)
                        .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262193927386038302/giphy.gif?ex=6695b532&is=669463b2&hm=62e2fb0cc811b9e5b198a44c4351ca8f5d28bcc728c10334c55ba6b2f00ad658&")
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let fortune: i32 = if d20 == 20 {
            thread_rng().gen_range(35_000..120_000)
        } else if d20 == 1 {
            1
        } else {
            let low  = 3_000 + (check - 1) * 900;
            let high = 3_000 + check * 900;
            if d20 >= check { thread_rng().gen_range(low..high) } else { thread_rng().gen_range(low..high) / 2 }
        };

        let roll_ref = ctx.data().d20f.get((d20 - 1) as usize);
        let roll_color = if d20 == 20 { data::EMBED_GOLD } else if d20 == 1 { data::EMBED_ERROR } else if d20 >= check { data::EMBED_SUCCESS } else { data::EMBED_FAIL };

        let desc = format!(
            "You rolled a **{}** and obtained **+{}** creds.\nYou needed a **{}** to pass.",
            d20, fortune, check
        );

        reply
            .edit(
                ctx,
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Claim Bonus")
                        .description(&desc)
                        .thumbnail(roll_ref.unwrap().to_string())
                        .color(roll_color)
                        .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262191655323308053/19c237178769d1c1fe6cd44b3399afb61d2840b9_hq.gif?ex=6695b315&is=66946195&hm=43de96a5e0aac7f571a537420608f6a3b893831b5ccbc5bcdd3b74c9378bcaa8&")
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;

        // Acquire write lock only after all Discord I/O is done
        let mut user_data = u.write().await;
        user_data.add_creds(fortune);
        user_data.reset_bonus();

        let levelup = user_data.update_xp(150);
        if levelup {
            let new_level = user_data.get_level();
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Level Up")
                        .description(format!(
                            "Wowzers, you powered up! <@{}> reached **Level {}**",
                            user.id,
                            new_level
                        ))
                        .thumbnail(user.avatar_url().unwrap_or_default())
                        .color(data::EMBED_LEVEL)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;

            if new_level == data::GOLD_LEVEL_THRESHOLD {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("⭐ Gold Status Unlocked!")
                            .description(format!(
                                "Congratulations <@{}>! You've reached **Level 10** and unlocked **Gold Status**!\n\nYou now earn a higher HYSA rate on uninvested portfolio cash.",
                                user.id
                            ))
                            .thumbnail(user.avatar_url().unwrap_or_default())
                            .color(data::EMBED_GOLD)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await?;
            }
        }
    } else {
        let desc: String = match bonus {
            2 => {
                format!(
                    "The ***Bonus*** will be ready after your next `/uwu`! (Count: {}/3)",
                    bonus
                )
            }
            _ => {
                format!("The ***Bonus*** is not ready! (Count: {}/3)", bonus)
            }
        };

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Claim Bonus")
                    .description(desc)
                    .color(data::EMBED_ERROR)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    ))
                    .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262191656124284999/f7WPkmj.jpeg?ex=6695b315&is=66946195&hm=552171d50e562072461dc76c8222e9791cd9931f2ee7252975f25ab0dc63b0e5&"),
            ),
        )
        .await?;
    }
    Ok(())
}

/// check your creds, tickets, and submits
#[poise::command(slash_command)]
pub async fn wallet(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    // get user info
    let luck: String = if user_data.get_luck().is_empty() {
        "N/A".to_string()
    } else {
        user_data.get_luck()
    };

    let daily: String = if user_data.check_daily() {
        "Available".to_string()
    } else {
        "Not Available".to_string()
    };

    let claim: String = if user_data.check_claim() {
        "Available".to_string()
    } else {
        format!("{} / 3", user_data.get_bonus())
    };

    let level: i32 = user_data.get_level();
    let xp: i32 = user_data.get_xp();
    let next_level = user_data.get_next_level();
    let creds: i32 = user_data.get_creds();
    let tickets: i32 = user_data.get_tickets();
    let gold_badge = if level >= data::GOLD_LEVEL_THRESHOLD { "  ⭐ **Gold Status**" } else { "" };

    let desc = format!(
        "**Level {}**{}  -  {}/{}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\nDaily UwU........... . . . **{}**\nAverage Luck..... . . . **{}**\nClaim Bonus....... . . . **{}**\n\nTotal Creds: **{}** \u{3000}\u{3000}\u{2000}Tickets: **{}**\n",
        level, gold_badge, xp, next_level, daily, luck, claim, creds, tickets
    );

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wallet")
                .description(desc)
                .thumbnail(user.avatar_url().unwrap_or_default().to_string())
                .color(data::EMBED_GOLD)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

fn fmt_pnl_short(pnl: f64) -> String {
    let sign = if pnl >= 0.0 { "+" } else { "-" };
    let abs = pnl.abs();
    if abs >= 1_000_000.0 {
        format!("{}${:.2}m", sign, abs / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{}${:.2}k", sign, abs / 1_000.0)
    } else {
        format!("{}${:.2}", sign, abs)
    }
}

/// show server rankings — use buttons to switch between Creds, Fortune, and Investment
#[poise::command(slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let data = &ctx.data().users;

    type InfoVec = Vec<(UserId, i64, String, String)>;
    let mut creds_info: InfoVec = Vec::new();
    let mut fortune_info: InfoVec = Vec::new();
    let mut invest_info: InfoVec = Vec::new();

    for x in data.iter() {
        let (id, u) = x.pair();

        let (creds, luck_score, luck_label, invest_pnl, invest_cost) = {
            let u = u.read().await;
            let creds = u.get_creds();
            let luck_score = u.get_rolling_luck_score();
            let luck_label = u.get_rolling_luck();
            let mut total_pnl = 0.0f64;
            let mut total_cost = 0.0f64;
            for trade in &u.stock.trade_history {
                if let Some(pnl) = trade.realized_pnl {
                    total_pnl += pnl;
                    total_cost += trade.total_creds - pnl;
                }
            }
            (creds, luck_score, luck_label, total_pnl, total_cost)
        };

        let user_name = id.to_user(ctx).await?.name;

        creds_info.push((*id, creds as i64, creds.to_string(), user_name.clone()));

        if luck_score > 0 {
            fortune_info.push((*id, luck_score as i64, luck_label, user_name.clone()));
        }

        if invest_pnl != 0.0 {
            let pct = if invest_cost > 0.0 { invest_pnl / invest_cost * 100.0 } else { 0.0 };
            let label = format!("{} ({:+.1}%)", fmt_pnl_short(invest_pnl / 100.0), pct);
            invest_info.push((*id, invest_pnl as i64, label, user_name));
        }
    }

    creds_info.sort_by(|a, b| b.1.cmp(&a.1));
    fortune_info.sort_by(|a, b| b.1.cmp(&a.1));
    invest_info.sort_by(|a, b| b.1.cmp(&a.1));

    if creds_info.is_empty() {
        ctx.say("No users found.").await?;
        return Ok(());
    }

    fn build_page(info: &[(UserId, i64, String, String)], start: usize) -> String {
        let mut text = String::from("﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n```\n");
        for (index, (_id, value, label, user_name)) in
            info.iter().enumerate().skip(start).take(10)
        {
            let display = if label.is_empty() { value.to_string() } else { label.clone() };
            let name: String = user_name.chars().take(16).collect();
            text.push_str(&format!(
                "{:<4} {:^16} {:>18}\n",
                format!("#{}", index + 1),
                name,
                display
            ));
        }
        text.push_str("```");
        text
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Sort { Creds, Fortune, Invest }

    fn make_components(active: Sort) -> Vec<serenity::CreateActionRow> {
        use poise::serenity_prelude::ButtonStyle::{Primary, Secondary};
        let buttons = vec![
            serenity::CreateButton::new("lb_creds")
                .label("Creds")
                .style(if active == Sort::Creds { Primary } else { Secondary }),
            serenity::CreateButton::new("lb_fortune")
                .label("Fortune")
                .style(if active == Sort::Fortune { Primary } else { Secondary }),
            serenity::CreateButton::new("lb_invest")
                .label("Investment")
                .style(if active == Sort::Invest { Primary } else { Secondary }),
            serenity::CreateButton::new("lb_back").label("<").style(Secondary),
            serenity::CreateButton::new("lb_next").label(">").style(Secondary),
        ];
        vec![serenity::CreateActionRow::Buttons(buttons)]
    }

    let creds_thumb   = creds_info[0].0.to_user(ctx).await?.avatar_url().unwrap_or_default();
    let fortune_thumb = if let Some(e) = fortune_info.first() { e.0.to_user(ctx).await?.avatar_url().unwrap_or_else(|| creds_thumb.clone()) } else { creds_thumb.clone() };
    let invest_thumb  = if let Some(e) = invest_info.first()  { e.0.to_user(ctx).await?.avatar_url().unwrap_or_else(|| creds_thumb.clone()) } else { creds_thumb.clone() };

    fn make_embed(
        info: &[(UserId, i64, String, String)],
        sort: Sort,
        page: usize,
        thumbnail: &str,
    ) -> serenity::CreateEmbed {
        let total_pages = (info.len() + 9) / 10;
        let title = match sort {
            Sort::Creds   => "Leaderboard — Creds",
            Sort::Fortune => "Leaderboard — Rolling Fortune",
            Sort::Invest  => "Leaderboard — Investment Gains",
        };
        let text = build_page(info, page * 10);
        serenity::CreateEmbed::new()
            .title(title)
            .color(data::EMBED_CYAN)
            .thumbnail(thumbnail.to_string())
            .description("Here lists the most accomplished in UwUversity!")
            .field("Rankings", text, false)
            .field("Page", format!("{}/{}", page + 1, total_pages.max(1)), false)
            .footer(default_footer())
    }

    let initial_embed = make_embed(&creds_info, Sort::Creds, 0, &creds_thumb);
    let initial_components = make_components(Sort::Creds);

    let reply = ctx
        .send(poise::CreateReply::default().embed(initial_embed).components(initial_components))
        .await?;

    let msg_og = Arc::new(RwLock::new(reply.into_message().await?));
    let msg = Arc::clone(&msg_og);

    let mut interactions = msg
        .read()
        .await
        .await_component_interactions(ctx)
        .stream();

    let ctx = ctx.serenity_context().clone();

    tokio::spawn(async move {
        let mut current_sort = Sort::Creds;
        let mut current_page: usize = 0;

        while let Ok(Some(interaction)) = tokio::time::timeout(Duration::new(60, 0), interactions.next()).await {
            let active_info = match current_sort {
                Sort::Creds   => &creds_info,
                Sort::Fortune => &fortune_info,
                Sort::Invest  => &invest_info,
            };
            let total_pages = (active_info.len() + 9) / 10;

            match interaction.data.custom_id.as_str() {
                "lb_creds"   => { current_sort = Sort::Creds;   current_page = 0; }
                "lb_fortune" => { current_sort = Sort::Fortune; current_page = 0; }
                "lb_invest"  => { current_sort = Sort::Invest;  current_page = 0; }
                "lb_back"    => { if current_page > 0 { current_page -= 1; } }
                "lb_next"    => { if current_page < total_pages.saturating_sub(1) { current_page += 1; } }
                _ => (),
            }

            let active_info = match current_sort {
                Sort::Creds   => &creds_info,
                Sort::Fortune => &fortune_info,
                Sort::Invest  => &invest_info,
            };

            let empty_msg = match current_sort {
                Sort::Fortune => Some("No fortune data yet — users need to /uwu first."),
                Sort::Invest  => Some("No investment data yet — users need to make trades first."),
                Sort::Creds   => None,
            };

            let embed = if active_info.is_empty() {
                serenity::CreateEmbed::new()
                    .title(match current_sort {
                        Sort::Creds   => "Leaderboard — Creds",
                        Sort::Fortune => "Leaderboard — Rolling Fortune",
                        Sort::Invest  => "Leaderboard — Investment Gains",
                    })
                    .color(data::EMBED_ERROR)
                    .description(empty_msg.unwrap_or("No data."))
                    .footer(default_footer())
            } else {
                let thumb = match current_sort {
                    Sort::Creds   => &creds_thumb,
                    Sort::Fortune => &fortune_thumb,
                    Sort::Invest  => &invest_thumb,
                };
                make_embed(active_info, current_sort, current_page, thumb)
            };

            let components = make_components(current_sort);

            interaction
                .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                .await
                .unwrap();

            msg.write()
                .await
                .edit(&ctx, EditMessage::default().embed(embed).components(components))
                .await
                .unwrap();
        }

        // timeout — strip buttons and grey out embed
        let active_info = match current_sort {
            Sort::Creds   => &creds_info,
            Sort::Fortune => &fortune_info,
            Sort::Invest  => &invest_info,
        };
        let thumb = match current_sort {
            Sort::Creds   => &creds_thumb,
            Sort::Fortune => &fortune_thumb,
            Sort::Invest  => &invest_thumb,
        };
        let timed_out_embed = make_embed(active_info, current_sort, current_page, thumb)
            .color(data::EMBED_ERROR);
        msg.write()
            .await
            .edit(&ctx, EditMessage::default().embed(timed_out_embed).components(vec![]))
            .await
            .ok();
    });

    Ok(())
}

/// buy tickets for the battle pass raffle
#[poise::command(slash_command)]
pub async fn buy_tickets(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;

    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let tickets = user_data.get_tickets();
    let creds = user_data.get_creds();

    let tkcost1 = 2000 + 300 * (tickets);
    let tkcost2 = (2000 + 300 * (tickets + 1)) + tkcost1;
    let tkcost3 = (2000 + 300 * (tickets + 2)) + tkcost2;

    let mut tkcostmax = 0;
    let mut tkcount = 0;
    let mut tkcreds = creds;
    while 2000 + 300 * (tickets + tkcount) <= tkcreds {
        tkcreds -= 2000 + 300 * (tickets + tkcount);
        tkcostmax += 2000 + 300 * (tickets + tkcount);
        tkcount += 1;
    }


    let mut desc = format!(
        "Welcome to the Shop, buy tickets here to participate in the Server's Battle Pass Raffle! (Total: {})\n\n", 
        creds
    );

    let mut buttons = Vec::new();
    for i in 0..5 {
        if i == 0 {
            let button_none = serenity::CreateButton::new("open_modal")
                .label("None")
                .custom_id("buy-none".to_string())
                .style(poise::serenity_prelude::ButtonStyle::Secondary);
            buttons.push(button_none);
        } else if i == 4 {
            if tkcount > 0 {
                let button_max = serenity::CreateButton::new("open_modal")
                    .label("MAX")
                    .custom_id("buy-max".to_string())
                    .style(poise::serenity_prelude::ButtonStyle::Danger);

                buttons.push(button_max);
                desc +=
                    format!("\nBuy **MAX** ({} Tickets) . . . {}\n", tkcount, tkcostmax).as_str();
            }
        } else if i == 1 && tkcost1 <= creds
            || i == 2 && tkcost2 <= creds
            || i == 3 && tkcost3 <= creds
        {
            let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
            let button = serenity::CreateButton::new("open_modal")
                .label("")
                .custom_id(format!("buy-{}", i))
                .emoji(emoji)
                .style(poise::serenity_prelude::ButtonStyle::Primary);

            buttons.push(button);

            if i == 1 {
                desc += format!("Buy **{}** Ticket.............. . . . {}\n", i, tkcost1).as_str();
            } else if i == 2 {
                desc += format!("Buy **{}** Ticket.............. . . . {}\n", i, tkcost2).as_str();
            } else if i == 3 {
                desc += format!("Buy **{}** Ticket.............. . . . {}\n", i, tkcost3).as_str();
            }
        } else if i == 1 {
            desc +=
                format!("~~Buy **{}** Ticket~~.............. . . . {}\n", i, tkcost1).as_str();
        } else if i == 2 {
            desc +=
                format!("~~Buy **{}** Ticket~~.............. . . . {}\n", i, tkcost2).as_str();
        } else if i == 3 {
            desc +=
                format!("~~Buy **{}** Ticket~~.............. . . . {}\n", i, tkcost3).as_str();
        }
    }

    let components = vec![serenity::CreateActionRow::Buttons(buttons)];
    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(
                    serenity::CreateEmbed::default()
                        .title("Buy Tickets".to_string())
                        .description(&desc)
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
        .stream();

    let ctx = ctx.serenity_context().clone();

    let user_id = user.id;

    let u = Arc::clone(&u);

    tokio::spawn(async move {
        while let Ok(Some(reaction)) = tokio::time::timeout(Duration::new(60, 0), reactions.next()).await {
            let bought_tickets;
            let purchase_cost;

            let react_id = reaction.member.clone().unwrap_or_default().user.id;
            if react_id == user_id {
                match reaction.data.custom_id.as_str() {
                    "buy-1" => {
                        bought_tickets = 1;
                        purchase_cost = tkcost1;
                    }

                    "buy-2" => {
                        bought_tickets = 2;
                        purchase_cost = tkcost2;
                    }

                    "buy-3" => {
                        bought_tickets = 3;
                        purchase_cost = tkcost3;
                    }

                    "buy-max" => {
                        bought_tickets = tkcount;
                        purchase_cost = tkcostmax;
                    }

                    _ => {
                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title("Buy Tickets".to_string())
                                            .description(&desc)
                                            .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262203993245876244/tumblr_inline_pamkf7AfPf1s2a9fg_500.gif?ex=6695be92&is=66946d12&hm=49948cee0fd647192a40c9e88ad890cbbcb63724c460ee61964c99594c9c3a53&")
                                            .colour(data::EMBED_ERROR)
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
                }

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::new()
                                    .title("Buy Tickets".to_string())
                                    .description(format!(
                                        "You purchased **{}** ticket(s)! Ganbatte!! (-{} creds)",
                                        bought_tickets, purchase_cost
                                    ))
                                    .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262202607980777662/tumblr_n8dtwljTrx1tt5tk6o1_500.gif?ex=6695bd48&is=66946bc8&hm=da981bf028647549f958bb60e30c9c2f5d4635b6b597c50fb58f50b1618f7619&")
                                    .color(data::EMBED_CYAN)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();
                let mut user_data = u.write().await;

                user_data.sub_creds(purchase_cost);
                user_data.add_tickets(bought_tickets);

                return;
            }
        }

        msg.write()
            .await
            .edit(
                &ctx,
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::default()
                            .title("Buy Tickets".to_string())
                            .description(&desc)
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

#[poise::command(slash_command)]
pub async fn voice_status(ctx: Context<'_>) -> Result<(), Error> {
    let data = &ctx.data().voice_users;

    let mut out: Vec<(UserId, VoiceUser)> = Vec::new();

    for x in data.iter() {
        let (id, u) = x.pair();
        out.push((*id, u.clone()));
    }

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
            .color(data::EMBED_ERROR)
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
    let emojis = {
        let raw = guild
            .emojis
            .values()
            .map(|e| e.to_string())
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
                .colour(data::EMBED_DEFAULT)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

// ── Professor simulation helpers ─────────────────────────────────────────────
// These replicate the core logic of /uwu and /claim_bonus without Discord ctx.
// Used by the Professor AI background task.

/// Simulate a /uwu roll for Professor.
/// Returns creds awarded (negative on critical failure, 0 if cooldown not met).
pub fn simulate_uwu(user_data: &mut data::UserData) -> i32 {
    // [TEST] daily cooldown disabled
    // if !user_data.check_daily() { return 0; }
    let d20: i32 = thread_rng().gen_range(1..21);
    let check: i32 = thread_rng().gen_range(6..15);
    let low = 2_000 + (check - 1) * 600;
    let high = 2_000 + check * 600;
    let fortune: i32 = thread_rng().gen_range(low..high);

    let total: i32 = if d20 == 20 {
        thread_rng().gen_range(50_000..200_000)
    } else if d20 == 1 {
        -thread_rng().gen_range(15_000..40_000)
    } else if d20 >= check {
        fortune
    } else {
        fortune / 2
    };

    if total < 0 {
        user_data.sub_creds(-total);
    } else {
        user_data.add_creds(total);
    }
    user_data.update_xp(500);
    user_data.add_rolls(d20);
    user_data.push_roll(d20);
    user_data.add_bonus();
    user_data.update_daily();
    total
}

/// Simulate a /claim_bonus roll for Professor.
/// Returns creds awarded (0 if bonus_count < 3).
pub fn simulate_claim(user_data: &mut data::UserData) -> i32 {
    if !user_data.check_claim() {
        return 0;
    }
    let d20: i32 = thread_rng().gen_range(1..21);
    let check: i32 = thread_rng().gen_range(6..15);
    let fortune: i32 = if d20 == 20 {
        thread_rng().gen_range(35_000..120_000)
    } else if d20 == 1 {
        1
    } else {
        let low  = 3_000 + (check - 1) * 900;
        let high = 3_000 + check * 900;
        if d20 >= check { thread_rng().gen_range(low..high) } else { thread_rng().gen_range(low..high) / 2 }
    };
    user_data.add_creds(fortune);
    user_data.update_xp(150);
    user_data.reset_bonus();
    fortune
}
