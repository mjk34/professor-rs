//! /leaderboard command — creds, fortune, and investment rankings with pagination.

use crate::{data, serenity, Context, Error};
use crate::helper::default_footer;
use poise::serenity_prelude::{EditMessage, futures, UserId};
use std::sync::Arc;
use std::time::Duration;

pub fn fmt_pnl_short(pnl: f64) -> String {
    let sign = if pnl >= 0.0 { "+" } else { "-" };
    let abs = pnl.abs();
    if abs >= 1_000_000.0 {
        format!("{}${:.2}m", sign, abs / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{}${:.2}k", sign, abs / 1_000.0)
    } else {
        format!("{sign}${abs:.2}")
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sort { Creds, Fortune, Invest }

/// (`user_id`, `sort_key`, `display_label`, `username`)
type Entry = (UserId, i64, String, String);

fn build_page(entries: &[Entry], page: usize) -> String {
    let start = page * 10;
    let mut text = String::from("﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n```\n");
    for (rank, (_, _, label, name)) in entries.iter().enumerate().skip(start).take(10) {
        let name_col: String = name.chars().take(16).collect();
        text.push_str(&format!("{:<4} {:^16} {:>18}\n", format!("#{}", rank + 1), name_col, label));
    }
    text.push_str("```");
    text
}

fn make_components(active: Sort) -> Vec<serenity::CreateActionRow> {
    use poise::serenity_prelude::ButtonStyle::{Primary, Secondary};
    vec![serenity::CreateActionRow::Buttons(vec![
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
    ])]
}

fn make_embed(entries: &[Entry], sort: Sort, page: usize, thumbnail: &str) -> serenity::CreateEmbed {
    let total_pages = entries.len().div_ceil(10).max(1);
    let title = match sort {
        Sort::Creds   => "Leaderboard — Creds",
        Sort::Fortune => "Leaderboard — Rolling Fortune",
        Sort::Invest  => "Leaderboard — Investment Gains",
    };
    serenity::CreateEmbed::new()
        .title(title)
        .color(data::EMBED_CYAN)
        .thumbnail(thumbnail.to_string())
        .description("Here lists the most accomplished in UwUversity!")
        .field("Rankings", build_page(entries, page), false)
        .field("Page", format!("{}/{}", page + 1, total_pages), false)
        .footer(default_footer())
}

fn empty_embed(sort: Sort) -> serenity::CreateEmbed {
    let (title, desc) = match sort {
        Sort::Fortune => ("Leaderboard — Rolling Fortune", "No fortune data yet — users need to /uwu first."),
        Sort::Invest  => ("Leaderboard — Investment Gains", "No investment data yet — users need to make trades first."),
        Sort::Creds   => ("Leaderboard — Creds", "No users found."),
    };
    serenity::CreateEmbed::new()
        .title(title)
        .color(data::EMBED_ERROR)
        .description(desc)
        .footer(default_footer())
}

struct Board<'a> {
    entries: &'a [Entry],
    thumb:   &'a str,
}

struct Boards<'a> {
    creds:   Board<'a>,
    fortune: Board<'a>,
    invest:  Board<'a>,
}

impl<'a> Boards<'a> {
    const fn get(&self, sort: Sort) -> &Board<'a> {
        match sort {
            Sort::Creds   => &self.creds,
            Sort::Fortune => &self.fortune,
            Sort::Invest  => &self.invest,
        }
    }
}

fn render(boards: &Boards<'_>, sort: Sort, page: usize) -> serenity::CreateEmbed {
    let board = boards.get(sort);
    if board.entries.is_empty() { empty_embed(sort) } else { make_embed(board.entries, sort, page, board.thumb) }
}

/// show server rankings — use buttons to switch between Creds, Fortune, and Investment
#[poise::command(slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let serenity_ctx = ctx.serenity_context().clone();

    // Snapshot (UserId, Arc) pairs without holding DashMap shard locks across any await point.
    let user_arcs: Vec<_> = ctx.data().users.iter()
        .map(|e| (*e.key(), Arc::clone(e.value())))
        .collect();

    // Read all user stats concurrently — RwLock reads, no DashMap involvement.
    let stats: Vec<(UserId, i32, i32, String, f64, f64)> =
        futures::future::join_all(user_arcs.iter().map(|(id, u)| async move {
            let u = u.read().await;
            let creds      = u.get_creds();
            let luck_score = u.get_rolling_luck_score();
            let luck_label = u.get_rolling_luck();
            let (pnl, cost) = u.stock.trade_history.iter()
                .filter_map(|t| t.realized_pnl.map(|p| (p, t.total_creds - p)))
                .fold((0.0f64, 0.0f64), |(pa, ca), (p, c)| (pa + p, ca + c));
            (*id, creds, luck_score, luck_label, pnl, cost)
        })).await;

    // Fetch username + avatar URL for every user concurrently — one API call per user,
    // covers both the display name and the thumbnail we need for the top-ranked user.
    let meta: Vec<(String, String)> =
        futures::future::join_all(stats.iter().map(|(id, ..)| {
            let ctx = serenity_ctx.clone();
            let id = *id;
            async move {
                match id.to_user(&ctx).await {
                    Ok(u) => (u.name.clone(), u.avatar_url().unwrap_or_default()),
                    Err(_) => ("Unknown".to_string(), String::new()),
                }
            }
        })).await;

    // Build the three sorted leaderboard vectors.
    let mut creds_info:   Vec<Entry> = Vec::new();
    let mut fortune_info: Vec<Entry> = Vec::new();
    let mut invest_info:  Vec<Entry> = Vec::new();

    for ((id, creds, luck_score, luck_label, pnl, cost), (name, _)) in stats.iter().zip(meta.iter()) {
        creds_info.push((*id, i64::from(*creds), creds.to_string(), name.clone()));

        if *luck_score > 0 {
            // Numeric score shown alongside tier so ties within "Blessed" are distinguishable.
            fortune_info.push((*id, i64::from(*luck_score), format!("{luck_label} ({luck_score})"), name.clone()));
        }

        if *pnl != 0.0 {
            let pct   = if *cost > 0.0 { pnl / cost * 100.0 } else { 0.0 };
            let label = format!("{} ({:+.1}%)", fmt_pnl_short(pnl / 100.0), pct);
            invest_info.push((*id, *pnl as i64, label, name.clone()));
        }
    }

    // Primary: sort key descending. Secondary: username ascending — deterministic tie-breaking.
    creds_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.3.cmp(&b.3)));
    fortune_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.3.cmp(&b.3)));
    invest_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.3.cmp(&b.3)));

    if creds_info.is_empty() {
        ctx.say("No users found.").await?;
        return Ok(());
    }

    // Resolve thumbnails from already-fetched meta — no extra API calls needed.
    let avatar_of = |id: UserId| -> String {
        stats.iter().zip(meta.iter())
            .find(|((uid, ..), _)| *uid == id)
            .map(|(_, (_, url))| url.clone())
            .unwrap_or_default()
    };
    let creds_thumb   = avatar_of(creds_info[0].0);
    let fortune_thumb = fortune_info.first().map_or_else(|| creds_thumb.clone(), |(id, ..)| avatar_of(*id));
    let invest_thumb  = invest_info.first().map_or_else(|| creds_thumb.clone(), |(id, ..)| avatar_of(*id));

    let boards = Boards {
        creds:   Board { entries: &creds_info,   thumb: creds_thumb.as_str()   },
        fortune: Board { entries: &fortune_info, thumb: fortune_thumb.as_str() },
        invest:  Board { entries: &invest_info,  thumb: invest_thumb.as_str()  },
    };

    let reply = ctx.send(poise::CreateReply::default()
        .embed(make_embed(&creds_info, Sort::Creds, 0, &creds_thumb))
        .components(make_components(Sort::Creds))
    ).await?;
    let mut msg = reply.into_message().await?;

    // Interaction loop — no spawn, no Arc<RwLock<Message>>, no silent unwrap panics.
    let mut sort = Sort::Creds;
    let mut page: usize = 0;

    loop {
        let Some(press) = msg
            .await_component_interaction(&serenity_ctx)
            .timeout(Duration::from_secs(60))
            .await
        else {
            // Timeout — grey out the current view and strip buttons.
            let embed = render(&boards, sort, page).color(data::EMBED_ERROR);
            msg.edit(&serenity_ctx, EditMessage::default().embed(embed).components(vec![])).await.ok();
            break;
        };

        press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

        // Compute page bounds from the current sort before applying the new button action.
        let total_pages = boards.get(sort).entries.len().div_ceil(10).max(1);

        match press.data.custom_id.as_str() {
            "lb_creds"   => { sort = Sort::Creds;   page = 0; }
            "lb_fortune" => { sort = Sort::Fortune; page = 0; }
            "lb_invest"  => { sort = Sort::Invest;  page = 0; }
            "lb_back"    => { page = page.saturating_sub(1); }
            "lb_next"    => { if page + 1 < total_pages { page += 1; } }
            _            => continue,
        }

        let embed = render(&boards, sort, page);
        msg.edit(&serenity_ctx, EditMessage::default().embed(embed).components(make_components(sort))).await.ok();
    }

    Ok(())
}
