//!---------------------------------------------------------------------!
//! Portfolio, watchlist, and trades commands                           !
//!---------------------------------------------------------------------!

use crate::api::{fetch_prices_map, fetch_quote_detail, market_data_err};
use crate::data::{self, AssetType, PendingOrder, Portfolio, Position, TradeAction, TradeRecord, TRADE_HISTORY_LIMIT};
use crate::helper::{creds_to_price, default_footer, fmt_qty, option_intrinsic, price_to_creds};
use crate::{serenity, Context, Error};
use chrono::Utc;
use poise::serenity_prelude::{futures, futures::StreamExt, EditMessage};
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

pub const NUM_EMOJI: [&str; 4] = ["1️⃣", "2️⃣", "3️⃣", "4️⃣"];

// ── Portfolio modals ──────────────────────────────────────────────────────────

#[derive(poise::Modal)]
#[name = "Create Portfolio"]
pub struct CreatePortfolioModal {
    #[name = "Portfolio name (max 32 characters)"]
    #[placeholder = "e.g. Tech Stocks"]
    pub name: String,
}

#[derive(poise::Modal)]
#[name = "Delete Portfolio"]
pub struct DeletePortfolioModal {
    #[name = "Portfolio name"]
    #[placeholder = "Enter the exact portfolio name"]
    pub name: String,
}

#[derive(poise::Modal)]
#[name = "Fund Portfolio"]
pub struct FundModal {
    #[name = "Dollar amount to deposit (e.g. $100)"]
    #[placeholder = "e.g. 100"]
    pub dollars: String,
}

#[derive(poise::Modal)]
#[name = "Withdraw from Portfolio"]
pub struct WithdrawModal {
    #[name = "Dollar amount to withdraw (e.g. $100)"]
    #[placeholder = "e.g. 100"]
    pub dollars: String,
}

// ── Trade helpers ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn apply_buy(
    port: &mut Portfolio,
    history: &mut VecDeque<TradeRecord>,
    ticker: &str,
    asset_name: &str,
    asset_type: AssetType,
    quantity: f64,
    price_per_unit: f64,
    total_cost_creds: f64,
    portfolio_name: &str,
) {
    port.cash -= total_cost_creds;

    if let Some(existing) = port.positions.iter_mut().find(|p| {
        p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_))
    }) {
        let total_qty = existing.quantity + quantity;
        existing.avg_cost =
            existing.avg_cost.mul_add(existing.quantity, total_cost_creds) / total_qty;
        existing.quantity = total_qty;
    } else {
        port.positions.push(Position {
            ticker: ticker.to_string(),
            asset_type,
            quantity,
            avg_cost: price_per_unit,
        });
    }

    let record = TradeRecord {
        portfolio: portfolio_name.to_string(),
        ticker: ticker.to_string(),
        asset_name: asset_name.to_string(),
        action: TradeAction::Buy,
        quantity,
        price_per_unit,
        total_creds: total_cost_creds,
        realized_pnl: None,
        timestamp: Utc::now(),
    };
    history.push_back(record);
    if history.len() > TRADE_HISTORY_LIMIT {
        history.pop_front();
    }
}

pub fn apply_sell(
    port: &mut Portfolio,
    history: &mut VecDeque<TradeRecord>,
    ticker: &str,
    asset_name: &str,
    quantity: f64,
    price_per_unit: f64,
    portfolio_name: &str,
) -> Option<f64> {
    let pos_idx = port
        .positions
        .iter()
        .position(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)))?;

    let avg_cost = port.positions[pos_idx].avg_cost;
    let proceeds = price_per_unit * quantity;
    let pnl = avg_cost.mul_add(-quantity, proceeds);

    port.cash += proceeds;
    port.positions[pos_idx].quantity -= quantity;
    if port.positions[pos_idx].quantity < 1e-9 { // epsilon: treat sub-nanoshare residuals as fully closed
        port.positions.remove(pos_idx);
    }

    let record = TradeRecord {
        portfolio: portfolio_name.to_string(),
        ticker: ticker.to_string(),
        asset_name: asset_name.to_string(),
        action: TradeAction::Sell,
        quantity,
        price_per_unit,
        total_creds: proceeds,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    };
    history.push_back(record);
    if history.len() > TRADE_HISTORY_LIMIT {
        history.pop_front();
    }
    Some(pnl)
}

// ── Portfolio picker / view helpers ──────────────────────────────────────────

pub async fn build_portfolio_picker(
    portfolios: &[data::Portfolio],
) -> (serenity::CreateEmbed, Vec<serenity::CreateActionRow>) {
    let at_cap = portfolios.len() >= 4;
    let list = if portfolios.is_empty() {
        "*No portfolios yet. Press **+ Create** to get started.*".to_string()
    } else {
        let unique_tickers: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            portfolios.iter().take(4).flat_map(|p| p.positions.iter())
                .filter_map(|pos| if seen.insert(pos.ticker.as_str()) { Some(pos.ticker.clone()) } else { None })
                .collect()
        };
        let prices = fetch_prices_map(&unique_tickers).await;

        let mut rows = Vec::new();
        for (i, p) in portfolios.iter().take(4).enumerate() {
            let mut positions_value: f64 = 0.0;
            let mut cost_basis: f64 = 0.0;
            for pos in &p.positions {
                positions_value += price_to_creds(*prices.get(&pos.ticker).unwrap_or(&0.0)) * pos.quantity;
                cost_basis += pos.avg_cost * pos.quantity;
            }
            let total = creds_to_price(p.cash + positions_value);
            let pnl_str = if cost_basis > 0.0 {
                format!("{:+.2}%", (positions_value - cost_basis) / cost_basis * 100.0)
            } else {
                "0.00%".to_string()
            };
            rows.push(format!(
                "{} **{}** — **${:.2}** | {} | {} positions",
                NUM_EMOJI[i], p.name, total, pnl_str, p.positions.len()
            ));
        }
        rows.join("\n")
    };
    let mut buttons: Vec<serenity::CreateButton> = portfolios
        .iter()
        .take(4)
        .enumerate()
        .map(|(i, _)| {
            serenity::CreateButton::new(format!("port_{i}"))
                .label(format!("{}", i + 1))
                .style(serenity::ButtonStyle::Primary)
        })
        .collect();
    if !at_cap {
        buttons.push(
            serenity::CreateButton::new("port_create")
                .label("Create")
                .style(serenity::ButtonStyle::Success),
        );
    }
    if !portfolios.is_empty() {
        buttons.push(
            serenity::CreateButton::new("port_delete")
                .label("Delete")
                .style(serenity::ButtonStyle::Danger),
        );
    }
    let components = vec![serenity::CreateActionRow::Buttons(buttons)];
    let embed = serenity::CreateEmbed::new()
        .title("Portfolio — Select")
        .description(list)
        .color(data::EMBED_CYAN)
        .footer(default_footer());
    (embed, components)
}

pub async fn build_portfolio_view_embed(portfolio: &data::Portfolio, pending_orders: &[PendingOrder], annual_rate: f64) -> serenity::CreateEmbed {
    let daily_accrual = (annual_rate / 100.0 / 365.0) * portfolio.cash;

    let unique_tickers: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        portfolio.positions.iter().filter_map(|p| if seen.insert(p.ticker.as_str()) { Some(p.ticker.clone()) } else { None }).collect()
    };
    let price_cache = fetch_prices_map(&unique_tickers).await;
    let mut positions_value: f64 = 0.0;
    for pos in &portfolio.positions {
        positions_value += price_to_creds(*price_cache.get(&pos.ticker).unwrap_or(&0.0)) * pos.quantity;
    }
    let total_value = portfolio.cash + positions_value;

    let mut desc = format!(
        "**Total Value:** ${:.2}\n**Cash:** ${:.2} | **Daily interest:** ~${:.2}\n\n",
        creds_to_price(total_value),
        creds_to_price(portfolio.cash),
        creds_to_price(daily_accrual)
    );

    if portfolio.positions.is_empty() {
        desc += "*No open positions.*";
    } else {
        desc += "**Positions:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";
        for pos in &portfolio.positions {
            let current_price_usd = price_cache.get(&pos.ticker).copied().unwrap_or(0.0);
            let cost_basis = pos.avg_cost * pos.quantity;

            if let AssetType::Option(contract) = &pos.asset_type {
                let intrinsic = option_intrinsic(&contract.option_type, current_price_usd, contract.strike);
                let current_premium =
                    crate::options::option_premium_creds(intrinsic, &contract.expiry, contract.contracts);
                let type_str = crate::helper::option_type_str(&contract.option_type);
                if contract.side == data::OptionSide::Short {
                    let pnl = cost_basis - current_premium;
                    desc += &format!(
                        "SHORT **{} {} ${:.2}** exp {} — {} contracts\nPremium rcvd: **${:.2}** | Obligation: **${:.2}** | P&L: **${:+.2}**{}\n\n",
                        pos.ticker, type_str, contract.strike,
                        contract.expiry.format("%Y-%m-%d"), contract.contracts,
                        creds_to_price(cost_basis),
                        creds_to_price(current_premium),
                        creds_to_price(pnl),
                        crate::helper::fmt_pct_change(pnl, cost_basis)
                    );
                } else {
                    let pnl = current_premium - cost_basis;
                    desc += &format!(
                        "**{} {} ${:.2}** exp {} — {} contracts\nCost: **${:.2}** | Value: **${:.2}** | P&L: **${:+.2}**{}\n\n",
                        pos.ticker, type_str, contract.strike,
                        contract.expiry.format("%Y-%m-%d"), contract.contracts,
                        creds_to_price(cost_basis),
                        creds_to_price(current_premium),
                        creds_to_price(pnl),
                        crate::helper::fmt_pct_change(pnl, cost_basis)
                    );
                }
            } else {
                let current_creds = price_to_creds(current_price_usd);
                let current_value = current_creds * pos.quantity;
                let pnl = current_value - cost_basis;
                let pnl_pct = if cost_basis > 0.0 {
                    pnl / cost_basis * 100.0
                } else {
                    0.0
                };
                desc += &format!(
                    "**{}** × {} — Avg: ${:.2} | Now: ${:.2}\nValue: **${:.2}** ({:.0} creds) | P&L: **${:+.2}** ({:+.1}%)\n\n",
                    pos.ticker,
                    fmt_qty(pos.quantity),
                    creds_to_price(pos.avg_cost),
                    current_price_usd,
                    creds_to_price(current_value),
                    current_value,
                    creds_to_price(pnl),
                    pnl_pct
                );
            }
        }
    }

    if !pending_orders.is_empty() {
        desc += "\n**Queued:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";
        for (i, order) in pending_orders.iter().take(5).enumerate() {
            let limit_tag = order.limit_price.map_or_else(|| "market".to_string(), |lp| format!("limit ${lp:.2}"));
            desc += &format!(
                "{} — {} {} {} | {} | expires: <t:{}:R>\n",
                data::NUMBER_EMOJS[(i + 1).min(9)],
                order.side.label().to_uppercase(),
                fmt_qty(order.quantity),
                order.ticker,
                limit_tag,
                order.expiry.timestamp(),
            );
        }
        desc += "\n*Use ❌1️⃣ ❌2️⃣ … below to cancel a queued order.*";
    }

    serenity::CreateEmbed::new()
        .title(format!("Portfolio — {}", portfolio.name))
        .description(desc)
        .color(data::EMBED_CYAN)
        .footer(default_footer())
}

// ── Portfolio command ─────────────────────────────────────────────────────────

/// View and manage your investment portfolios
#[poise::command(slash_command)]
pub async fn portfolio(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let fed_rate_val = *ctx.data().hysa_fed_rate.read().await;
    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    let annual_rate = {
        let user_data = u.read().await;
        if crate::helper::is_gold(&user_data) { crate::helper::gold_hysa_rate(fed_rate_val) } else { data::BASE_HYSA_RATE }
    };

    let init_portfolios = { u.read().await.stock.portfolios.clone() };
    let (init_pe, init_pc) = build_portfolio_picker(&init_portfolios).await;
    let reply = ctx.send(poise::CreateReply::default().embed(init_pe).components(init_pc)).await?;

    'picker: loop {
        // Refresh and show the picker on each iteration
        let portfolios = { u.read().await.stock.portfolios.clone() };
        let (pe, pc) = build_portfolio_picker(&portfolios).await;
        reply.edit(ctx, poise::CreateReply::default().embed(pe.clone()).components(pc)).await?;

        let msg = reply.message().await?;
        let Some(press) = msg
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(45))
            .await
        else {
            reply.edit(ctx, poise::CreateReply::default().embed(pe).components(vec![])).await?;
            return Ok(());
        };

        // ── Create ────────────────────────────────────────────────────────────
        if press.data.custom_id == "port_create" {
            let Some(modal) = poise::execute_modal_on_component_interaction::<CreatePortfolioModal>(
                ctx, press, None, Some(Duration::from_secs(30)),
            ).await? else { continue 'picker; };

            let name = modal.name.trim().to_string();

            let err = if name.is_empty() {
                Some("Portfolio name cannot be empty.".to_string())
            } else if name.len() > 32 {
                Some("Portfolio name must be 32 characters or fewer.".to_string())
            } else {
                let ud = u.read().await;
                if ud.stock.portfolios.len() >= 4 {
                    Some("You have reached the maximum of **4** portfolios.".to_string())
                } else if ud.stock.portfolios.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                    Some(format!("A portfolio named **{name}** already exists."))
                } else { None }
            };

            if let Some(err_msg) = err {
                reply.edit(ctx, poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Create")
                        .description(err_msg)
                        .color(data::EMBED_ERROR),
                ).components(vec![])).await?;
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue 'picker;
            }

            { let mut ud = u.write().await; ud.stock.portfolios.push(Portfolio::new(name.clone())); }

            let success_embed = serenity::CreateEmbed::new()
                .title("Portfolio — Create")
                .description(format!("Portfolio **{name}** created!"))
                .color(data::EMBED_SUCCESS)
                .footer(default_footer());
            let create_btns = vec![serenity::CreateActionRow::Buttons(vec![
                serenity::CreateButton::new("new_port_back").label("↩ Back").style(serenity::ButtonStyle::Secondary),
                serenity::CreateButton::new("new_port_fund").label("Fund").style(serenity::ButtonStyle::Success),
            ])];
            reply.edit(ctx, poise::CreateReply::default()
                .embed(success_embed.clone())
                .components(create_btns)
            ).await?;

            let msg2 = reply.message().await?;
            let Some(press2) = msg2
                .await_component_interaction(ctx.serenity_context())
                .author_id(ctx.author().id)
                .timeout(Duration::from_secs(45))
                .await
            else {
                // Timeout on Back/Fund — strip buttons, go back to picker
                reply.edit(ctx, poise::CreateReply::default().embed(success_embed).components(vec![])).await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue 'picker;
            };

            if press2.data.custom_id == "new_port_back" {
                press2.defer(ctx.http()).await?;
                continue 'picker;
            }

            // new_port_fund
            let Some(fund_modal) = poise::execute_modal_on_component_interaction::<FundModal>(
                ctx, press2, None, Some(Duration::from_secs(30)),
            ).await? else { continue 'picker; };

            let dollars: f64 = match fund_modal.dollars.trim().trim_start_matches('$').replace(',', "").parse() {
                Ok(v) if v > 0.0 && v <= 100_000.0 => v,
                _ => {
                    reply.edit(ctx, poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Fund")
                            .description("Amount must be between $0.01 and $100,000.00.")
                            .color(data::EMBED_ERROR),
                    ).components(vec![])).await?;
                    return Ok(());
                }
            };

            let amount = price_to_creds(dollars) as i32;
            let fund_result: Result<f64, String> = {
                let mut user_data = u.write().await;
                if user_data.get_creds() < amount {
                    Err(format!("Insufficient creds. You have **{}** but need **{}**.", user_data.get_creds(), amount))
                } else {
                    match user_data.stock.portfolios.iter_mut().find(|p| p.name == name) {
                        None => Err(format!("Portfolio **{name}** no longer exists.")),
                        Some(p) => {
                            p.cash += f64::from(amount);
                            let new_cash = p.cash;
                            user_data.sub_creds(amount);
                            Ok(new_cash)
                        }
                    }
                }
            };

            match fund_result {
                Err(msg) => {
                    reply.edit(ctx, poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Fund")
                            .description(msg)
                            .color(data::EMBED_ERROR),
                    ).components(vec![])).await?;
                }
                Ok(new_cash) => {
                    reply.edit(ctx, poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Fund")
                            .description(format!(
                                "Deposited **${:.2}** into **{}**.\nNew cash balance: **${:.2}**.",
                                dollars, name, creds_to_price(new_cash)
                            ))
                            .color(data::EMBED_SUCCESS)
                            .footer(default_footer()),
                    ).components(vec![])).await?;
                }
            }
            return Ok(());

        // ── Delete (from picker) ───────────────────────────────────────────────
        } else if press.data.custom_id == "port_delete" {
            let Some(del_modal) = poise::execute_modal_on_component_interaction::<DeletePortfolioModal>(
                ctx, press, None, Some(Duration::from_secs(30)),
            ).await? else { continue 'picker; };

            let del_name = del_modal.name.trim().to_string();

            let lookup = {
                let ud = u.read().await;
                ud.stock.portfolios.iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&del_name))
                    .map(|p| (p.cash > 0.0, !p.positions.is_empty(), p.cash, p.positions.len()))
            };

            let (has_cash, has_positions, cash, positions_count) = match lookup {
                None => {
                    reply.edit(ctx, poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Delete")
                            .description(format!("No portfolio named **{del_name}** found."))
                            .color(data::EMBED_ERROR),
                    ).components(vec![])).await?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue 'picker;
                }
                Some(v) => v,
            };

            if !has_cash && !has_positions {
                { let mut ud = u.write().await; ud.stock.portfolios.retain(|p| !p.name.eq_ignore_ascii_case(&del_name)); }
                continue 'picker;
            }

            let detail = if has_cash && has_positions {
                format!("**{del_name}** has **{cash:.0}** creds cash and **{positions_count}** open positions.")
            } else if has_cash {
                format!("**{del_name}** has **{cash:.0}** creds cash.")
            } else {
                format!("**{del_name}** has **{positions_count}** open positions.")
            };

            let confirm_btns = vec![serenity::CreateActionRow::Buttons(vec![
                serenity::CreateButton::new("pdel_yes").label("Liquidate & Delete").style(serenity::ButtonStyle::Danger),
                serenity::CreateButton::new("pdel_no").label("Cancel").style(serenity::ButtonStyle::Secondary),
            ])];
            reply.edit(ctx, poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Delete")
                    .description(format!("{detail}\n\nLiquidate all positions at market price and return cash to wallet?"))
                    .color(data::EMBED_FAIL)
                    .footer(default_footer()),
            ).components(confirm_btns)).await?;

            let msg2 = reply.message().await?;
            let conf = msg2
                .await_component_interaction(ctx.serenity_context())
                .author_id(ctx.author().id)
                .timeout(Duration::from_secs(45))
                .await;

            match conf {
                None => {}
                Some(c) => {
                    c.defer(ctx.http()).await?;
                    if c.data.custom_id != "pdel_yes" {
                        continue 'picker;
                    }
                    let (portfolio_cash, positions_for_fetch) = {
                        let ud = u.read().await;
                        match ud.stock.portfolios.iter().find(|p| p.name.eq_ignore_ascii_case(&del_name)) {
                            None => (0.0, vec![]),
                            Some(p) => (p.cash, p.positions.clone()),
                        }
                    };
                    let tickers_for_fetch: Vec<String> = positions_for_fetch.iter().map(|p| p.ticker.clone()).collect();
                    let prices = fetch_prices_map(&tickers_for_fetch).await;
                    let mut total_proceeds = portfolio_cash;
                    for pos in &positions_for_fetch {
                        let price_usd = prices.get(&pos.ticker).copied().unwrap_or(0.0);
                        let value = match &pos.asset_type {
                            AssetType::Option(contract) => {
                                let intrinsic = option_intrinsic(&contract.option_type, price_usd, contract.strike);
                                price_to_creds(intrinsic * 100.0) * f64::from(contract.contracts)
                            }
                            _ => price_to_creds(price_usd) * pos.quantity,
                        };
                        total_proceeds += value;
                    }
                    {
                        let mut ud = u.write().await;
                        ud.stock.portfolios.retain(|p| !p.name.eq_ignore_ascii_case(&del_name));
                        ud.add_creds(total_proceeds as i32);
                    }
                }
            }

        // ── Numbered portfolio ────────────────────────────────────────────────
        } else {
            press.defer(ctx.http()).await?;
            let idx: usize = press.data.custom_id
                .strip_prefix("port_")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let port_name = {
                let ud = u.read().await;
                match ud.stock.portfolios.get(idx) {
                    Some(p) => p.name.clone(),
                    None => continue 'picker,
                }
            };

            'view: loop {
                let (port_opt, port_orders) = {
                    let ud = u.read().await;
                    let port = ud.stock.portfolios.iter().find(|p| p.name == port_name).cloned();
                    let orders: Vec<PendingOrder> = ud.stock.pending_orders.iter()
                        .filter(|o| o.portfolio_name == port_name)
                        .cloned()
                        .collect();
                    (port, orders)
                };
                let Some(port) = port_opt else { continue 'picker; };

                let embed = build_portfolio_view_embed(&port, &port_orders, annual_rate).await;
                let mut view_btns = vec![
                    serenity::CreateButton::new("pv_back").label("↩ Back").style(serenity::ButtonStyle::Secondary),
                    serenity::CreateButton::new("pv_fund").label("Fund").style(serenity::ButtonStyle::Success),
                ];
                if port.cash > 0.0 {
                    view_btns.push(serenity::CreateButton::new("pv_withdraw").label("Withdraw").style(serenity::ButtonStyle::Primary));
                }
                view_btns.push(serenity::CreateButton::new("pv_delete").label("Delete").style(serenity::ButtonStyle::Danger));
                let mut action_buttons = vec![serenity::CreateActionRow::Buttons(view_btns)];
                if !port_orders.is_empty() {
                    let cancel_btns: Vec<serenity::CreateButton> = port_orders.iter().take(5).enumerate().map(|(i, o)| {
                        serenity::CreateButton::new(format!("pv_cancel_{}", o.id))
                            .label(format!("❌{}", data::NUMBER_EMOJS[(i + 1).min(9)]))
                            .style(serenity::ButtonStyle::Secondary)
                    }).collect();
                    action_buttons.push(serenity::CreateActionRow::Buttons(cancel_btns));
                }
                reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(action_buttons)).await?;

                let msg2 = reply.message().await?;
                let Some(action) = msg2
                    .await_component_interaction(ctx.serenity_context())
                    .author_id(ctx.author().id)
                    .timeout(Duration::from_secs(45))
                    .await
                else {
                    reply.edit(ctx, poise::CreateReply::default().embed(embed).components(vec![])).await?;
                    return Ok(());
                };

                match action.data.custom_id.as_str() {
                    "pv_back" => {
                        action.defer(ctx.http()).await?;
                        continue 'picker;
                    }

                    "pv_fund" => {
                        let Some(modal) = poise::execute_modal_on_component_interaction::<FundModal>(
                            ctx, action, None, Some(Duration::from_secs(30)),
                        ).await? else { return Ok(()); };

                        let dollars: f64 = match modal.dollars.trim().trim_start_matches('$').replace(',', "").parse() {
                            Ok(v) if v > 0.0 && v <= 100_000.0 => v,
                            _ => {
                                reply.edit(ctx, poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Fund")
                                        .description("Amount must be between $0.01 and $100,000.00.")
                                        .color(data::EMBED_ERROR),
                                ).components(vec![])).await?;
                                return Ok(());
                            }
                        };

                        let amount = price_to_creds(dollars) as i32;
                        let fund_result: Result<f64, String> = {
                            let mut user_data = u.write().await;
                            if user_data.get_creds() < amount {
                                Err(format!(
                                    "Insufficient wallet balance. You have **{:.0}** creds but need **{:.0}**.",
                                    user_data.get_creds(), amount
                                ))
                            } else {
                                match user_data.stock.portfolios.iter_mut().find(|p| p.name == port_name) {
                                    None => Err(format!("Portfolio **{port_name}** no longer exists.")),
                                    Some(p) => {
                                        p.cash += f64::from(amount);
                                        let new_cash = p.cash;
                                        user_data.sub_creds(amount);
                                        Ok(new_cash)
                                    }
                                }
                            }
                        };

                        match fund_result {
                            Err(msg) => {
                                reply.edit(ctx, poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Fund")
                                        .description(msg)
                                        .color(data::EMBED_ERROR),
                                ).components(vec![])).await?;
                            }
                            Ok(new_cash) => {
                                reply.edit(ctx, poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Fund")
                                        .description(format!(
                                            "Deposited **${:.2}** into **{}**.\nNew cash balance: **${:.2}**.",
                                            dollars, port_name, creds_to_price(new_cash)
                                        ))
                                        .color(data::EMBED_SUCCESS)
                                        .footer(default_footer()),
                                ).components(vec![])).await?;
                            }
                        }
                        return Ok(());
                    }

                    "pv_withdraw" => {
                        let Some(modal) = poise::execute_modal_on_component_interaction::<WithdrawModal>(
                            ctx, action, None, Some(Duration::from_secs(30)),
                        ).await? else { return Ok(()); };

                        let dollars: f64 = match modal.dollars.trim().trim_start_matches('$').replace(',', "").parse() {
                            Ok(v) if v > 0.0 && v <= 100_000.0 => v,
                            _ => {
                                reply.edit(ctx, poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Withdraw")
                                        .description("Amount must be between $0.01 and $100,000.00.")
                                        .color(data::EMBED_ERROR),
                                ).components(vec![])).await?;
                                return Ok(());
                            }
                        };

                        let amount = price_to_creds(dollars) as i32;
                        let withdraw_result: Result<f64, String> = {
                            let mut user_data = u.write().await;
                            match user_data.stock.portfolios.iter_mut().find(|p| p.name == port_name) {
                                None => Err(format!("Portfolio **{port_name}** no longer exists.")),
                                Some(p) if p.cash < f64::from(amount) => Err(format!(
                                    "Insufficient cash. **{}** has **${:.2}** but tried to withdraw **${:.2}**.",
                                    port_name, creds_to_price(p.cash), dollars
                                )),
                                Some(p) => {
                                    p.cash -= f64::from(amount);
                                    let remaining = p.cash;
                                    user_data.add_creds(amount);
                                    Ok(remaining)
                                }
                            }
                        };

                        match withdraw_result {
                            Err(msg) => {
                                reply.edit(ctx, poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Withdraw")
                                        .description(msg)
                                        .color(data::EMBED_ERROR),
                                ).components(vec![])).await?;
                            }
                            Ok(remaining) => {
                                reply.edit(ctx, poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Withdraw")
                                        .description(format!(
                                            "Withdrew **${:.2}** from **{}** to your wallet.\nRemaining cash: **${:.2}**.",
                                            dollars, port_name, creds_to_price(remaining)
                                        ))
                                        .color(data::EMBED_SUCCESS)
                                        .footer(default_footer()),
                                ).components(vec![])).await?;
                            }
                        }
                        return Ok(());
                    }

                    "pv_delete" => {
                        action.defer(ctx.http()).await?;

                        let port_info = {
                            let ud = u.read().await;
                            ud.stock.portfolios.iter()
                                .find(|p| p.name == port_name)
                                .map(|p| (p.cash > 0.0, !p.positions.is_empty(), p.cash, p.positions.len()))
                        };

                        let (has_cash, has_positions, cash, positions_count) = match port_info {
                            None => continue 'picker,
                            Some(v) => v,
                        };

                        if !has_cash && !has_positions {
                            { let mut ud = u.write().await; ud.stock.portfolios.retain(|p| p.name != port_name); }
                            continue 'picker;
                        }

                        let detail = if has_cash && has_positions {
                            format!("**{port_name}** has **{cash:.0}** creds cash and **{positions_count}** open positions.")
                        } else if has_cash {
                            format!("**{port_name}** has **{cash:.0}** creds cash.")
                        } else {
                            format!("**{port_name}** has **{positions_count}** open positions.")
                        };

                        let confirm_buttons = vec![serenity::CreateActionRow::Buttons(vec![
                            serenity::CreateButton::new("del_yes").label("Liquidate & Delete").style(serenity::ButtonStyle::Danger),
                            serenity::CreateButton::new("del_no").label("Cancel").style(serenity::ButtonStyle::Secondary),
                        ])];
                        reply.edit(ctx, poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Portfolio — Delete")
                                .description(format!("{detail}\n\nLiquidate all positions at market price and return cash to wallet?"))
                                .color(data::EMBED_FAIL)
                                .footer(default_footer()),
                        ).components(confirm_buttons)).await?;

                        let msg3 = reply.message().await?;
                        let conf = msg3
                            .await_component_interaction(ctx.serenity_context())
                            .author_id(ctx.author().id)
                            .timeout(Duration::from_secs(45))
                            .await;

                        match conf {
                            None => continue 'picker,
                            Some(c) => {
                                c.defer(ctx.http()).await?;
                                if c.data.custom_id == "del_yes" {
                                    let (portfolio_cash, positions_for_fetch) = {
                                        let ud = u.read().await;
                                        match ud.stock.portfolios.iter().find(|p| p.name == port_name) {
                                            None => (0.0, vec![]),
                                            Some(p) => (p.cash, p.positions.clone()),
                                        }
                                    };
                                    let tickers_for_fetch: Vec<String> = positions_for_fetch.iter().map(|p| p.ticker.clone()).collect();
                                    let prices = fetch_prices_map(&tickers_for_fetch).await;
                                    let mut total_proceeds = portfolio_cash;
                                    for pos in &positions_for_fetch {
                                        let price_usd = prices.get(&pos.ticker).copied().unwrap_or(0.0);
                                        let value = match &pos.asset_type {
                                            AssetType::Option(contract) => {
                                                let intrinsic = option_intrinsic(&contract.option_type, price_usd, contract.strike);
                                                price_to_creds(intrinsic * 100.0) * f64::from(contract.contracts)
                                            }
                                            _ => price_to_creds(price_usd) * pos.quantity,
                                        };
                                        total_proceeds += value;
                                    }
                                    {
                                        let mut ud = u.write().await;
                                        ud.stock.portfolios.retain(|p| p.name != port_name);
                                        ud.add_creds(total_proceeds as i32);
                                    }
                                    continue 'picker;
                                }
                                // Cancelled — stay in view
                                continue 'view;
                            }
                        }
                    }

                    id if id.starts_with("pv_cancel_") => {
                        action.defer(ctx.http()).await?;
                        let order_id: u32 = id.strip_prefix("pv_cancel_")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(u32::MAX);
                        {
                            let mut ud = u.write().await;
                            ud.stock.pending_orders.retain(|o| o.id != order_id);
                        }
                        continue 'view;
                    }

                    _ => {}
                }
                break 'view;
            } // end 'view loop
        } // end else (numbered portfolio)
    } // end 'picker loop
}

// ── Watchlist ─────────────────────────────────────────────────────────────────

#[derive(poise::Modal)]
#[name = "Add to Watchlist"]
pub struct WatchlistAddModal {
    #[name = "Ticker symbol (e.g. AAPL, BTC-USD)"]
    #[placeholder = "AAPL"]
    pub ticker: String,
}

#[derive(poise::Modal)]
#[name = "Remove from Watchlist"]
pub struct WatchlistRemoveModal {
    #[name = "Ticker symbol to remove"]
    #[placeholder = "AAPL"]
    pub ticker: String,
}

pub async fn build_watchlist_embed(
    tickers: &[String],
) -> (serenity::CreateEmbed, Vec<serenity::CreateActionRow>) {
    let description = if tickers.is_empty() {
        "*Your watchlist is empty. Press **Add** to track an asset.*".to_string()
    } else {
        let results = futures::future::join_all(
            tickers.iter().map(|t| { let t = t.clone(); async move { let r = fetch_quote_detail(&t).await; (t, r) } })
        ).await;

        let rows: Vec<String> = results.into_iter().map(|(ticker, quote)| {
            match quote {
                None => format!("`{ticker}` — fetch failed"),
                Some(q) => {
                    let price_usd = q.regular_market_price.unwrap_or(0.0);
                    let change_pct = q.regular_market_change_percent.unwrap_or(0.0);
                    let arrow = if change_pct >= 0.0 { "▲" } else { "▼" };
                    format!("**{}** — {} | ${:.2} | {} **{:.2}%**", ticker, q.display_name(), price_usd, arrow, change_pct.abs())
                }
            }
        }).collect();
        rows.join("\n")
    };

    let embed = serenity::CreateEmbed::new()
        .title("Watchlist")
        .description(description)
        .color(data::EMBED_CYAN)
        .footer(default_footer());

    let mut buttons = vec![
        serenity::CreateButton::new("wl_add")
            .label("Add")
            .style(serenity::ButtonStyle::Success),
    ];
    if !tickers.is_empty() {
        buttons.push(
            serenity::CreateButton::new("wl_remove")
                .label("Remove")
                .style(serenity::ButtonStyle::Primary),
        );
        buttons.push(
            serenity::CreateButton::new("wl_clear")
                .label("Clear")
                .style(serenity::ButtonStyle::Danger),
        );
    }

    let components = vec![serenity::CreateActionRow::Buttons(buttons)];
    (embed, components)
}

/// View and manage your watchlist
#[poise::command(slash_command)]
pub async fn watchlist(ctx: Context<'_>) -> Result<(), Error> {
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();

    let tickers = { u.read().await.stock.watchlist.clone() };
    let (mut embed, mut components) = build_watchlist_embed(&tickers).await;
    let reply = ctx.send(poise::CreateReply::default().embed(embed.clone()).components(components.clone())).await?;

    loop {
        let msg = reply.message().await?;
        let Some(press) = msg
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(60))
            .await
        else {
            reply.edit(ctx, poise::CreateReply::default().embed(embed).components(vec![])).await?;
            return Ok(());
        };

        match press.data.custom_id.as_str() {
            "wl_add" => {
                let Some(modal) = poise::execute_modal_on_component_interaction::<WatchlistAddModal>(
                    ctx, press, None, Some(Duration::from_secs(30)),
                ).await? else { continue; };

                let query = modal.ticker.trim().to_string();
                match crate::api::resolve_ticker(&query).await {
                    None => {
                        reply.edit(ctx, poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Watchlist — Add")
                                .description(market_data_err(&query))
                                .color(data::EMBED_ERROR),
                        ).components(vec![])).await?;
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                    Some(quote) => {
                        let ticker = quote.symbol.clone();
                        let result: Result<(), String> = {
                            let mut ud = u.write().await;
                            if ud.stock.watchlist.contains(&ticker) {
                                Err(format!("**{ticker}** is already on your watchlist."))
                            } else if ud.stock.watchlist.len() >= 20 {
                                Err("Watchlist is full (max 20 tickers).".to_string())
                            } else {
                                ud.stock.watchlist.push(ticker.clone());
                                Ok(())
                            }
                        };
                        if let Err(msg) = result {
                            reply.edit(ctx, poise::CreateReply::default().embed(
                                serenity::CreateEmbed::new()
                                    .title("Watchlist — Add")
                                    .description(msg)
                                    .color(data::EMBED_ERROR),
                            ).components(vec![])).await?;
                            tokio::time::sleep(Duration::from_secs(3)).await;
                        }
                    }
                }
            }

            "wl_remove" => {
                let Some(modal) = poise::execute_modal_on_component_interaction::<WatchlistRemoveModal>(
                    ctx, press, None, Some(Duration::from_secs(30)),
                ).await? else { continue; };

                let ticker = modal.ticker.trim().to_uppercase();
                let removed = {
                    let mut ud = u.write().await;
                    let before = ud.stock.watchlist.len();
                    ud.stock.watchlist.retain(|t| t != &ticker);
                    ud.stock.watchlist.len() < before
                };
                if !removed {
                    reply.edit(ctx, poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Watchlist — Remove")
                            .description(format!("**{ticker}** is not on your watchlist."))
                            .color(data::EMBED_ERROR),
                    ).components(vec![])).await?;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }

            "wl_clear" => {
                press.defer(ctx.http()).await?;
                { let mut ud = u.write().await; ud.stock.watchlist.clear(); }
            }

            _ => continue,
        }

        let tickers = { u.read().await.stock.watchlist.clone() };
        (embed, components) = build_watchlist_embed(&tickers).await;
        reply.edit(ctx, poise::CreateReply::default().embed(embed.clone()).components(components.clone())).await?;
    }
}

// ── Trade History ─────────────────────────────────────────────────────────────

pub fn build_summary_embed(trades: &VecDeque<TradeRecord>) -> serenity::CreateEmbed {
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

pub fn build_filtered_embed(trades: &VecDeque<TradeRecord>, gains_only: bool) -> serenity::CreateEmbed {
    let title = if gains_only {
        "Trade History — Gains"
    } else {
        "Trade History — Losses"
    };
    let filtered: Vec<_> = trades
        .iter()
        .filter(|t| {
            t.realized_pnl
                .is_some_and(|p| if gains_only { p > 0.0 } else { p < 0.0 })
        })
        .collect();

    if filtered.is_empty() {
        return serenity::CreateEmbed::new()
            .title(title)
            .description("No trades match this filter.")
            .color(if gains_only {
                data::EMBED_SUCCESS
            } else {
                data::EMBED_FAIL
            });
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
        .color(if gains_only {
            data::EMBED_SUCCESS
        } else {
            data::EMBED_FAIL
        })
        .footer(default_footer())
}

pub fn build_all_trades_embed(trades: &VecDeque<TradeRecord>) -> serenity::CreateEmbed {
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
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let trade_history = {
        let user_data = u.read().await;
        user_data.stock.trade_history.clone()
    };

    let embed = build_summary_embed(&trade_history);

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(embed)
                .components(trade_buttons()),
        )
        .await?;

    let mut msg = reply.into_message().await?;
    let ctx_serenity = ctx.serenity_context().clone();
    let author_id = ctx.author().id;

    tokio::spawn(async move {
        let mut interactions = msg
            .await_component_interactions(&ctx_serenity)
            .author_id(author_id)
            .stream();

        let mut last_embed = build_summary_embed(&trade_history);

        while let Ok(Some(interaction)) = tokio::time::timeout(Duration::from_secs(5 * 60), interactions.next()).await {
            let embed = match interaction.data.custom_id.as_str() {
                "trades-summary" => build_summary_embed(&trade_history),
                "trades-gains" => build_filtered_embed(&trade_history, true),
                "trades-losses" => build_filtered_embed(&trade_history, false),
                "trades-all" => build_all_trades_embed(&trade_history),
                _ => continue,
            };

            interaction
                .create_response(
                    &ctx_serenity,
                    serenity::CreateInteractionResponse::Acknowledge,
                )
                .await
                .ok();

            msg.edit(
                    &ctx_serenity,
                    EditMessage::default()
                        .embed(embed.clone())
                        .components(trade_buttons()),
                )
                .await
                .ok();

            last_embed = embed;
        }

        // timeout — strip buttons and grey out last active embed
        msg.edit(&ctx_serenity, EditMessage::default().embed(last_embed.color(data::EMBED_ERROR)).components(Vec::new()))
            .await
            .ok();
    });

    Ok(())
}
