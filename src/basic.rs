//!---------------------------------------------------------------------!
//! This file contains a collection of commands that is fundamental     !
//! to professorBot's functionality and purpose                         !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - ping                                                      !
//!     [x] - uwu                                                       !
//!     [x] - claim_bonus                                               !
//!     [-] - wallet                                                    !
//!     [-] - leaderboard                                               !
//!     [x] - buy_tickets                                               !
//!     [x] - voice_status                                              !
//!     [x] - info                                                      !
//!---------------------------------------------------------------------!

use crate::data::{self, VoiceUser};
use crate::gpt::gpt_string;
use crate::reminder;
use crate::{serenity, Context, Error};
use chrono::prelude::Utc;
use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::{EditMessage, ReactionType, UserId};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// ping the bot to see if its alive or to play ping pong
#[poise::command(slash_command)]
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

/// claim your daily, 500xp, and 2 wishes (Once a day)
#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let mut user_data = u.write().await;

    // check if daily is available
    // if !user_data.check_daily() {
    //     ctx.send(
    //         poise::CreateReply::default().embed(
    //             serenity::CreateEmbed::new()
    //                 .title("Daily")
    //                 .description("Your next **/uwu** is tomorrow")
    //                 .color(EMBED_ERROR)
    //                 .thumbnail(user.avatar_url().unwrap_or_default()),
    //         ),
    //     )
    //     .await?;
    //     return Ok(());
    // }

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
        roll_color = data::EMBED_GOLD;
    } else if d20 == 1 {
        total = fortune;
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

    // temporary message to roll the dice
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

    // generate fortune readings with gpt3.5
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    let prompt = if d20 == 1 {
        "give me a bad fortune that's funny, only the fortune, no quotes, like a fortune cookie, less than 20 words"
    } else {
        "give me a good fortune that's funny, only the fortune, no quotes, like a fortune cookie, less than 20 words"
    };

    let mut tries = 0;
    let reading;
    let gpt_key: String = env::var("API_KEY").expect("missing GPT API_KEY");
    loop {
        match gpt_string(gpt_key.clone(), prompt.to_string()).await {
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

    reminder::check_birthday(ctx).await;

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
                        .color(data::EMBED_DEFAULT)
                        .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262193927386038302/giphy.gif?ex=6695b532&is=669463b2&hm=62e2fb0cc811b9e5b198a44c4351ca8f5d28bcc728c10334c55ba6b2f00ad658&")
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
                        .color(data::EMBED_GOLD)
                        .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262191655323308053/19c237178769d1c1fe6cd44b3399afb61d2840b9_hq.gif?ex=6695b315&is=66946195&hm=43de96a5e0aac7f571a537420608f6a3b893831b5ccbc5bcdd3b74c9378bcaa8&")
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

/// check how many creds, wishes, or submits you have
#[poise::command(slash_command)]
pub async fn wallet(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    // get user info
    let luck: String = if user_data.get_luck() == "" {
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

    let desc = format!(
        "**Level {} **  -  {}/{}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\nDaily UwU........... . . . **{}**\nAverage Luck..... . . . **{}**\nClaim Bonus....... . . . **{}**\n\nTotal Creds: **{}** \u{3000}\u{3000}\u{2000}Tickets: **{}**\n",
        level, xp, next_level, daily, luck, claim, creds, tickets
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

//TODO create buttons for various sorts, creds/pokedex/tickets
/// show the top wealthiest users in the server
#[poise::command(slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let data = &ctx.data().users;

    let mut all_creds = Vec::new();
    let mut all_fortune = Vec::new();
    let mut all_level = vec::new();

    for x in data.iter() {
        let (id, u) = x.pair();
        let u = u.read().await;

        let user_name = id.to_user(ctx).await?.name;
        info.push((*id, u.get_creds(), String::new(), user_name));
    }

    info.sort_by(|a, b| b.1.cmp(&a.1));
    let total_pages = (&info.len()) / 10 + 1;

    let buttons = vec![
        serenity::CreateButton::new("open_modal")
            .label("<")
            .custom_id("back".to_string())
            .style(poise::serenity_prelude::ButtonStyle::Secondary),
        serenity::CreateButton::new("open_modal")
            .label(">")
            .custom_id("next".to_string())
            .style(poise::serenity_prelude::ButtonStyle::Secondary),
    ];
    let components = vec![serenity::CreateActionRow::Buttons(buttons)];

    let first_thumbnail = info[0]
        .0
        .to_user(ctx)
        .await?
        .avatar_url()
        .unwrap_or_default();

    let leaderboard_text = get_learderboard(&info, &display, &fortune, &level, 0);

    let embed = serenity::CreateEmbed::new()
        .title("Leaderboard")
        .color(data::EMBED_CYAN)
        .thumbnail(first_thumbnail.clone())
        .description("Here lists the most accomplished in UwUversity!")
        .field("Rankings", leaderboard_text, false)
        .footer(serenity::CreateEmbedFooter::new(
            "@~ powered by UwUntu & RustyBamboo",
        ));

    if total_pages > 1 {
        let reply = ctx
            .send(
                poise::CreateReply::default()
                    .embed(embed)
                    .components(components),
            )
            .await?;

        let msg_og = Arc::new(RwLock::new(reply.into_message().await?));

        let msg = Arc::clone(&msg_og);

        let mut reactions = msg
            .read()
            .await
            .await_component_interactions(ctx)
            .timeout(Duration::new(60, 0))
            .stream();

        let ctx = ctx.serenity_context().clone();

        let info = info.clone();
        let display = display.clone();
        let fortune = fortune.clone();
        let level = level.clone();
        tokio::spawn(async move {
            let mut current_page: usize = 0;
            while let Some(reaction) = reactions.next().await {
                let label = reaction.data.custom_id.as_str();
                match label {
                    "back" => {
                        if current_page > 0 {
                            current_page -= 10;
                        }
                    }
                    "next" => {
                        if current_page < total_pages - 1 {
                            current_page += 10;
                        }
                    }
                    _ => (),
                };

                let leaderboard_text =
                    get_learderboard(&info, &display, &fortune, &level, current_page);

                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();

                let embed = serenity::CreateEmbed::new()
                    .title("Leaderboard")
                    .color(data::EMBED_CYAN)
                    .thumbnail(first_thumbnail.clone())
                    .description("Here lists the most accomplished in UwUversity!")
                    .field("Rankings", leaderboard_text, false)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    ));
                msg.write()
                    .await
                    .edit(&ctx, EditMessage::default().embed(embed))
                    .await
                    .unwrap();
            }
        });
    } else {
        ctx.send(
            poise::CreateReply::default()
                .embed(embed)
                .components(components),
        )
        .await?;
    }

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

    println!("{}", tkcount);

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
        } else {
            if i == 1 {
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
        .timeout(Duration::new(60, 0))
        .stream();

    let ctx = ctx.serenity_context().clone();

    let user_id = user.id;

    let u = Arc::clone(&u);

    tokio::spawn(async move {
        while let Some(reaction) = reactions.next().await {
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

/// get the status of voice channels
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

/// get info about the server
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

    let mut desc = format!(
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
    );
    desc.truncate(4096);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title(&guild.name)
                .thumbnail(&icon_url)
                .image(&banner_url)
                .description(desc)
                .colour(data::EMBED_DEFAULT)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}
