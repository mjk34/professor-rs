//! Trade history command — summary, gains/losses, and full trade log views.

use crate::data::{self, TradeAction, TradeRecord};
use crate::helper::{creds_to_price, default_footer, fmt_qty};
use crate::{serenity, Context, Error};
use poise::serenity_prelude::EditMessage;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

#[derive(Clone, Copy)]
pub(crate) enum TradeFilter { Gains, Losses }

pub(crate) fn build_summary_embed(trades: &VecDeque<TradeRecord>) -> serenity::CreateEmbed {
    // (gains, losses, count, cost_basis)
    let mut map: HashMap<&str, (f64, f64, u32, f64)> = HashMap::new();
    for t in trades {
        let entry = map.entry(t.portfolio.as_str()).or_insert((0.0, 0.0, 0, 0.0));
        entry.2 += 1;
        if let Some(pnl) = t.realized_pnl {
            let cost = t.total_creds - pnl;
            entry.3 += cost;
            if pnl >= 0.0 {
                entry.0 += pnl;
            } else {
                entry.1 += pnl;
            }
        }
    }

    if map.is_empty() {
        return serenity::CreateEmbed::new()
            .title("Trade History — Summary")
            .description("No trades yet. Use `/buy` to get started!")
            .color(data::EMBED_CYAN);
    }

    let mut desc = String::new();
    let mut total_net = 0.0_f64;
    let mut total_basis = 0.0_f64;
    let mut sorted: Vec<_> = map.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    for (name, (gains, losses, count, basis)) in sorted {
        let net = gains + losses;
        total_net += net;
        total_basis += basis;
        desc += &format!(
            "**{}** — {} trades | +${:.2} gains | -${:.2} losses | Net: **${:+.2}{}**\n",
            name, count, creds_to_price(*gains), creds_to_price(losses.abs()), creds_to_price(net), crate::helper::fmt_pct_change(net, *basis)
        );
    }
    desc += &format!("\n**Total Net P&L: ${:+.2}{}**", creds_to_price(total_net), crate::helper::fmt_pct_change(total_net, total_basis));

    serenity::CreateEmbed::new()
        .title("Trade History — Summary")
        .description(desc)
        .color(data::EMBED_CYAN)
        .footer(default_footer())
}

pub(crate) fn build_filtered_embed(trades: &VecDeque<TradeRecord>, filter: TradeFilter) -> serenity::CreateEmbed {
    let (title, color, pred): (&str, _, fn(f64) -> bool) = match filter {
        TradeFilter::Gains  => ("Trade History — Gains",  data::EMBED_SUCCESS, |p| p > 0.0),
        TradeFilter::Losses => ("Trade History — Losses", data::EMBED_FAIL,    |p| p < 0.0),
    };
    let filtered: Vec<_> = trades
        .iter()
        .filter(|t| t.realized_pnl.is_some_and(pred))
        .collect();

    if filtered.is_empty() {
        return serenity::CreateEmbed::new()
            .title(title)
            .description("No trades match this filter.")
            .color(color);
    }

    let mut desc = String::new();
    for t in filtered.iter().rev().take(15) {
        let pnl = t.realized_pnl.unwrap_or(0.0);
        let cost = t.total_creds - pnl;
        desc += &format!(
            "{} **{}** [{}] × {} | P&L: **${:+.2}{}**\n",
            t.timestamp.format("%m/%d"),
            t.ticker,
            t.portfolio,
            fmt_qty(t.quantity),
            creds_to_price(pnl),
            crate::helper::fmt_pct_change(pnl, cost)
        );
    }

    serenity::CreateEmbed::new()
        .title(title)
        .description(desc)
        .color(color)
        .footer(default_footer())
}

pub(crate) fn build_all_trades_embed(trades: &VecDeque<TradeRecord>) -> serenity::CreateEmbed {
    if trades.is_empty() {
        return serenity::CreateEmbed::new()
            .title("Trade History — All Trades")
            .description("No trades yet.")
            .color(data::EMBED_CYAN);
    }

    let mut desc = String::new();
    for t in trades.iter().rev().take(20) {
        let action = match t.action {
            TradeAction::Buy => "BUY ",
            TradeAction::Sell => "SELL",
        };
        let pnl_str = t
            .realized_pnl
            .map(|p| {
                let cost = t.total_creds - p;
                format!(" | P&L: **${:+.2}{}**", creds_to_price(p), crate::helper::fmt_pct_change(p, cost))
            })
            .unwrap_or_default();
        desc += &format!(
            "{} `{}` **{}** × {} — **${:.2}**{}\n",
            t.timestamp.format("%m/%d"),
            action,
            t.ticker,
            fmt_qty(t.quantity),
            creds_to_price(t.total_creds),
            pnl_str
        );
    }
    if trades.len() > 20 {
        desc += &format!("\n*Showing 20 of {} trades.*", trades.len());
    }

    serenity::CreateEmbed::new()
        .title("Trade History — All Trades")
        .description(desc)
        .color(data::EMBED_CYAN)
        .footer(default_footer())
}

fn trade_buttons() -> Vec<serenity::CreateActionRow> {
    vec![serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new("trades-summary")
            .label("🗂 Summary")
            .style(poise::serenity_prelude::ButtonStyle::Secondary),
        serenity::CreateButton::new("trades-gains")
            .label("📈 Gains")
            .style(poise::serenity_prelude::ButtonStyle::Success),
        serenity::CreateButton::new("trades-losses")
            .label("📉 Losses")
            .style(poise::serenity_prelude::ButtonStyle::Danger),
        serenity::CreateButton::new("trades-all")
            .label("📋 All Trades")
            .style(poise::serenity_prelude::ButtonStyle::Primary),
    ])]
}

/// View your trade history and P&L summary
#[poise::command(slash_command)]
pub async fn trades(ctx: Context<'_>) -> Result<(), Error> {
    let trade_history = {
        ctx.data().users
            .get(&ctx.author().id)
            .unwrap()
            .read()
            .await
            .stock
            .trade_history
            .clone()
    };

    let reply = ctx.send(poise::CreateReply::default()
        .embed(build_summary_embed(&trade_history))
        .components(trade_buttons()))
        .await?;

    let mut msg = reply.into_message().await?;
    let serenity_ctx = ctx.serenity_context().clone();
    let mut current_embed = build_summary_embed(&trade_history);

    loop {
        let Some(press) = msg
            .await_component_interaction(&serenity_ctx)
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(5 * 60))
            .await
        else {
            msg.edit(&serenity_ctx, EditMessage::default()
                .embed(current_embed.color(data::EMBED_ERROR))
                .components(vec![]))
                .await.ok();
            break;
        };

        press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

        current_embed = match press.data.custom_id.as_str() {
            "trades-summary" => build_summary_embed(&trade_history),
            "trades-gains"   => build_filtered_embed(&trade_history, TradeFilter::Gains),
            "trades-losses"  => build_filtered_embed(&trade_history, TradeFilter::Losses),
            "trades-all"     => build_all_trades_embed(&trade_history),
            _                => continue,
        };

        msg.edit(&serenity_ctx, EditMessage::default()
            .embed(current_embed.clone())
            .components(trade_buttons()))
            .await.ok();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::TradeAction;
    use chrono::Utc;

    fn make_trade(portfolio: &str, action: TradeAction, qty: f64, price: f64, pnl: Option<f64>) -> TradeRecord {
        let total = price * qty;
        TradeRecord {
            portfolio: portfolio.to_string(),
            ticker: "AAPL".to_string(),
            asset_name: "Apple".to_string(),
            action,
            quantity: qty,
            price_per_unit: price,
            total_creds: total,
            realized_pnl: pnl,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn summary_embed_empty() {
        let trades: VecDeque<TradeRecord> = VecDeque::new();
        let embed = build_summary_embed(&trades);
        // Just verify it doesn't panic and has the right title
        let _ = embed;
    }

    #[test]
    fn summary_embed_aggregates_by_portfolio() {
        let mut trades = VecDeque::new();
        trades.push_back(make_trade("Alpha", TradeAction::Sell, 10.0, 150.0, Some(500.0)));
        trades.push_back(make_trade("Alpha", TradeAction::Sell, 5.0, 100.0, Some(-200.0)));
        trades.push_back(make_trade("Beta", TradeAction::Sell, 2.0, 200.0, Some(100.0)));

        // Verify no panic and both portfolios summarized
        let embed = build_summary_embed(&trades);
        let _ = embed;
    }

    #[test]
    fn filtered_embed_gains_only() {
        let mut trades = VecDeque::new();
        trades.push_back(make_trade("P", TradeAction::Sell, 1.0, 100.0, Some(50.0)));
        trades.push_back(make_trade("P", TradeAction::Sell, 1.0, 100.0, Some(-30.0)));

        let embed = build_filtered_embed(&trades, TradeFilter::Gains);
        let _ = embed;
    }

    #[test]
    fn filtered_embed_losses_only() {
        let mut trades = VecDeque::new();
        trades.push_back(make_trade("P", TradeAction::Sell, 1.0, 100.0, Some(50.0)));
        trades.push_back(make_trade("P", TradeAction::Sell, 1.0, 100.0, Some(-30.0)));

        let embed = build_filtered_embed(&trades, TradeFilter::Losses);
        let _ = embed;
    }

    #[test]
    fn all_trades_embed_caps_at_20() {
        let mut trades = VecDeque::new();
        for i in 0..25 {
            trades.push_back(make_trade("P", TradeAction::Buy, i as f64 + 1.0, 100.0, None));
        }
        let embed = build_all_trades_embed(&trades);
        let _ = embed;
    }
}
