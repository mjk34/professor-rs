//! Economy commands — /uwu, `/claim_bonus`, `/buy_tickets`.

use crate::{data, serenity, Context, Error};
use crate::helper::default_footer;
use poise::serenity_prelude::{EditMessage, ReactionType};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use std::sync::Arc;
use std::time::Duration;

async fn send_level_up(ctx: Context<'_>, user: &serenity::User, level: i32) -> Result<(), Error> {
    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Level Up")
            .description(format!("Wowzers, you powered up! <@{}> reached **Level {}**", user.id, level))
            .thumbnail(user.avatar_url().unwrap_or_default())
            .color(data::EMBED_LEVEL)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

/// claim your daily, 500xp (Once a day)
#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let u = Arc::clone(ctx.data().users.get(&user.id).unwrap().value());

    let d20 = thread_rng().gen_range(data::D20_RANGE);
    let check = thread_rng().gen_range(data::DC_CHECK_RANGE);

    let low = data::DAILY_FORTUNE_BASE + (check - 1) * data::DAILY_FORTUNE_STEP;
    let high = data::DAILY_FORTUNE_BASE + check * data::DAILY_FORTUNE_STEP;
    let fortune = thread_rng().gen_range(low..high);

    let (total, roll_str, roll_context, roll_color): (i32, String, String, serenity::Color) = if d20 == 20 {
        (thread_rng().gen_range(data::DAILY_CRIT_SUCCESS), "**Critical Success!!**".to_string(), "+".to_string(), data::EMBED_GOLD)
    } else if d20 == 1 {
        (thread_rng().gen_range(data::DAILY_CRIT_FAIL), "**Critical Failure!**".to_string(), "-".to_string(), data::EMBED_FAIL)
    } else if d20 >= check {
        (fortune, "Yippee, you passed.".to_string(), "+".to_string(), data::EMBED_SUCCESS)
    } else {
        (fortune / 2, "*oof*, you failed...".to_string(), "+".to_string(), data::EMBED_ERROR)
    };

    // Check the cooldown and apply the roll under one write lock, before any await,
    // so concurrent /uwu invocations can't both pass the check and pay out twice.
    let claimed = {
        let mut user_data = u.write().await;
        if user_data.check_daily() {
            if d20 == 1 {
                user_data.sub_creds(total);
            } else {
                user_data.add_creds(total);
            }
            let levelup = user_data.update_xp(data::DAILY_XP);
            user_data.add_rolls(d20);
            user_data.push_roll(d20);
            user_data.add_bonus();
            user_data.update_daily();
            Ok(levelup.then(|| user_data.get_level()))
        } else {
            Err(user_data.next_daily_timestamp())
        }
    };
    let new_level = match claimed {
        Ok(new_level) => new_level,
        Err(next_daily) => {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description(format!("You've already rolled today! Next roll available <t:{next_daily}:R>."))
                    .color(data::EMBED_ERROR)
                    .footer(default_footer()),
            )).await?;
            return Ok(());
        }
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

    if let Some(level) = new_level {
        send_level_up(ctx, user, level).await?;
    }

    Ok(())
}

/// claim bonus creds for every three dailies
#[poise::command(slash_command)]
pub async fn claim_bonus(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let u = Arc::clone(ctx.data().users.get(&user.id).unwrap().value());

    let d20: i32 = thread_rng().gen_range(data::D20_RANGE);
    let check: i32 = thread_rng().gen_range(data::DC_CHECK_RANGE);
    let fortune: i32 = if d20 == 20 {
        thread_rng().gen_range(data::BONUS_CRIT_SUCCESS)
    } else if d20 == 1 {
        1
    } else {
        let low  = data::BONUS_FORTUNE_BASE + (check - 1) * data::BONUS_FORTUNE_STEP;
        let high = data::BONUS_FORTUNE_BASE + check * data::BONUS_FORTUNE_STEP;
        let v = thread_rng().gen_range(low..high);
        if d20 >= check { v } else { v / 2 }
    };

    // Check eligibility and apply the payout under one write lock, before any await,
    // so concurrent invocations can't both claim the same bonus.
    let claimed = {
        let mut user_data = u.write().await;
        if user_data.check_claim() {
            user_data.add_creds(fortune);
            user_data.reset_bonus();
            let levelup = user_data.update_xp(data::BONUS_XP);
            Ok(levelup.then(|| user_data.get_level()))
        } else {
            Err(user_data.get_bonus())
        }
    };

    if let Ok(new_level) = claimed {
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

        if let Some(level) = new_level {
            send_level_up(ctx, user, level).await?;
        }
    } else if let Err(bonus) = claimed {
        let desc: String = match bonus {
            n if n == data::BONUS_CLAIM_THRESHOLD - 1 => format!("The ***Bonus*** will be ready after your next `/uwu`! (Count: {bonus}/{})", data::BONUS_CLAIM_THRESHOLD),
            _ => format!("The ***Bonus*** is not ready! (Count: {bonus}/{})", data::BONUS_CLAIM_THRESHOLD),
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
    let u = Arc::clone(ctx.data().users.get(&user.id).unwrap().value());
    let serenity_ctx = ctx.serenity_context().clone();

    let (tickets, creds) = {
        let ud = u.read().await;
        (ud.get_tickets(), ud.get_creds())
    };

    let ticket_cost = |n: i32| data::TICKET_BASE_COST + data::TICKET_COST_STEP * n;

    let tkcost1 = ticket_cost(tickets);
    let tkcost2 = ticket_cost(tickets + 1) + tkcost1;
    let tkcost3 = ticket_cost(tickets + 2) + tkcost2;

    let mut tkcostmax = 0;
    let mut tkcount = 0;
    let mut tkcreds = creds;
    while ticket_cost(tickets + tkcount) <= tkcreds && tkcount < data::MAX_TICKET_PURCHASE {
        let c = ticket_cost(tickets + tkcount);
        tkcreds -= c;
        tkcostmax += c;
        tkcount += 1;
    }

    let mut desc = format!(
        "Welcome to the Shop, buy tickets here to participate in the Server's Battle Pass Raffle! (Total: {creds})\n\n"
    );

    let mut buttons = vec![
        serenity::CreateButton::new("buy-none")
            .label("None")
            .style(serenity::ButtonStyle::Secondary),
    ];

    for (i, cost) in [(1, tkcost1), (2, tkcost2), (3, tkcost3)] {
        if cost <= creds {
            let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
            buttons.push(
                serenity::CreateButton::new(format!("buy-{i}"))
                    .label("")
                    .emoji(emoji)
                    .style(serenity::ButtonStyle::Primary),
            );
            desc += &format!("Buy **{i}** Ticket.............. . . . {cost}\n");
        } else {
            desc += &format!("~~Buy **{i}** Ticket~~.............. . . . {cost}\n");
        }
    }

    if tkcount > 0 {
        buttons.push(
            serenity::CreateButton::new("buy-max")
                .label("MAX")
                .style(serenity::ButtonStyle::Danger),
        );
        desc += &format!("\nBuy **MAX** ({tkcount} Tickets) . . . {tkcostmax}\n");
    }

    let reply = ctx.send(poise::CreateReply::default()
        .embed(serenity::CreateEmbed::default()
            .title("Buy Tickets")
            .description(&desc)
            .colour(data::EMBED_DEFAULT)
            .footer(default_footer()))
        .components(vec![serenity::CreateActionRow::Buttons(buttons)])
    ).await?;

    let mut msg = reply.into_message().await?;

    loop {
        let Some(press) = msg
            .await_component_interaction(&serenity_ctx)
            .author_id(user.id)
            .timeout(Duration::from_secs(60))
            .await
        else {
            msg.edit(&serenity_ctx, EditMessage::default()
                .embed(serenity::CreateEmbed::default()
                    .title("Buy Tickets")
                    .description(&desc)
                    .colour(data::EMBED_ERROR)
                    .footer(default_footer()))
                .components(vec![]))
                .await.ok();
            break;
        };

        press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

        let (bought, cost) = match press.data.custom_id.as_str() {
            "buy-1"    => (1, tkcost1),
            "buy-2"    => (2, tkcost2),
            "buy-3"    => (3, tkcost3),
            "buy-max"  => (tkcount, tkcostmax),
            "buy-none" => {
                msg.edit(&serenity_ctx, EditMessage::default()
                    .embed(serenity::CreateEmbed::default()
                        .title("Buy Tickets")
                        .description(&desc)
                        .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262203993245876244/tumblr_inline_pamkf7AfPf1s2a9fg_500.gif?ex=6695be92&is=66946d12&hm=49948cee0fd647192a40c9e88ad890cbbcb63724c460ee61964c99594c9c3a53&")
                        .colour(data::EMBED_ERROR)
                        .footer(default_footer()))
                    .components(vec![]))
                    .await.ok();
                break;
            }
            _ => continue,
        };

        // Prices were quoted from a snapshot up to 60s old — re-validate under the write lock
        // so a concurrent spend can't overdraw creds or buy at a stale ticket price.
        let charged = {
            let mut ud = u.write().await;
            let fresh = ud.get_tickets() == tickets && ud.get_creds() >= cost;
            if fresh {
                ud.sub_creds(cost);
                ud.add_tickets(bought);
            }
            fresh
        };

        if !charged {
            msg.edit(&serenity_ctx, EditMessage::default()
                .embed(serenity::CreateEmbed::new()
                    .title("Buy Tickets")
                    .description("Your creds or tickets changed since this shop opened — run `/buy_tickets` again.")
                    .colour(data::EMBED_ERROR)
                    .footer(default_footer()))
                .components(vec![]))
                .await.ok();
            break;
        }

        msg.edit(&serenity_ctx, EditMessage::default()
            .embed(serenity::CreateEmbed::new()
                .title("Buy Tickets")
                .description(format!("You purchased **{bought}** ticket(s)! Ganbatte!! (-{cost} creds)"))
                .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262202607980777662/tumblr_n8dtwljTrx1tt5tk6o1_500.gif?ex=6695bd48&is=66946bc8&hm=da981bf028647549f958bb60e30c9c2f5d4635b6b597c50fb58f50b1618f7619&")
                .color(data::EMBED_CYAN)
                .footer(default_footer()))
            .components(vec![]))
            .await.ok();
        break;
    }

    Ok(())
}
