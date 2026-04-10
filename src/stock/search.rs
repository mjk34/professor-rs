//! /search command — single detailed view and compact multi-asset view.

use crate::api::{self, market_data_err, resolve_ticker, with_logo, FmpProfile, FmpRatios};
use crate::data::{self, AssetType, OrderSide, PendingOrder, MAX_PENDING_ORDERS};
use crate::helper::{creds_to_price, default_footer, fmt_limit_tag, fmt_qty, format_large_num, price_to_creds};
use crate::stock::modals::{BuyModal, SellModal};
use crate::trader::{apply_buy, apply_sell};
use crate::{serenity, Context, Error};
use crate::api::{is_market_hours, order_expiry};
use poise::serenity_prelude::futures;
use std::time::Duration;

/// Builds the embed for a single ticker's detailed view.
fn build_quote_embed(
    ticker: &str,
    display_name: &str,
    price_usd: f64,
    change: f64,
    change_pct: f64,
    market_status: &str,
    profile: &Option<FmpProfile>,
    ratios: &Option<FmpRatios>,
) -> serenity::CreateEmbed {
    let color = if change >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
    let arrow = if change_pct >= 0.0 { "+" } else { "" };
    let mut desc = format!("**${price_usd:.2}** ({arrow}{change_pct:.2}%)\n");

    if let Some(p) = profile {
        desc += "─────────────────────\n";
        let mut meta = Vec::new();
        if let Some(sector) = &p.sector { meta.push(sector.clone()); }
        if let Some(industry) = &p.industry { meta.push(industry.clone()); }
        if !meta.is_empty() { desc += &format!("{}\n", meta.join(" · ")); }
        if let Some(country) = &p.country {
            let exchange = p.exchange.as_deref().unwrap_or("");
            desc += &format!("Country: **{}**  Exchange: **{}**\n", country.to_uppercase(), exchange.to_uppercase());
        }
        desc += "─────────────────────\n";
        if let Some(vol) = p.volume { desc += &format!("Volume: **{}**\n", format_large_num(vol as f64)); }
        if let Some(mc) = p.market_cap { desc += &format!("Market Cap: **{}**\n", format_large_num(mc)); }
        if let Some(pe) = ratios.as_ref().and_then(|r| r.pe_ratio).filter(|&v| v > 0.0) {
            desc += &format!("P/E (TTM): **{pe:.2}**\n");
        }
        if let Some(r) = &p.range { desc += &format!("52-Week: **{r}**\n"); }
    }
    desc += &format!("\n{market_status}");

    with_logo(
        serenity::CreateEmbed::new()
            .title(format!("{ticker} — {display_name}"))
            .description(desc)
            .color(color)
            .footer(default_footer()),
        ticker,
    )
}

/// Look up a stock, ETF, or crypto by ticker symbol
#[poise::command(slash_command)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Ticker symbol — comma-separated for multiple (e.g. NVDA, AAPL, BTC-USD)"]
    query: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let items: Vec<&str> = query.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();

    if items.len() == 1 {
        // ── Single detailed view ──────────────────────────────────────────────
        let quote = if let Some(q) = resolve_ticker(items[0]).await { q } else {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Search")
                    .description(market_data_err(items[0]))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        };
        let ticker = quote.symbol.clone();
        let (profile, ratios) = tokio::join!(
            api::fetch_fmp_profile(&ticker),
            api::fetch_fmp_ratios(&ticker)
        );

        let price_usd = profile.as_ref().and_then(|p| p.price).or(quote.regular_market_price).unwrap_or(0.0);
        let change     = profile.as_ref().and_then(|p| p.change).unwrap_or(0.0);
        let change_pct = profile.as_ref().and_then(|p| p.change_percentage).unwrap_or(0.0);
        let display_name = profile.as_ref().and_then(|p| p.company_name.clone()).unwrap_or_else(|| quote.display_name());
        let market_status = quote.market_status();

        let embed = build_quote_embed(&ticker, &display_name, price_usd, change, change_pct, &market_status, &profile, &ratios);

        let data_ref = &ctx.data().users;
        let u = data_ref.get(&ctx.author().id).unwrap();

        let has_position = {
            let user_data = u.read().await;
            user_data.stock.portfolios.iter().any(|p| {
                p.positions.iter().any(|pos| pos.ticker == ticker && !matches!(&pos.asset_type, AssetType::Option(_)))
            })
        };

        let make_buttons = |disabled: bool| {
            let mut btns = vec![
                serenity::CreateButton::new("search_buy").label("Buy").style(serenity::ButtonStyle::Success).disabled(disabled),
            ];
            if has_position {
                btns.push(serenity::CreateButton::new("search_sell").label("Sell").style(serenity::ButtonStyle::Danger).disabled(disabled));
            }
            vec![serenity::CreateActionRow::Buttons(btns)]
        };
        let reply = ctx.send(poise::CreateReply::default().embed(embed.clone()).components(make_buttons(false))).await?;
        let msg = reply.message().await?;

        let (is_buy, port_name, modal_amount, limit_price) = loop {
            let Some(press) = msg
                .await_component_interaction(ctx.serenity_context())
                .author_id(ctx.author().id)
                .timeout(Duration::from_secs(45))
                .await
            else {
                reply.edit(ctx, poise::CreateReply::default().embed(embed).components(vec![])).await?;
                return Ok(());
            };

            let is_buy = press.data.custom_id == "search_buy";

            let (default_port, has_portfolios) = {
                let user_data = u.read().await;
                let ports = &user_data.stock.portfolios;
                if ports.is_empty() {
                    (String::new(), false)
                } else if is_buy {
                    (ports[0].name.clone(), true)
                } else {
                    let default = ports.iter()
                        .find(|p| p.positions.iter().any(|pos| pos.ticker == ticker && !matches!(&pos.asset_type, AssetType::Option(_))))
                        .or_else(|| ports.first())
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    (default, true)
                }
            };

            if !has_portfolios {
                press.create_response(ctx.http(), serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .embed(serenity::CreateEmbed::new()
                            .title(if is_buy { "Buy" } else { "Sell" })
                            .description("You have no portfolios. Create one with `/portfolio create`.")
                            .color(data::EMBED_ERROR))
                )).await?;
                continue;
            }

            reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(make_buttons(true))).await?;

            let modal_result = if is_buy {
                let portfolio_info = {
                    let user_data = u.read().await;
                    let lines: Vec<String> = user_data.stock.portfolios.iter()
                        .map(|p| {
                            let max_shares = if price_usd > 0.0 { p.cash / price_to_creds(price_usd) } else { 0.0 };
                            format!("{} (${:.2}) - max {} shares", p.name, creds_to_price(p.cash), fmt_qty(max_shares))
                        })
                        .collect();
                    lines.join("\n")
                };
                poise::execute_modal_on_component_interaction::<BuyModal>(
                    ctx, press,
                    Some(BuyModal { portfolio: String::new(), amount: String::new(), limit_price: String::new(), portfolio_info }),
                    Some(Duration::from_secs(30)),
                ).await?.map(|m| (m.portfolio, m.amount, m.limit_price))
            } else {
                let holdings_info = {
                    let user_data = u.read().await;
                    user_data.stock.portfolios.iter()
                        .find(|p| p.name == default_port)
                        .and_then(|p| p.positions.iter().find(|pos| pos.ticker == ticker && !matches!(&pos.asset_type, AssetType::Option(_))))
                        .map(|pos| format!("{} shares (${:.2})", fmt_qty(pos.quantity), pos.quantity * price_usd))
                        .unwrap_or_default()
                };
                poise::execute_modal_on_component_interaction::<SellModal>(
                    ctx, press,
                    Some(SellModal { portfolio: default_port, amount: String::new(), limit_price: String::new(), holdings_info }),
                    Some(Duration::from_secs(30)),
                ).await?.map(|m| (m.portfolio, m.amount, m.limit_price))
            };

            let Some((port_name, modal_amount, limit_str)) = modal_result else {
                reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(make_buttons(false))).await?;
                continue;
            };

            let limit_price: Option<f64> = limit_str.trim().parse::<f64>().ok().filter(|&v| v > 0.0);
            reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(vec![])).await?;
            break (is_buy, port_name, modal_amount, limit_price);
        };

        // Parse input: "$500" → dollar amount, "10" → shares
        let input = modal_amount.trim();
        let sell_all = !is_buy && input.to_lowercase() == "all";
        let sell_pct: Option<f64> = if !is_buy && !sell_all {
            input.strip_suffix('%')
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|&v| v > 0.0 && v <= 100.0)
        } else {
            None
        };

        let (quantity_opt, amount_opt): (Option<f64>, Option<f64>) = if sell_all || sell_pct.is_some() {
            (None, None)
        } else if let Some(stripped) = input.strip_prefix('$') {
            if let Some(a) = stripped.replace(',', "").parse::<f64>().ok().filter(|&v| v > 0.0) {
                (None, Some(a))
            } else {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title(if is_buy { "Buy" } else { "Sell" })
                        .description("Invalid dollar amount — use e.g. `$500`.")
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            }
        } else if let Some(q) = input.replace(',', "").parse::<f64>().ok().filter(|&v| v > 0.0) {
            (Some(q), None)
        } else {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(if is_buy { "Buy" } else { "Sell" })
                    .description(if is_buy {
                        "Invalid share count — use e.g. `10`, or `$500` for a dollar amount."
                    } else {
                        "Invalid amount — use e.g. `10`, `$500`, `50%`, or `all`."
                    })
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        };

        let asset_type = quote.asset_type();
        let price_per_unit = price_to_creds(price_usd);

        if is_buy {
            let qty = quantity_opt.unwrap_or_else(|| amount_opt.unwrap() / price_usd);
            let total_cost = match amount_opt {
                Some(amt) => price_to_creds(amt),
                None => price_per_unit * qty,
            };
            let should_queue = !is_market_hours() || limit_price.is_some_and(|lp| price_usd > lp);

            let mut user_data = u.write().await;
            let Some(port_idx) = user_data.stock.portfolios.iter().position(|p| p.name == port_name) else {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Buy")
                        .description(format!("Portfolio **{port_name}** not found."))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            };

            if should_queue {
                if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                    drop(user_data);
                    ctx.send(poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new().title("Queue Failed")
                            .description(format!("You have reached the limit of **{MAX_PENDING_ORDERS}** pending orders. Cancel some in `/portfolio`."))
                            .color(data::EMBED_ERROR),
                    )).await?;
                    return Ok(());
                }
                let expiry = order_expiry();
                let id = user_data.stock.next_order_id;
                user_data.stock.next_order_id = id.wrapping_add(1);
                user_data.stock.pending_orders.push(PendingOrder {
                    id, side: OrderSide::Buy, ticker: ticker.clone(),
                    asset_name: display_name.clone(), asset_type,
                    portfolio_name: port_name.clone(), quantity: qty, limit_price, expiry,
                });
                drop(user_data);
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Buy Order Queued")
                        .description(format!(
                            "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                            fmt_qty(qty), ticker, fmt_limit_tag(limit_price), port_name, qty * price_usd, expiry.timestamp(),
                        ))
                        .color(data::EMBED_SUCCESS).footer(default_footer()),
                )).await?;
                return Ok(());
            }

            if user_data.stock.portfolios[port_idx].cash < total_cost {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Buy")
                        .description(format!(
                            "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                            creds_to_price(total_cost), total_cost, port_name,
                            creds_to_price(user_data.stock.portfolios[port_idx].cash),
                            user_data.stock.portfolios[port_idx].cash,
                        ))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            }

            {
                let stock = &mut user_data.stock;
                apply_buy(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &display_name, asset_type, qty, price_per_unit, total_cost, &port_name);
            }
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                with_logo(
                    serenity::CreateEmbed::new().title("Buy")
                        .description(format!(
                            "Bought **{} {}** ({}) for **${:.2}** ({:.0} creds)\n${:.2}/unit | Portfolio: **{}**",
                            fmt_qty(qty), ticker, display_name, creds_to_price(total_cost), total_cost, price_usd, port_name,
                        ))
                        .color(data::EMBED_SUCCESS).footer(default_footer()),
                    &ticker,
                )
            )).await?;
        } else {
            let mut user_data = u.write().await;
            let Some(port_idx) = user_data.stock.portfolios.iter().position(|p| p.name == port_name) else {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Sell")
                        .description(format!("Portfolio **{port_name}** not found."))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            };

            let pos_idx = if let Some(i) = user_data.stock.portfolios[port_idx].positions.iter()
                .position(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_))) { i } else {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Sell")
                        .description(format!("No **{ticker}** position in portfolio **{port_name}**."))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            };

            let held = user_data.stock.portfolios[port_idx].positions[pos_idx].quantity;
            let qty = if sell_all { held }
                else if let Some(pct) = sell_pct { held * (pct / 100.0) }
                else {
                    let raw = quantity_opt.unwrap_or_else(|| amount_opt.unwrap() / price_usd);
                    if (raw - held).abs() < 5e-5 { held } else { raw }
                };

            if qty > held + 1e-9 {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Sell")
                        .description(format!("You only hold **{}** of **{}** but tried to sell **{}**.", fmt_qty(held), ticker, fmt_qty(qty)))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            }

            let should_queue = !is_market_hours() || limit_price.is_some_and(|lp| price_usd < lp);

            if should_queue {
                if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                    drop(user_data);
                    ctx.send(poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new().title("Queue Failed")
                            .description(format!("You have reached the limit of **{MAX_PENDING_ORDERS}** pending orders. Cancel some in `/portfolio`."))
                            .color(data::EMBED_ERROR),
                    )).await?;
                    return Ok(());
                }
                let expiry = order_expiry();
                let id = user_data.stock.next_order_id;
                user_data.stock.next_order_id = id.wrapping_add(1);
                user_data.stock.pending_orders.push(PendingOrder {
                    id, side: OrderSide::Sell, ticker: ticker.clone(),
                    asset_name: display_name.clone(), asset_type: AssetType::Stock,
                    portfolio_name: port_name.clone(), quantity: qty, limit_price, expiry,
                });
                drop(user_data);
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new().title("Sell Order Queued")
                        .description(format!(
                            "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                            fmt_qty(qty), ticker, fmt_limit_tag(limit_price), port_name, qty * price_usd, expiry.timestamp(),
                        ))
                        .color(data::EMBED_SUCCESS).footer(default_footer()),
                )).await?;
                return Ok(());
            }

            let (proceeds, pnl) = {
                let stock = &mut user_data.stock;
                let pnl = apply_sell(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &display_name, qty, price_per_unit, &port_name).unwrap_or(0.0);
                (price_per_unit * qty, pnl)
            };
            let pnl_str = if pnl >= 0.0 { format!("▲ +${:.2} ({:.0} creds)", creds_to_price(pnl), pnl) }
                          else           { format!("▼ -${:.2} ({:.0} creds)", creds_to_price(pnl.abs()), pnl) };
            let pnl_color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                with_logo(
                    serenity::CreateEmbed::new().title("Sell")
                        .description(format!(
                            "Sold **{} {}** for **${:.2}** ({:.0} creds)\n${:.2}/unit | Realized P&L: **{}**",
                            fmt_qty(qty), ticker, creds_to_price(proceeds), proceeds, price_usd, pnl_str,
                        ))
                        .color(pnl_color).footer(default_footer()),
                    &ticker,
                )
            )).await?;
        }
    } else {
        // ── Compact multi-asset view (max 10) ─────────────────────────────────
        let results = futures::future::join_all(
            items.iter().take(10).map(|&q| async move { (q, resolve_ticker(q).await) })
        ).await;
        let mut rows = Vec::new();
        for (q, quote_opt) in results {
            let Some(quote) = quote_opt else {
                rows.push(format!("`{q}` — could not resolve"));
                continue;
            };
            let price_usd  = quote.regular_market_price.unwrap_or(0.0);
            let change_pct = quote.regular_market_change_percent.unwrap_or(0.0);
            let arrow = if change_pct >= 0.0 { "▲" } else { "▼" };
            rows.push(format!("**{}** — {} | ${:.2} | {} {:.2}%", quote.symbol, quote.display_name(), price_usd, arrow, change_pct.abs()));
        }
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Search Results")
                .description(rows.join("\n"))
                .color(data::EMBED_CYAN)
                .footer(default_footer()),
        )).await?;
    }

    Ok(())
}
