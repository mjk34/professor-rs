//! Economy commands — /uwu, `/claim_bonus`, `/buy_tickets`.

use crate::{data, serenity, Context, Error};
use crate::helper::default_footer;
use poise::serenity_prelude::{EditMessage, ReactionType};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use std::sync::Arc;
use std::time::Duration;

/// Natural d20 results with special outcomes.
const NAT_20: i32 = 20;
const NAT_1: i32 = 1;
/// Row of `reference/d20.txt` holding the un-rolled die image (rows 0..20 are the faces).
const D20_BASE_IMAGE_IDX: usize = 28;
/// Pause between the "rolling…" embed and the result reveal.
const ROLL_REVEAL_DELAY: Duration = Duration::from_secs(2);
/// A crit-fail bonus claim pays a token amount.
const BONUS_CRIT_FAIL_CREDS: i32 = 1;
/// How long the ticket shop waits for a button press.
const SHOP_TIMEOUT: Duration = Duration::from_secs(60);
/// Ticket shop button ids. Tier buttons are `buy-1`..`buy-3`.
const BUY_TIER_PREFIX: &str = "buy-";
const BUY_MAX_ID: &str = "buy-max";
const BUY_NONE_ID: &str = "buy-none";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RollOutcome { CritSuccess, CritFail, Pass, Fail }

impl RollOutcome {
    const fn from_roll(d20: i32, check: i32) -> Self {
        if d20 == NAT_20 { Self::CritSuccess }
        else if d20 == NAT_1 { Self::CritFail }
        else if d20 >= check { Self::Pass }
        else { Self::Fail }
    }

    /// Red for a crit fail, grey for an ordinary fail — shared by `/uwu` and `/claim_bonus`.
    const fn color(self) -> serenity::Color {
        match self {
            Self::CritSuccess => data::EMBED_GOLD,
            Self::CritFail => data::EMBED_FAIL,
            Self::Pass => data::EMBED_SUCCESS,
            Self::Fail => data::EMBED_ERROR,
        }
    }

    const fn daily_label(self) -> &'static str {
        match self {
            Self::CritSuccess => "**Critical Success!!**",
            Self::CritFail => "**Critical Failure!**",
            Self::Pass => "Yippee, you passed.",
            Self::Fail => "*oof*, you failed...",
        }
    }
}

/// Creds range for a fortune at DC `check`: one `step`-wide band above `base`.
const fn fortune_range(base: i32, step: i32, check: i32) -> std::ops::Range<i32> {
    (base + (check - 1) * step)..(base + check * step)
}

/// Image URL from `reference/d20.txt`, or empty if the file is short.
fn d20_image(d20f: &[String], idx: usize) -> &str {
    d20f.get(idx).map_or("", String::as_str)
}

/// Image for a rolled face (1..=20).
fn d20_face(d20f: &[String], d20: i32) -> &str {
    usize::try_from(d20 - 1).map_or("", |idx| d20_image(d20f, idx))
}

/// Price of the next ticket for someone who already owns `owned`.
const fn ticket_cost(owned: i32) -> i32 {
    data::TICKET_BASE_COST.saturating_add(data::TICKET_COST_STEP.saturating_mul(owned))
}

/// Ticket shop prices for a user's current tickets and creds.
#[derive(Debug, PartialEq, Eq)]
struct Quote {
    /// Total cost of buying 1, 2 and 3 tickets.
    tiers: [i32; 3],
    /// Most tickets affordable (capped at `MAX_TICKET_PURCHASE`) and their total cost.
    max_count: i32,
    max_cost: i32,
}

fn quote(tickets: i32, creds: i32) -> Quote {
    let mut tiers = [0; 3];
    let mut total = 0_i32;
    for (n, tier) in (0..).zip(tiers.iter_mut()) {
        total = total.saturating_add(ticket_cost(tickets.saturating_add(n)));
        *tier = total;
    }
    let (mut max_count, mut max_cost) = (0, 0_i32);
    while max_count < data::MAX_TICKET_PURCHASE {
        let next = ticket_cost(tickets.saturating_add(max_count));
        if max_cost.saturating_add(next) > creds {
            break;
        }
        max_cost += next;
        max_count += 1;
    }
    Quote { tiers, max_count, max_cost }
}

/// Charges for `bought` tickets at a price quoted when the user owned `quoted_tickets`.
/// Refuses if their tickets changed since (stale price) or they can no longer afford it.
const fn try_purchase(user: &mut data::UserData, quoted_tickets: i32, cost: i32, bought: i32) -> bool {
    if user.get_tickets() != quoted_tickets || user.get_creds() < cost {
        return false;
    }
    user.sub_creds(cost);
    user.add_tickets(bought);
    true
}

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

    let outcome = RollOutcome::from_roll(d20, check);
    let fortune = thread_rng().gen_range(fortune_range(data::DAILY_FORTUNE_BASE, data::DAILY_FORTUNE_STEP, check));
    let total = match outcome {
        RollOutcome::CritSuccess => thread_rng().gen_range(data::DAILY_CRIT_SUCCESS),
        RollOutcome::CritFail => thread_rng().gen_range(data::DAILY_CRIT_FAIL),
        RollOutcome::Pass => fortune,
        RollOutcome::Fail => fortune / 2,
    };
    let sign = if outcome == RollOutcome::CritFail { "-" } else { "+" };

    // Check the cooldown and apply the roll under one write lock, before any await,
    // so concurrent /uwu invocations can't both pass the check and pay out twice.
    let claimed = {
        let mut user_data = u.write().await;
        if user_data.check_daily() {
            if outcome == RollOutcome::CritFail {
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

    let base_ref = d20_image(&ctx.data().d20f, D20_BASE_IMAGE_IDX);
    let roll_ref = d20_face(&ctx.data().d20f, d20);

    let random_meme = thread_rng().gen_range(0..100);
    let ponder_image = if random_meme < 50 {
        "https://cdn.discordapp.com/attachments/1260223476766343188/1262189235558027274/pondering-my-orb-header-art.png?ex=6695b0d4&is=66945f54&hm=e704148f7bda31c186f2b9385ec81c0c5ab6c631cea0166d9a0bb677b84274a4&"
    } else if (50..75).contains(&random_meme) {
        ctx.data().ponder.choose(&mut thread_rng()).map_or("", std::string::String::as_str)
    } else {
        ctx.data().meme.choose(&mut thread_rng()).map_or("", std::string::String::as_str)
    };

    let reading = if outcome == RollOutcome::CritFail {
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

    tokio::time::sleep(ROLL_REVEAL_DELAY).await;

    let desc = format!(
        "{} **{sign}{total}** creds.\nYou needed a **{check}** to pass, you rolled a **{d20}**.\n\n{reading}",
        outcome.daily_label(),
    );

    reply.edit(ctx, poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Daily")
            .description(&desc)
            .thumbnail(roll_ref)
            .color(outcome.color())
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
    let outcome = RollOutcome::from_roll(d20, check);
    let fortune: i32 = match outcome {
        RollOutcome::CritSuccess => thread_rng().gen_range(data::BONUS_CRIT_SUCCESS),
        RollOutcome::CritFail => BONUS_CRIT_FAIL_CREDS,
        RollOutcome::Pass | RollOutcome::Fail => {
            let v = thread_rng().gen_range(fortune_range(data::BONUS_FORTUNE_BASE, data::BONUS_FORTUNE_STEP, check));
            if outcome == RollOutcome::Pass { v } else { v / 2 }
        }
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

    let new_level = match claimed {
        Ok(new_level) => new_level,
        Err(bonus) => {
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
            return Ok(());
        }
    };

    {
        let base_ref = d20_image(&ctx.data().d20f, D20_BASE_IMAGE_IDX);

        let reply = ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Claim Bonus")
                .description("Rolling for Bonus loot...\n---\n")
                .thumbnail(base_ref)
                .color(data::EMBED_DEFAULT)
                .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262193927386038302/giphy.gif?ex=6695b532&is=669463b2&hm=62e2fb0cc811b9e5b198a44c4351ca8f5d28bcc728c10334c55ba6b2f00ad658&")
                .footer(default_footer()),
        )).await?;

        tokio::time::sleep(ROLL_REVEAL_DELAY).await;

        let roll_ref = d20_face(&ctx.data().d20f, d20);

        reply.edit(ctx, poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Claim Bonus")
                .description(format!("You rolled a **{d20}** and obtained **+{fortune}** creds.\nYou needed a **{check}** to pass."))
                .thumbnail(roll_ref)
                .color(outcome.color())
                .image("https://cdn.discordapp.com/attachments/1260223476766343188/1262191655323308053/19c237178769d1c1fe6cd44b3399afb61d2840b9_hq.gif?ex=6695b315&is=66946195&hm=43de96a5e0aac7f571a537420608f6a3b893831b5ccbc5bcdd3b74c9378bcaa8&")
                .footer(default_footer()),
        )).await?;

        if let Some(level) = new_level {
            send_level_up(ctx, user, level).await?;
        }
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

    let q = quote(tickets, creds);

    let mut desc = format!(
        "Welcome to the Shop, buy tickets here to participate in the Server's Battle Pass Raffle! (Total: {creds})\n\n"
    );

    let mut buttons = vec![
        serenity::CreateButton::new(BUY_NONE_ID)
            .label("None")
            .style(serenity::ButtonStyle::Secondary),
    ];

    for (i, cost) in (1_usize..).zip(q.tiers) {
        if cost <= creds {
            let emoji = ReactionType::Unicode(data::NUMBER_EMOJS[i].to_string());
            buttons.push(
                serenity::CreateButton::new(format!("{BUY_TIER_PREFIX}{i}"))
                    .label("")
                    .emoji(emoji)
                    .style(serenity::ButtonStyle::Primary),
            );
            desc += &format!("Buy **{i}** Ticket.............. . . . {cost}\n");
        } else {
            desc += &format!("~~Buy **{i}** Ticket~~.............. . . . {cost}\n");
        }
    }

    if q.max_count > 0 {
        buttons.push(
            serenity::CreateButton::new(BUY_MAX_ID)
                .label("MAX")
                .style(serenity::ButtonStyle::Danger),
        );
        desc += &format!("\nBuy **MAX** ({} Tickets) . . . {}\n", q.max_count, q.max_cost);
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
            .timeout(SHOP_TIMEOUT)
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
            BUY_MAX_ID => (q.max_count, q.max_cost),
            BUY_NONE_ID => {
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
            id => {
                let tier = id.strip_prefix(BUY_TIER_PREFIX).and_then(|n| n.parse::<usize>().ok());
                match tier.and_then(|n| Some((n, *q.tiers.get(n.checked_sub(1)?)?))) {
                    Some((n, cost)) => (i32::try_from(n)?, cost),
                    None => continue,
                }
            }
        };

        // Prices were quoted from a snapshot up to 60s old — re-validate under the write lock
        // so a concurrent spend can't overdraw creds or buy at a stale ticket price.
        let charged = try_purchase(&mut *u.write().await, tickets, cost, bought);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_outcomes() {
        assert_eq!(RollOutcome::from_roll(NAT_20, 15), RollOutcome::CritSuccess);
        assert_eq!(RollOutcome::from_roll(NAT_1, 2), RollOutcome::CritFail);
        assert_eq!(RollOutcome::from_roll(10, 10), RollOutcome::Pass);
        assert_eq!(RollOutcome::from_roll(9, 10), RollOutcome::Fail);
    }

    #[test]
    fn fortune_ranges_are_never_empty() {
        for check in data::DC_CHECK_RANGE {
            assert!(!fortune_range(data::DAILY_FORTUNE_BASE, data::DAILY_FORTUNE_STEP, check).is_empty());
            assert!(!fortune_range(data::BONUS_FORTUNE_BASE, data::BONUS_FORTUNE_STEP, check).is_empty());
        }
        assert!(!data::DAILY_CRIT_SUCCESS.is_empty() && !data::DAILY_CRIT_FAIL.is_empty());
        assert!(!data::BONUS_CRIT_SUCCESS.is_empty());
    }

    #[test]
    fn d20_face_maps_rolls_to_rows() {
        let rows: Vec<String> = (0..29).map(|i| i.to_string()).collect();
        assert_eq!(d20_face(&rows, 1), "0");
        assert_eq!(d20_face(&rows, 20), "19");
        assert_eq!(d20_face(&rows, 0), "");
        assert_eq!(d20_image(&rows, D20_BASE_IMAGE_IDX), "28");
        assert_eq!(d20_image(&[], D20_BASE_IMAGE_IDX), "");
    }

    #[test]
    fn quote_tiers_and_max() {
        let c0 = data::TICKET_BASE_COST;
        let c1 = c0 + data::TICKET_COST_STEP;
        let c2 = c1 + data::TICKET_COST_STEP;
        let q = quote(0, c0 + c1);
        assert_eq!(q.tiers, [c0, c0 + c1, c0 + c1 + c2]);
        assert_eq!((q.max_count, q.max_cost), (2, c0 + c1));
        assert_eq!(quote(0, c0 - 1).max_count, 0);
        // Capped at MAX_TICKET_PURCHASE however rich.
        let rich = quote(0, i32::MAX);
        assert_eq!(rich.max_count, data::MAX_TICKET_PURCHASE);
        // Huge ticket counts saturate instead of overflowing.
        assert_eq!(quote(i32::MAX, 0).tiers[2], i32::MAX);
    }

    #[test]
    fn try_purchase_rejects_stale_or_unaffordable() {
        let mut u = data::UserData::default();
        u.add_creds(5_000);
        assert!(!try_purchase(&mut u, 1, 2_000, 1)); // ticket count changed since quote
        assert!(!try_purchase(&mut u, 0, 5_001, 1)); // can no longer afford
        assert!(try_purchase(&mut u, 0, 5_000, 2)); // exact balance
        assert_eq!((u.get_creds(), u.get_tickets()), (0, 2));
    }
}
