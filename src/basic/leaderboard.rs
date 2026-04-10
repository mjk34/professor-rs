//! /leaderboard command — creds, fortune, and investment rankings with pagination.

use crate::{data, serenity, Context, Error};
use crate::helper::default_footer;
use poise::serenity_prelude::{EditMessage, futures::StreamExt, UserId};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

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

fn build_page(info: &[(UserId, i64, String, String)], start: usize) -> String {
    let mut text = String::from("﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n```\n");
    for (index, (_id, value, label, user_name)) in info.iter().enumerate().skip(start).take(10) {
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

fn make_embed(
    info: &[(UserId, i64, String, String)],
    sort: Sort,
    page: usize,
    thumbnail: &str,
) -> serenity::CreateEmbed {
    let total_pages = info.len().div_ceil(10);
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

/// show server rankings — use buttons to switch between Creds, Fortune, and Investment
#[poise::command(slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
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

        creds_info.push((*id, i64::from(creds), creds.to_string(), user_name.clone()));

        if luck_score > 0 {
            fortune_info.push((*id, i64::from(luck_score), luck_label, user_name.clone()));
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

    let creds_thumb   = creds_info[0].0.to_user(ctx).await?.avatar_url().unwrap_or_default();
    let fortune_thumb = if let Some(e) = fortune_info.first() { e.0.to_user(ctx).await?.avatar_url().unwrap_or_else(|| creds_thumb.clone()) } else { creds_thumb.clone() };
    let invest_thumb  = if let Some(e) = invest_info.first()  { e.0.to_user(ctx).await?.avatar_url().unwrap_or_else(|| creds_thumb.clone()) } else { creds_thumb.clone() };

    let initial_embed = make_embed(&creds_info, Sort::Creds, 0, &creds_thumb);
    let initial_components = make_components(Sort::Creds);

    let reply = ctx
        .send(poise::CreateReply::default().embed(initial_embed).components(initial_components))
        .await?;

    let msg_og = Arc::new(RwLock::new(reply.into_message().await?));
    let msg = Arc::clone(&msg_og);

    let mut interactions = msg.read().await.await_component_interactions(ctx).stream();
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
            let total_pages = active_info.len().div_ceil(10);

            match interaction.data.custom_id.as_str() {
                "lb_creds"   => { current_sort = Sort::Creds;   current_page = 0; }
                "lb_fortune" => { current_sort = Sort::Fortune; current_page = 0; }
                "lb_invest"  => { current_sort = Sort::Invest;  current_page = 0; }
                "lb_back"    => { current_page = current_page.saturating_sub(1); }
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

            interaction.create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge).await.unwrap();

            msg.write().await
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
        let timed_out_embed = make_embed(active_info, current_sort, current_page, thumb).color(data::EMBED_ERROR);
        msg.write().await
            .edit(&ctx, EditMessage::default().embed(timed_out_embed).components(vec![]))
            .await
            .ok();
    });

    Ok(())
}
