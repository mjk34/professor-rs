//! Economy commands — /uwu, /claim_bonus, /buy_tickets, and Professor simulation helpers.

use crate::{data, serenity, Context, Error};
use crate::helper::default_footer;
use poise::serenity_prelude::{EditMessage, futures::StreamExt, ReactionType};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

async fn send_gold_unlock(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("⭐ Gold Status Unlocked!")
            .description(format!(
                "Congratulations <@{}>! You've reached **Level 10** and unlocked **Gold Status**!\n\nYou now earn a higher HYSA rate on uninvested portfolio cash.",
                user.id
            ))
            .thumbnail(user.avatar_url().unwrap_or_default())
            .color(data::EMBED_GOLD)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

/// claim your daily, 500xp (Once a day)
#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();

    let d20 = thread_rng().gen_range(1..21);
    let check = thread_rng().gen_range(6..15);

    let low = 2_000 + (check - 1) * 600;
    let high = 2_000 + check * 600;
    let fortune = thread_rng().gen_range(low..high);

    let (total, roll_str, roll_context, roll_color): (i32, String, String, serenity::Color) = if d20 == 20 {
        (thread_rng().gen_range(50_000..200_000), "**Critical Success!!**".to_string(), "+".to_string(), data::EMBED_GOLD)
    } else if d20 == 1 {
        (thread_rng().gen_range(15_000..40_000), "**Critical Failure!**".to_string(), "-".to_string(), data::EMBED_FAIL)
    } else if d20 >= check {
        (fortune, "Yippee, you passed.".to_string(), "+".to_string(), data::EMBED_SUCCESS)
    } else {
        (fortune / 2, "*oof*, you failed...".to_string(), "+".to_string(), data::EMBED_ERROR)
    };

    let base_ref = ctx.data().d20f.get(28).map_or("", std::string::String::as_str);
    let roll_ref = ctx.data().d20f.get((d20 - 1) as usize).map_or("", std::string::String::as_str);

    let random_meme = thread_rng().gen_range(0..100);
    let ponder_image = if random_meme < 50 {
        "https://cdn.discordapp.com/attachments/1260223476766343188/1262189235558027274/pondering-my-orb-header-art.png?ex=6695b0d4&is=66945f54&hm=e704148f7bda31c186f2b9385ec81c0c5ab6c631cea0166d9a0bb677b84274a4&"
    } else if (50..75).contains(&random_meme) {
        ctx.data().ponder.choose(&mut thread_rng()).map_or("", std::string::String::as_str)
    } else {
        ctx.data().meme.choose(&mut thread_rng()).map_or("", std::string::String::as_str)
    };

    let reading = if d20 == 1 {
        ctx.data().bad_fortune.choose(&mut thread_rng()).cloned().unwrap_or_default()
    } else {
        ctx.data().good_fortune.choose(&mut thread_rng()).cloned().unwrap_or_default()
    };

    let desc = format!("---\nYou needed a **{check}** to pass...\n\n---\n---");
    let reply = ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Daily")
            .description(&desc)
            .thumbnail(base_ref)
            .color(data::EMBED_DEFAULT)
            .image(ponder_image)
            .footer(default_footer()),
    )).await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let desc = format!(
        "{roll_str} **{roll_context}{total}** creds.\nYou needed a **{check}** to pass, you rolled a **{d20}**.\n\n{reading}",
    );

    reply.edit(ctx, poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Daily")
            .description(&desc)
            .thumbnail(roll_ref)
            .color(roll_color)
            .image(ponder_image)
            .footer(default_footer()),
    )).await?;

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
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Level Up")
                .description(format!("Wowzers, you powered up! <@{}> reached **Level {}**", user.id, new_level))
                .thumbnail(user.avatar_url().unwrap_or_default())
                .color(data::EMBED_LEVEL)
                .footer(default_footer()),
        )).await?;

        if new_level == data::GOLD_LEVEL_THRESHOLD {
            send_gold_unlock(ctx).await?;
        }
    }

    Ok(())
}

/// claim bonus creds for every three dailies
#[poise::command(slash_command)]
pub async fn claim_bonus(ctx: Context<'_>) -> Result<(), Error> {
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
        let base_ref = ctx.data().d20f.get(28).map_or("", std::string::String::as_str);

        let reply = ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Claim Bonus")
                .description("Rolling for Bonus loot...\n---\n")
                .thumbnail(base_ref)
                .color(data::EMBED_DEFAULT)
                .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262193927386038302/giphy.gif?ex=6695b532&is=669463b2&hm=62e2fb0cc811b9e5b198a44c4351ca8f5d28bcc728c10334c55ba6b2f00ad658&")
                .footer(default_footer()),
        )).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let fortune: i32 = if d20 == 20 {
            thread_rng().gen_range(35_000..120_000)
        } else if d20 == 1 {
            1
        } else {
            let low  = 3_000 + (check - 1) * 900;
            let high = 3_000 + check * 900;
            let v = thread_rng().gen_range(low..high);
            if d20 >= check { v } else { v / 2 }
        };

        let roll_ref = ctx.data().d20f.get((d20 - 1) as usize).map_or("", std::string::String::as_str);
        let roll_color = if d20 == 20 { data::EMBED_GOLD } else if d20 == 1 { data::EMBED_ERROR } else if d20 >= check { data::EMBED_SUCCESS } else { data::EMBED_FAIL };

        reply.edit(ctx, poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Claim Bonus")
                .description(format!("You rolled a **{d20}** and obtained **+{fortune}** creds.\nYou needed a **{check}** to pass."))
                .thumbnail(roll_ref)
                .color(roll_color)
                .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262191655323308053/19c237178769d1c1fe6cd44b3399afb61d2840b9_hq.gif?ex=6695b315&is=66946195&hm=43de96a5e0aac7f571a537420608f6a3b893831b5ccbc5bcdd3b74c9378bcaa8&")
                .footer(default_footer()),
        )).await?;

        let mut user_data = u.write().await;
        user_data.add_creds(fortune);
        user_data.reset_bonus();

        let levelup = user_data.update_xp(150);
        if levelup {
            let new_level = user_data.get_level();
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Level Up")
                    .description(format!("Wowzers, you powered up! <@{}> reached **Level {}**", user.id, new_level))
                    .thumbnail(user.avatar_url().unwrap_or_default())
                    .color(data::EMBED_LEVEL)
                    .footer(default_footer()),
            )).await?;

            if new_level == data::GOLD_LEVEL_THRESHOLD {
                send_gold_unlock(ctx).await?;
            }
        }
    } else {
        let desc: String = match bonus {
            2 => format!("The ***Bonus*** will be ready after your next `/uwu`! (Count: {bonus}/3)"),
            _ => format!("The ***Bonus*** is not ready! (Count: {bonus}/3)"),
        };

        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Claim Bonus")
                .description(desc)
                .color(data::EMBED_ERROR)
                .footer(default_footer())
                .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262191656124284999/f7WPkmj.jpeg?ex=6695b315&is=66946195&hm=552171d50e562072461dc76c8222e9791cd9931f2ee7252975f25ab0dc63b0e5&"),
        )).await?;
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

    let mut desc = format!(
        "Welcome to the Shop, buy tickets here to participate in the Server's Battle Pass Raffle! (Total: {creds})\n\n"
    );

    let mut buttons = Vec::new();
    for i in 0..5 {
        if i == 0 {
            buttons.push(
                serenity::CreateButton::new("open_modal")
                    .label("None")
                    .custom_id("buy-none".to_string())
                    .style(poise::serenity_prelude::ButtonStyle::Secondary),
            );
        } else if i == 4 {
            if tkcount > 0 {
                buttons.push(
                    serenity::CreateButton::new("open_modal")
                        .label("MAX")
                        .custom_id("buy-max".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Danger),
                );
                desc += format!("\nBuy **MAX** ({tkcount} Tickets) . . . {tkcostmax}\n").as_str();
            }
        } else if i == 1 && tkcost1 <= creds
            || i == 2 && tkcost2 <= creds
            || i == 3 && tkcost3 <= creds
        {
            let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
            buttons.push(
                serenity::CreateButton::new("open_modal")
                    .label("")
                    .custom_id(format!("buy-{i}"))
                    .emoji(emoji)
                    .style(poise::serenity_prelude::ButtonStyle::Primary),
            );
            if i == 1 {
                desc += format!("Buy **{i}** Ticket.............. . . . {tkcost1}\n").as_str();
            } else if i == 2 {
                desc += format!("Buy **{i}** Ticket.............. . . . {tkcost2}\n").as_str();
            } else if i == 3 {
                desc += format!("Buy **{i}** Ticket.............. . . . {tkcost3}\n").as_str();
            }
        } else if i == 1 {
            desc += format!("~~Buy **{i}** Ticket~~.............. . . . {tkcost1}\n").as_str();
        } else if i == 2 {
            desc += format!("~~Buy **{i}** Ticket~~.............. . . . {tkcost2}\n").as_str();
        } else if i == 3 {
            desc += format!("~~Buy **{i}** Ticket~~.............. . . . {tkcost3}\n").as_str();
        }
    }

    let components = vec![serenity::CreateActionRow::Buttons(buttons)];
    let reply = ctx.send(poise::CreateReply::default()
        .embed(serenity::CreateEmbed::default()
            .title("Buy Tickets".to_string())
            .description(&desc)
            .colour(data::EMBED_DEFAULT)
            .footer(default_footer()))
        .components(components)
    ).await?;

    let msg_og = Arc::new(RwLock::new(reply.into_message().await?));
    let msg = Arc::clone(&msg_og);
    let mut reactions = msg.read().await.await_component_interactions(ctx).stream();
    let ctx = ctx.serenity_context().clone();
    let user_id = user.id;
    let u = Arc::clone(&u);

    tokio::spawn(async move {
        while let Ok(Some(reaction)) = tokio::time::timeout(Duration::new(60, 0), reactions.next()).await {
            let react_id = reaction.member.clone().unwrap_or_default().user.id;
            if react_id != user_id {
                continue;
            }

            let (bought_tickets, purchase_cost) = match reaction.data.custom_id.as_str() {
                "buy-1"   => (1, tkcost1),
                "buy-2"   => (2, tkcost2),
                "buy-3"   => (3, tkcost3),
                "buy-max" => (tkcount, tkcostmax),
                _ => {
                    msg.write().await.edit(&ctx, EditMessage::default()
                        .embed(serenity::CreateEmbed::default()
                            .title("Buy Tickets".to_string())
                            .description(&desc)
                            .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262203993245876244/tumblr_inline_pamkf7AfPf1s2a9fg_500.gif?ex=6695be92&is=66946d12&hm=49948cee0fd647192a40c9e88ad890cbbcb63724c460ee61964c99594c9c3a53&")
                            .colour(data::EMBED_ERROR)
                            .footer(default_footer()))
                        .components(Vec::new())
                    ).await.unwrap();
                    return;
                }
            };

            msg.write().await.edit(&ctx, EditMessage::default()
                .embed(serenity::CreateEmbed::new()
                    .title("Buy Tickets".to_string())
                    .description(format!("You purchased **{bought_tickets}** ticket(s)! Ganbatte!! (-{purchase_cost} creds)"))
                    .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262202607980777662/tumblr_n8dtwljTrx1tt5tk6o1_500.gif?ex=6695bd48&is=66946bc8&hm=da981bf028647549f958bb60e30c9c2f5d4635b6b597c50fb58f50b1618f7619&")
                    .color(data::EMBED_CYAN)
                    .footer(default_footer()))
                .components(Vec::new())
            ).await.unwrap();

            let mut user_data = u.write().await;
            user_data.sub_creds(purchase_cost);
            user_data.add_tickets(bought_tickets);
            return;
        }

        msg.write().await.edit(&ctx, EditMessage::default()
            .embed(serenity::CreateEmbed::default()
                .title("Buy Tickets".to_string())
                .description(&desc)
                .colour(data::EMBED_ERROR)
                .footer(default_footer()))
            .components(Vec::new())
        ).await.unwrap();
    });

    Ok(())
}

// ── Professor simulation helpers ─────────────────────────────────────────────
// These replicate the core logic of /uwu and /claim_bonus without Discord ctx.
// Used by the Professor AI background task.

/// Simulate a /uwu roll for Professor.
/// Returns creds awarded (negative on critical failure, 0 if cooldown not met).
pub fn simulate_uwu(user_data: &mut data::UserData) -> i32 {
    if !user_data.check_daily() { return 0; }
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
    if !user_data.check_claim() { return 0; }
    let d20: i32 = thread_rng().gen_range(1..21);
    let check: i32 = thread_rng().gen_range(6..15);
    let fortune: i32 = if d20 == 20 {
        thread_rng().gen_range(35_000..120_000)
    } else if d20 == 1 {
        1
    } else {
        let low  = 3_000 + (check - 1) * 900;
        let high = 3_000 + check * 900;
        let v = thread_rng().gen_range(low..high);
        if d20 >= check { v } else { v / 2 }
    };
    user_data.add_creds(fortune);
    user_data.update_xp(150);
    user_data.reset_bonus();
    fortune
}
