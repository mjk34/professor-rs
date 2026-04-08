//!---------------------------------------------------------------------!
//! Stock trading commands (search, buy, sell) + buy/sell modals       !
//!---------------------------------------------------------------------!

use crate::api::{is_market_hours, market_data_err, order_expiry, resolve_ticker, with_logo};
use crate::data::{self, AssetType, OrderSide, PendingOrder, MAX_PENDING_ORDERS};
use crate::helper::{creds_to_price, default_footer, fmt_limit_tag, fmt_qty, price_to_creds};
use crate::trader::{apply_buy, apply_sell};
use crate::{serenity, Context, Error};
use poise::serenity_prelude::futures;
use std::time::Duration;

// ── Trade modals (used by /search buy/sell buttons) ───────────────────────────

fn parse_trade_fields(data: &serenity::ModalInteractionData) -> (String, String, String) {
    let mut portfolio   = String::new();
    let mut amount      = String::new();
    let mut limit_price = String::new();
    for row in &data.components {
        for comp in &row.components {
            if let serenity::ActionRowComponent::InputText(t) = comp {
                match t.custom_id.as_str() {
                    "portfolio"   => portfolio   = t.value.clone().unwrap_or_default(),
                    "amount"      => amount      = t.value.clone().unwrap_or_default(),
                    "limit_price" => limit_price = t.value.clone().unwrap_or_default(),
                    _ => {}
                }
            }
        }
    }
    (portfolio, amount, limit_price)
}

struct BuyModal {
    portfolio: String,
    amount: String,
    limit_price: String,
    /// Per-portfolio cash breakdown shown in the read-only display field.
    /// Not an input — only used by `create`; `parse` ignores it.
    portfolio_info: String,
}

impl poise::Modal for BuyModal {
    fn create(defaults: Option<Self>, custom_id: String) -> serenity::CreateInteractionResponse {
        let portfolio_val  = defaults.as_ref().map(|d| d.portfolio.as_str()).unwrap_or("");
        let amount_val     = defaults.as_ref().map(|d| d.amount.as_str()).unwrap_or("");
        let portfolio_info = defaults.as_ref().map(|d| d.portfolio_info.as_str()).unwrap_or("");

        let mut components = vec![
            serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short, "Portfolio", "portfolio",
                ).value(portfolio_val)
            ),
        ];

        if !portfolio_info.is_empty() {
            components.push(serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Paragraph, "Available Cash", "portfolio_info",
                )
                .value(portfolio_info)
                .required(false)
            ));
        }

        components.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(
                serenity::InputTextStyle::Short, "Amount (shares e.g. 10, or dollars e.g. $500)", "amount",
            )
            .placeholder("e.g. 10 or $500")
            .value(amount_val)
        ));

        components.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(
                serenity::InputTextStyle::Short, "Limit Price (optional)", "limit_price",
            )
            .placeholder("e.g. 150.00 — buy when price ≤ this (blank = market)")
            .required(false)
        ));

        serenity::CreateInteractionResponse::Modal(
            serenity::CreateModal::new(custom_id, "Buy").components(components)
        )
    }

    fn parse(data: serenity::ModalInteractionData) -> Result<Self, &'static str> {
        let (portfolio, amount, limit_price) = parse_trade_fields(&data);
        Ok(BuyModal { portfolio, amount, limit_price, portfolio_info: String::new() })
    }
}

struct SellModal {
    portfolio: String,
    amount: String,
    limit_price: String,
    /// Dynamic label injected into the Amount field (e.g. "10.5 shares ($1,234.56)").
    /// Not an input — only used by `create`; `parse` leaves it empty.
    holdings_info: String,
}

impl poise::Modal for SellModal {
    fn create(defaults: Option<Self>, custom_id: String) -> serenity::CreateInteractionResponse {
        let portfolio_val = defaults.as_ref().map(|d| d.portfolio.as_str()).unwrap_or("");
        let amount_val    = defaults.as_ref().map(|d| d.amount.as_str()).unwrap_or("");
        let amount_label  = defaults.as_ref().and_then(|d| {
            if d.holdings_info.is_empty() { None }
            else {
                let s = format!("Amount — {}", d.holdings_info);
                Some(s.chars().take(45).collect::<String>())
            }
        }).unwrap_or_else(|| "Amount (shares, dollars, or 'all')".to_string());

        serenity::CreateInteractionResponse::Modal(
            serenity::CreateModal::new(custom_id, "Sell")
                .components(vec![
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short, "Portfolio", "portfolio",
                        ).value(portfolio_val)
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short, amount_label, "amount",
                        )
                        .placeholder("e.g. 10, $500, 50%, or all")
                        .value(amount_val)
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short, "Limit Price (optional)", "limit_price",
                        )
                        .placeholder("e.g. 200.00 — sell when price ≥ this (blank = market)")
                        .required(false)
                    ),
                ])
        )
    }

    fn parse(data: serenity::ModalInteractionData) -> Result<Self, &'static str> {
        let (portfolio, amount, limit_price) = parse_trade_fields(&data);
        Ok(SellModal { portfolio, amount, limit_price, holdings_info: String::new() })
    }
}

// ── Search ─────────────────────────────────────────────────────────────────────

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
        // Detailed single view
        let quote = match resolve_ticker(items[0]).await {
            Some(q) => q,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Search")
                            .description(market_data_err(&items[0]))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };
        let ticker = quote.symbol.clone();
        let (profile, ratios) = tokio::join!(
            crate::api::fetch_fmp_profile(&ticker),
            crate::api::fetch_fmp_ratios(&ticker)
        );

        let price_usd = profile
            .as_ref()
            .and_then(|p| p.price)
            .or(quote.regular_market_price)
            .unwrap_or(0.0);
        let change = profile.as_ref().and_then(|p| p.change).unwrap_or(0.0);
        let change_pct = profile
            .as_ref()
            .and_then(|p| p.change_percentage)
            .unwrap_or(0.0);
        let color = if change >= 0.0 {
            data::EMBED_SUCCESS
        } else {
            data::EMBED_FAIL
        };
        let display_name = profile
            .as_ref()
            .and_then(|p| p.company_name.clone())
            .unwrap_or_else(|| quote.display_name());

        let arrow = if change_pct >= 0.0 { "+" } else { "" };
        let mut desc = format!("**${:.2}** ({}{:.2}%)\n", price_usd, arrow, change_pct);

        if let Some(p) = &profile {
            desc += "─────────────────────\n";
            let mut meta = Vec::new();
            if let Some(sector) = &p.sector { meta.push(sector.clone()); }
            if let Some(industry) = &p.industry { meta.push(industry.clone()); }
            if !meta.is_empty() {
                desc += &format!("{}\n", meta.join(" · "));
            }
            if let Some(country) = &p.country {
                let exchange = p.exchange.as_deref().unwrap_or("");
                desc += &format!("Country: **{}**  Exchange: **{}**\n", country.to_uppercase(), exchange.to_uppercase());
            }
            desc += "─────────────────────\n";
            if let Some(vol) = p.volume {
                desc += &format!("Volume: **{}**\n", crate::helper::format_large_num(vol as f64));
            }
            if let Some(mc) = p.market_cap {
                desc += &format!("Market Cap: **{}**\n", crate::helper::format_large_num(mc));
            }
            if let Some(pe) = ratios.as_ref().and_then(|r| r.pe_ratio).filter(|&v| v > 0.0) {
                desc += &format!("P/E (TTM): **{:.2}**\n", pe);
            }
            if let Some(r) = &p.range {
                desc += &format!("52-Week: **{}**\n", r);
            }
        }
        desc += &format!("\n{}", quote.market_status());

        let embed = with_logo(
            serenity::CreateEmbed::new()
                .title(format!("{} — {}", ticker, display_name))
                .description(desc)
                .color(color)
                .footer(default_footer()),
            &ticker,
        );

        let data_ref = &ctx.data().users;
        let u = data_ref.get(&ctx.author().id).unwrap();

        let has_position = {
            let user_data = u.read().await;
            user_data.stock.portfolios.iter().any(|p| {
                p.positions.iter().any(|pos| {
                    pos.ticker == ticker && !matches!(&pos.asset_type, AssetType::Option(_))
                })
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
        let reply = ctx.send(
            poise::CreateReply::default().embed(embed.clone()).components(make_buttons(false)),
        ).await?;
        let msg = reply.message().await?;

        let (is_buy, port_name, modal_amount, limit_price) = loop {
            // Wait for Buy/Sell button press (60s)
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

            // Determine pre-fill portfolio name
            let (default_port, has_portfolios) = {
                let user_data = u.read().await;
                let ports = &user_data.stock.portfolios;
                if ports.is_empty() {
                    (String::new(), false)
                } else if is_buy {
                    (ports[0].name.clone(), true)
                } else {
                    let default = ports
                        .iter()
                        .find(|p| p.positions.iter().any(|pos| {
                            pos.ticker == ticker
                                && !matches!(&pos.asset_type, AssetType::Option(_))
                        }))
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

            // Disable buttons while the modal is open so there's nothing to
            // accidentally click (avoids "Interaction failed" on re-click)
            reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(make_buttons(true))).await?;

            let modal_result = if is_buy {
                let portfolio_info = {
                    let user_data = u.read().await;
                    let lines: Vec<String> = user_data.stock.portfolios.iter()
                        .map(|p| {
                            let max_shares = if price_usd > 0.0 {
                                p.cash / price_to_creds(price_usd)
                            } else {
                                0.0
                            };
                            format!("{} (${:.2}) - max {} shares", p.name, creds_to_price(p.cash), fmt_qty(max_shares))
                        })
                        .collect();
                    lines.join("\n")
                };
                poise::execute_modal_on_component_interaction::<BuyModal>(
                    ctx,
                    press,
                    Some(BuyModal { portfolio: String::new(), amount: String::new(), limit_price: String::new(), portfolio_info }),
                    Some(Duration::from_secs(30)),
                ).await?.map(|m| (m.portfolio, m.amount, m.limit_price))
            } else {
                let holdings_info = {
                    let user_data = u.read().await;
                    user_data.stock.portfolios.iter()
                        .find(|p| p.name == default_port)
                        .and_then(|p| p.positions.iter().find(|pos| {
                            pos.ticker == ticker && !matches!(&pos.asset_type, AssetType::Option(_))
                        }))
                        .map(|pos| format!("{} shares (${:.2})", fmt_qty(pos.quantity), pos.quantity * price_usd))
                        .unwrap_or_default()
                };
                poise::execute_modal_on_component_interaction::<SellModal>(
                    ctx,
                    press,
                    Some(SellModal { portfolio: default_port, amount: String::new(), limit_price: String::new(), holdings_info }),
                    Some(Duration::from_secs(30)),
                ).await?.map(|m| (m.portfolio, m.amount, m.limit_price))
            };

            let Some((port_name, modal_amount, limit_str)) = modal_result else {
                // Modal dismissed — re-enable buttons
                reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(make_buttons(false))).await?;
                continue;
            };

            let limit_price: Option<f64> = limit_str.trim().parse::<f64>().ok().filter(|&v| v > 0.0);
            reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(vec![])).await?;
            break (is_buy, port_name, modal_amount, limit_price);
        };

        // Parse input: "$500" → dollar amount, "10" → shares
        // Sell also accepts "all" and "50%" — these are resolved after position lookup
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
        } else if input.starts_with('$') {
            match input[1..].replace(',', "").parse::<f64>().ok().filter(|&v| v > 0.0) {
                Some(a) => (None, Some(a)),
                None => {
                    ctx.send(poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title(if is_buy { "Buy" } else { "Sell" })
                            .description("Invalid dollar amount — use e.g. `$500`.")
                            .color(data::EMBED_ERROR),
                    )).await?;
                    return Ok(());
                }
            }
        } else {
            match input.replace(',', "").parse::<f64>().ok().filter(|&v| v > 0.0) {
                Some(q) => (Some(q), None),
                None => {
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
                }
            }
        };

        let asset_type = quote.asset_type();
        let price_per_unit = price_to_creds(price_usd);

        if is_buy {
            let qty = quantity_opt.unwrap_or_else(|| amount_opt.unwrap() / price_usd);
            let total_cost = match amount_opt {
                Some(amt) => price_to_creds(amt),
                None => price_per_unit * qty,
            };
            let should_queue = !is_market_hours()
                || limit_price.map(|lp| price_usd > lp).unwrap_or(false);

            let mut user_data = u.write().await;
            let Some(port_idx) = user_data.stock.portfolios.iter().position(|p| p.name == port_name) else {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(format!("Portfolio **{}** not found.", port_name))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            };

            if should_queue {
                if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                    drop(user_data);
                    ctx.send(poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Queue Failed")
                            .description(format!("You have reached the limit of **{}** pending orders. Cancel some in `/portfolio`.", MAX_PENDING_ORDERS))
                            .color(data::EMBED_ERROR),
                    )).await?;
                    return Ok(());
                }
                let expiry = order_expiry();
                let id = user_data.stock.next_order_id;
                user_data.stock.next_order_id = id.wrapping_add(1);
                user_data.stock.pending_orders.push(PendingOrder {
                    id,
                    side: OrderSide::Buy,
                    ticker: ticker.clone(),
                    asset_name: display_name.clone(),
                    asset_type,
                    portfolio_name: port_name.clone(),
                    quantity: qty,
                    limit_price,
                    expiry,
                });
                drop(user_data);

                let limit_tag = fmt_limit_tag(limit_price);
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy Order Queued")
                        .description(format!(
                            "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                            fmt_qty(qty), ticker, limit_tag, port_name, qty * price_usd, expiry.timestamp(),
                        ))
                        .color(data::EMBED_SUCCESS)
                        .footer(default_footer()),
                )).await?;
                return Ok(());
            }

            if user_data.stock.portfolios[port_idx].cash < total_cost {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
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
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(format!(
                            "Bought **{} {}** ({}) for **${:.2}** ({:.0} creds)\n${:.2}/unit | Portfolio: **{}**",
                            fmt_qty(qty), ticker, display_name, creds_to_price(total_cost), total_cost, price_usd, port_name,
                        ))
                        .color(data::EMBED_SUCCESS)
                        .footer(default_footer()),
                    &ticker,
                )
            )).await?;
        } else {
            let mut user_data = u.write().await;
            let Some(port_idx) = user_data.stock.portfolios.iter().position(|p| p.name == port_name) else {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(format!("Portfolio **{}** not found.", port_name))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            };

            let pos_idx = match user_data.stock.portfolios[port_idx]
                .positions
                .iter()
                .position(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)))
            {
                Some(i) => i,
                None => {
                    ctx.send(poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Sell")
                            .description(format!("No **{}** position in portfolio **{}**.", ticker, port_name))
                            .color(data::EMBED_ERROR),
                    )).await?;
                    return Ok(());
                }
            };

            let held = user_data.stock.portfolios[port_idx].positions[pos_idx].quantity;
            let qty = if sell_all {
                held
            } else if let Some(pct) = sell_pct {
                held * (pct / 100.0)
            } else {
                let raw = quantity_opt.unwrap_or_else(|| amount_opt.unwrap() / price_usd);
                if (raw - held).abs() < 5e-5 { held } else { raw }
            };

            if qty > held + 1e-9 {
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(format!(
                            "You only hold **{}** of **{}** but tried to sell **{}**.",
                            fmt_qty(held), ticker, fmt_qty(qty),
                        ))
                        .color(data::EMBED_ERROR),
                )).await?;
                return Ok(());
            }

            let should_queue = !is_market_hours()
                || limit_price.map(|lp| price_usd < lp).unwrap_or(false);

            if should_queue {
                if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                    drop(user_data);
                    ctx.send(poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Queue Failed")
                            .description(format!("You have reached the limit of **{}** pending orders. Cancel some in `/portfolio`.", MAX_PENDING_ORDERS))
                            .color(data::EMBED_ERROR),
                    )).await?;
                    return Ok(());
                }
                let expiry = order_expiry();
                let id = user_data.stock.next_order_id;
                user_data.stock.next_order_id = id.wrapping_add(1);
                user_data.stock.pending_orders.push(PendingOrder {
                    id,
                    side: OrderSide::Sell,
                    ticker: ticker.clone(),
                    asset_name: display_name.clone(),
                    asset_type: AssetType::Stock,
                    portfolio_name: port_name.clone(),
                    quantity: qty,
                    limit_price,
                    expiry,
                });
                drop(user_data);

                let limit_tag = fmt_limit_tag(limit_price);
                ctx.send(poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell Order Queued")
                        .description(format!(
                            "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                            fmt_qty(qty), ticker, limit_tag, port_name, qty * price_usd, expiry.timestamp(),
                        ))
                        .color(data::EMBED_SUCCESS)
                        .footer(default_footer()),
                )).await?;
                return Ok(());
            }

            let (proceeds, pnl) = {
                let stock = &mut user_data.stock;
                let pnl = apply_sell(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &display_name, qty, price_per_unit, &port_name).unwrap_or(0.0);
                (price_per_unit * qty, pnl)
            };

            let pnl_str = if pnl >= 0.0 {
                format!("▲ +${:.2} ({:.0} creds)", creds_to_price(pnl), pnl)
            } else {
                format!("▼ -${:.2} ({:.0} creds)", creds_to_price(pnl.abs()), pnl)
            };
            let pnl_color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
            drop(user_data);

            ctx.send(poise::CreateReply::default().embed(
                with_logo(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(format!(
                            "Sold **{} {}** for **${:.2}** ({:.0} creds)\n${:.2}/unit | Realized P&L: **{}**",
                            fmt_qty(qty), ticker, creds_to_price(proceeds), proceeds, price_usd, pnl_str,
                        ))
                        .color(pnl_color)
                        .footer(default_footer()),
                    &ticker,
                )
            )).await?;
        }
    } else {
        // Compact multi-asset view (max 10)
        let results = futures::future::join_all(
            items.iter().take(10).map(|&q| async move { (q, resolve_ticker(q).await) })
        ).await;
        let mut rows = Vec::new();
        for (q, quote_opt) in results {
            let Some(quote) = quote_opt else {
                rows.push(format!("`{}` — could not resolve", q));
                continue;
            };
            let ticker = quote.symbol.clone();
            let price_usd = quote.regular_market_price.unwrap_or(0.0);
            let change_pct = quote.regular_market_change_percent.unwrap_or(0.0);
            let arrow = if change_pct >= 0.0 { "▲" } else { "▼" };
            rows.push(format!(
                "**{}** — {} | ${:.2} | {} {:.2}%",
                ticker,
                quote.display_name(),
                price_usd,
                arrow,
                change_pct.abs()
            ));
        }

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Search Results")
                    .description(rows.join("\n"))
                    .color(data::EMBED_CYAN)
                    .footer(default_footer()),
            ),
        )
        .await?;
    }

    Ok(())
}

// ── Buy / Sell ─────────────────────────────────────────────────────────────────

/// Buy a stock, ETF, or crypto
#[poise::command(slash_command)]
pub async fn buy(
    ctx: Context<'_>,
    #[description = "Ticker symbol (e.g. AAPL, BTC-USD)"] ticker_query: String,
    #[description = "Number of shares to buy (fractional ok)"] quantity: Option<f64>,
    #[description = "Dollar amount to spend (e.g. 200 to buy $200 worth)"] amount: Option<f64>,
    #[description = "Portfolio to buy into"] portfolio: String,
    #[description = "Limit price in USD — buy when price drops to or below this"] limit_price: Option<f64>,
) -> Result<(), Error> {
    // Validate: exactly one of quantity or amount must be provided
    if quantity.is_none() && amount.is_none() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description("Provide either **quantity** (shares) or **amount** (dollars), not neither.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }
    if quantity.is_some() && amount.is_some() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description("Provide either **quantity** (shares) or **amount** (dollars), not both.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let quote = match resolve_ticker(&ticker_query).await {
        Some(q) => q,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(market_data_err(&ticker_query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();

    // [TEST] market open guard disabled
    // if !quote.is_market_open() { ctx.send(market_closed_reply("Buy", &ticker)).await?; return Ok(()); }

    let price_usd = match quote.regular_market_price {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let asset_type = quote.asset_type();

    let price_per_unit = price_to_creds(price_usd);
    let quantity = quantity.unwrap_or_else(|| amount.unwrap() / price_usd);

    if quantity <= 0.0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description("Quantity must be greater than 0.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let total_cost = match amount {
        Some(amt) => price_to_creds(amt),
        None => price_per_unit * quantity,
    };

    // Queue if outside market hours OR limit price not yet met
    let should_queue = !is_market_hours()
        || limit_price.map(|lp| price_usd > lp).unwrap_or(false);

    if should_queue {
        ctx.defer().await?;

        let expiry = order_expiry();
        let reason = if !is_market_hours() {
            "Market is closed — order will execute at next open.".to_string()
        } else {
            format!(
                "Limit buy: current price **${:.2}** > limit **${:.2}**.",
                price_usd,
                limit_price.unwrap()
            )
        };
        let limit_str = limit_price.map(|lp| format!(" @ limit **${:.2}**", lp)).unwrap_or_default();
        let confirm_id = format!("buy_confirm_{}", ctx.author().id);
        let cancel_id  = format!("buy_cancel_{}", ctx.author().id);

        let reply = ctx.send(
            poise::CreateReply::default()
                .embed(
                    serenity::CreateEmbed::new()
                        .title("Queue Order?")
                        .description(format!(
                            "{}\n\nQueue **{} {}**{} in **{}**?\nExpires: <t:{}:f>",
                            reason, fmt_qty(quantity), ticker, limit_str, portfolio,
                            expiry.timestamp(),
                        ))
                        .color(data::EMBED_DEFAULT),
                )
                .components(vec![serenity::CreateActionRow::Buttons(vec![
                    serenity::CreateButton::new(&confirm_id)
                        .label("Queue")
                        .style(serenity::ButtonStyle::Primary),
                    serenity::CreateButton::new(&cancel_id)
                        .label("Cancel")
                        .style(serenity::ButtonStyle::Secondary),
                ])]),
        )
        .await?;

        let msg = reply.message().await?;
        let press = match msg
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(60))
            .await
        {
            Some(p) => p,
            None => {
                reply.edit(ctx, poise::CreateReply::default().components(vec![])).await?;
                return Ok(());
            }
        };

        press.defer(ctx.serenity_context()).await?;

        if press.data.custom_id == cancel_id {
            reply.edit(ctx, poise::CreateReply::default()
                .embed(serenity::CreateEmbed::new().title("Cancelled").color(data::EMBED_ERROR))
                .components(vec![]))
                .await?;
            return Ok(());
        }

        // Store the order
        {
            let data_ref = &ctx.data().users;
            let u = data_ref.get(&ctx.author().id).unwrap();
            let mut user_data = u.write().await;

            // Validate portfolio exists
            if !user_data.stock.portfolios.iter().any(|p| p.name.eq_ignore_ascii_case(&portfolio)) {
                reply.edit(ctx, poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new()
                        .title("Queue Failed")
                        .description(format!("No portfolio named **{}** found.", portfolio))
                        .color(data::EMBED_ERROR))
                    .components(vec![]))
                    .await?;
                return Ok(());
            }

            if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                reply.edit(ctx, poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new()
                        .title("Queue Failed")
                        .description(format!("You have reached the limit of **{}** pending orders. Cancel some in `/portfolio`.", MAX_PENDING_ORDERS))
                        .color(data::EMBED_ERROR))
                    .components(vec![]))
                    .await?;
                return Ok(());
            }

            let id = user_data.stock.next_order_id;
            user_data.stock.next_order_id = id.wrapping_add(1);
            user_data.stock.pending_orders.push(PendingOrder {
                id,
                side: OrderSide::Buy,
                ticker: ticker.clone(),
                asset_name: asset_name.clone(),
                asset_type,
                portfolio_name: portfolio.clone(),
                quantity,
                limit_price,
                expiry,
            });
        }

        let limit_tag = fmt_limit_tag(limit_price);
        reply.edit(ctx, poise::CreateReply::default()
            .embed(serenity::CreateEmbed::new()
                .title("Buy Order Queued")
                .description(format!(
                    "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                    fmt_qty(quantity), ticker, limit_tag, portfolio, quantity * price_usd, expiry.timestamp(),
                ))
                .color(data::EMBED_SUCCESS))
            .components(vec![]))
            .await?;
        return Ok(());
    }

    // Immediate execution path
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = match user_data.stock.portfolios.iter().position(|p| p.name.eq_ignore_ascii_case(&portfolio)) {
        Some(i) => i,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(format!("No portfolio named **{}** found.", portfolio))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    if user_data.stock.portfolios[port_idx].cash < total_cost {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description(format!(
                        "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                        creds_to_price(total_cost), total_cost, portfolio,
                        creds_to_price(user_data.stock.portfolios[port_idx].cash),
                        user_data.stock.portfolios[port_idx].cash
                    ))
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    {
        let stock = &mut user_data.stock;
        apply_buy(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &asset_name, asset_type, quantity, price_per_unit, total_cost, &portfolio);
    }
    drop(user_data);

    let embed = with_logo(
        serenity::CreateEmbed::new()
            .title("Buy")
            .description(format!(
                "Bought **{} {}** ({}) for **${:.2}** ({:.0} creds)\n${:.2}/unit | Portfolio: **{}**",
                fmt_qty(quantity), ticker, asset_name, creds_to_price(total_cost), total_cost, price_usd, portfolio
            ))
            .color(data::EMBED_SUCCESS)
            .footer(default_footer()),
        &ticker,
    );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Sell a stock, ETF, or crypto
#[poise::command(slash_command)]
pub async fn sell(
    ctx: Context<'_>,
    #[description = "Ticker symbol (e.g. AAPL, BTC-USD)"] ticker_query: String,
    #[description = "Portfolio to sell from"] portfolio: String,
    #[description = "Number of shares to sell (fractional ok)"] quantity: Option<f64>,
    #[description = "Dollar amount to sell (e.g. 200 to sell $200 worth)"] amount: Option<f64>,
    #[description = "Limit price in USD — sell when price rises to or above this"] limit_price: Option<f64>,
) -> Result<(), Error> {
    if quantity.is_some() && amount.is_some() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Sell")
                    .description("Provide either a **quantity** or a **dollar amount**, not both.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let quote = match resolve_ticker(&ticker_query).await {
        Some(q) => q,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(market_data_err(&ticker_query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();

    // [TEST] market open guard disabled
    // if !quote.is_market_open() { ctx.send(market_closed_reply("Sell", &ticker)).await?; return Ok(()); }

    let price_usd = match quote.regular_market_price {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let price_per_unit = price_to_creds(price_usd);

    // ── Phase 1: snapshot held quantity under read lock (no await) ──────────
    let (held, port_name_normalized) = {
        let data_ref = &ctx.data().users;
        let u = data_ref.get(&ctx.author().id).unwrap();
        let user_data = u.read().await;

        let port_idx = match user_data.stock.portfolios.iter().position(|p| p.name.eq_ignore_ascii_case(&portfolio)) {
            Some(i) => i,
            None => {
                drop(user_data);
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Sell")
                            .description(format!("No portfolio named **{}** found.", portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let pos = user_data.stock.portfolios[port_idx]
            .positions
            .iter()
            .find(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)));

        match pos {
            Some(p) => (p.quantity, user_data.stock.portfolios[port_idx].name.clone()),
            None => {
                drop(user_data);
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Sell")
                            .description(format!("No **{}** position in portfolio **{}**.", ticker, portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        }
    };

    let quantity = if let Some(q) = quantity {
        if (q - held).abs() < 5e-5 { held } else { q }
    } else if let Some(a) = amount {
        let raw = a / price_usd;
        if (raw - held).abs() < 5e-5 { held } else { raw }
    } else {
        held
    };

    if held < quantity - 1e-9 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Sell")
                    .description(format!(
                        "You only hold **{}** of **{}** but tried to sell **{}**.",
                        fmt_qty(held), ticker, fmt_qty(quantity)
                    ))
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    // Queue if outside market hours OR limit price not yet met
    let should_queue = !is_market_hours()
        || limit_price.map(|lp| price_usd < lp).unwrap_or(false);

    if should_queue {
        ctx.defer().await?;

        let expiry = order_expiry();
        let reason = if !is_market_hours() {
            "Market is closed — order will execute at next open.".to_string()
        } else {
            format!(
                "Limit sell: current price **${:.2}** < limit **${:.2}**.",
                price_usd,
                limit_price.unwrap()
            )
        };
        let limit_str = limit_price.map(|lp| format!(" @ limit **${:.2}**", lp)).unwrap_or_default();
        let confirm_id = format!("sell_confirm_{}", ctx.author().id);
        let cancel_id  = format!("sell_cancel_{}", ctx.author().id);

        let reply = ctx.send(
            poise::CreateReply::default()
                .embed(
                    serenity::CreateEmbed::new()
                        .title("Queue Order?")
                        .description(format!(
                            "{}\n\nQueue **{} {}**{} from **{}**?\nExpires: <t:{}:f>",
                            reason, fmt_qty(quantity), ticker, limit_str, port_name_normalized,
                            expiry.timestamp(),
                        ))
                        .color(data::EMBED_DEFAULT),
                )
                .components(vec![serenity::CreateActionRow::Buttons(vec![
                    serenity::CreateButton::new(&confirm_id)
                        .label("Queue")
                        .style(serenity::ButtonStyle::Primary),
                    serenity::CreateButton::new(&cancel_id)
                        .label("Cancel")
                        .style(serenity::ButtonStyle::Secondary),
                ])]),
        )
        .await?;

        let msg = reply.message().await?;
        let press = match msg
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(60))
            .await
        {
            Some(p) => p,
            None => {
                reply.edit(ctx, poise::CreateReply::default().components(vec![])).await?;
                return Ok(());
            }
        };

        press.defer(ctx.serenity_context()).await?;

        if press.data.custom_id == cancel_id {
            reply.edit(ctx, poise::CreateReply::default()
                .embed(serenity::CreateEmbed::new().title("Cancelled").color(data::EMBED_ERROR))
                .components(vec![]))
                .await?;
            return Ok(());
        }

        {
            let data_ref = &ctx.data().users;
            let u = data_ref.get(&ctx.author().id).unwrap();
            let mut user_data = u.write().await;

            if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                reply.edit(ctx, poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new()
                        .title("Queue Failed")
                        .description(format!("You have reached the limit of **{}** pending orders. Cancel some in `/portfolio`.", MAX_PENDING_ORDERS))
                        .color(data::EMBED_ERROR))
                    .components(vec![]))
                    .await?;
                return Ok(());
            }

            let id = user_data.stock.next_order_id;
            user_data.stock.next_order_id = id.wrapping_add(1);
            user_data.stock.pending_orders.push(PendingOrder {
                id,
                side: OrderSide::Sell,
                ticker: ticker.clone(),
                asset_name: asset_name.clone(),
                asset_type: AssetType::Stock,
                portfolio_name: port_name_normalized.clone(),
                quantity,
                limit_price,
                expiry,
            });
        }

        let limit_tag = fmt_limit_tag(limit_price);
        reply.edit(ctx, poise::CreateReply::default()
            .embed(serenity::CreateEmbed::new()
                .title("Sell Order Queued")
                .description(format!(
                    "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                    fmt_qty(quantity), ticker, limit_tag, port_name_normalized, quantity * price_usd, expiry.timestamp(),
                ))
                .color(data::EMBED_SUCCESS))
            .components(vec![]))
            .await?;
        return Ok(());
    }

    // ── Immediate execution path ─────────────────────────────────────────────
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = user_data.stock.portfolios.iter().position(|p| p.name == port_name_normalized).unwrap();

    let (proceeds, pnl) = {
        let stock = &mut user_data.stock;
        let pnl = apply_sell(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &asset_name, quantity, price_per_unit, &portfolio).unwrap_or(0.0);
        (price_per_unit * quantity, pnl)
    };

    let pnl_str = if pnl >= 0.0 {
        format!("▲ +${:.2} ({:.0} creds)", creds_to_price(pnl), pnl)
    } else {
        format!("▼ -${:.2} ({:.0} creds)", creds_to_price(pnl.abs()), pnl)
    };
    let color = if pnl >= 0.0 {
        data::EMBED_SUCCESS
    } else {
        data::EMBED_FAIL
    };
    drop(user_data);

    let embed = with_logo(
        serenity::CreateEmbed::new()
            .title("Sell")
            .description(format!(
                "Sold **{} {}** for **${:.2}** ({:.0} creds)\n${:.2}/unit | Realized P&L: **{}**",
                fmt_qty(quantity), ticker, creds_to_price(proceeds), proceeds, price_usd, pnl_str
            ))
            .color(color)
            .footer(default_footer()),
        &ticker,
    );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

