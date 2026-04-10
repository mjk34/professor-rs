//! /portfolio command — create, view, fund, withdraw, and delete portfolios.

use crate::api::{fetch_prices_map};
use crate::data::{self, AssetType, PendingOrder, Portfolio, BASE_HYSA_RATE};
use crate::helper::{creds_to_price, default_footer, fmt_qty, option_intrinsic, price_to_creds};
use crate::{serenity, Context, Error};
use std::collections::HashMap;
use std::time::Duration;

/// Compute total liquidation value (cash + all positions at current market prices) for a portfolio.
fn liquidation_value(port: &Portfolio, prices: &HashMap<String, f64>) -> f64 {
    let positions_value: f64 = port.positions.iter().map(|pos| {
        let price_usd = prices.get(&pos.ticker).copied().unwrap_or(0.0);
        match &pos.asset_type {
            AssetType::Option(contract) => {
                let intrinsic = option_intrinsic(&contract.option_type, price_usd, contract.strike);
                price_to_creds(intrinsic * 100.0) * f64::from(contract.contracts)
            }
            _ => price_to_creds(price_usd) * pos.quantity,
        }
    }).sum();
    port.cash + positions_value
}

pub const NUM_EMOJI: [&str; 4] = ["1️⃣", "2️⃣", "3️⃣", "4️⃣"];

// ── Portfolio modals ──────────────────────────────────────────────────────────

#[derive(Debug, poise::Modal)]
#[name = "Create Portfolio"]
pub struct CreatePortfolioModal {
    #[name = "Portfolio name (max 32 characters)"]
    #[placeholder = "e.g. Tech Stocks"]
    pub name: String,
}

#[derive(Debug, poise::Modal)]
#[name = "Delete Portfolio"]
pub struct DeletePortfolioModal {
    #[name = "Portfolio name"]
    #[placeholder = "Enter the exact portfolio name"]
    pub name: String,
}

/// Walks a modal's components and returns the `dollars` input text field, or empty.
fn read_dollars_field(data: &serenity::ModalInteractionData) -> String {
    for row in &data.components {
        for comp in &row.components {
            if let serenity::ActionRowComponent::InputText(t) = comp {
                if t.custom_id == "dollars" {
                    return t.value.clone().unwrap_or_default();
                }
            }
        }
    }
    String::new()
}

#[derive(Debug)]
pub struct FundModal {
    pub dollars: String,
    /// Read-only display: wallet balance available to deposit.
    pub available_info: String,
}

impl poise::Modal for FundModal {
    fn create(defaults: Option<Self>, custom_id: String) -> serenity::CreateInteractionResponse {
        let available = defaults.as_ref().map_or("", |d| d.available_info.as_str());
        let mut components = vec![];
        if !available.is_empty() {
            components.push(serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Total Available Cash for Depositing",
                    "available_info",
                )
                .value(available)
                .required(false),
            ));
        }
        components.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(
                serenity::InputTextStyle::Short,
                "Dollar amount to deposit",
                "dollars",
            )
            .placeholder("e.g. 100"),
        ));
        serenity::CreateInteractionResponse::Modal(
            serenity::CreateModal::new(custom_id, "Fund Portfolio").components(components),
        )
    }

    fn parse(data: serenity::ModalInteractionData) -> Result<Self, &'static str> {
        Ok(Self { dollars: read_dollars_field(&data), available_info: String::new() })
    }
}

#[derive(Debug)]
pub struct WithdrawModal {
    pub dollars: String,
    /// Read-only display: portfolio cash available to withdraw.
    pub available_info: String,
}

impl poise::Modal for WithdrawModal {
    fn create(defaults: Option<Self>, custom_id: String) -> serenity::CreateInteractionResponse {
        let available = defaults.as_ref().map_or("", |d| d.available_info.as_str());
        let mut components = vec![];
        if !available.is_empty() {
            components.push(serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short,
                    "Total Available Cash to Withdraw",
                    "available_info",
                )
                .value(available)
                .required(false),
            ));
        }
        components.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(
                serenity::InputTextStyle::Short,
                "Dollar amount to withdraw",
                "dollars",
            )
            .placeholder("e.g. 100"),
        ));
        serenity::CreateInteractionResponse::Modal(
            serenity::CreateModal::new(custom_id, "Withdraw from Portfolio").components(components),
        )
    }

    fn parse(data: serenity::ModalInteractionData) -> Result<Self, &'static str> {
        Ok(Self { dollars: read_dollars_field(&data), available_info: String::new() })
    }
}

// ── Embed builders ────────────────────────────────────────────────────────────

pub async fn build_portfolio_picker(
    portfolios: &[Portfolio],
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

    let mut buttons: Vec<serenity::CreateButton> = portfolios.iter().take(4).enumerate()
        .map(|(i, _)| serenity::CreateButton::new(format!("port_{i}"))
            .label(format!("{}", i + 1))
            .style(serenity::ButtonStyle::Primary))
        .collect();
    if !at_cap {
        buttons.push(serenity::CreateButton::new("port_create").label("Create").style(serenity::ButtonStyle::Success));
    }
    if !portfolios.is_empty() {
        buttons.push(serenity::CreateButton::new("port_delete").label("Delete").style(serenity::ButtonStyle::Danger));
    }

    let embed = serenity::CreateEmbed::new()
        .title("Portfolio — Select")
        .description(list)
        .color(data::EMBED_CYAN)
        .footer(default_footer());
    (embed, vec![serenity::CreateActionRow::Buttons(buttons)])
}

pub async fn build_portfolio_view_embed(
    portfolio: &Portfolio,
    pending_orders: &[PendingOrder],
    annual_rate: f64,
) -> serenity::CreateEmbed {
    let daily_accrual = (annual_rate / 100.0 / 365.0) * portfolio.cash;

    let unique_tickers: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        portfolio.positions.iter()
            .filter_map(|p| if seen.insert(p.ticker.as_str()) { Some(p.ticker.clone()) } else { None })
            .collect()
    };
    let price_cache = fetch_prices_map(&unique_tickers).await;
    let positions_value: f64 = portfolio.positions.iter()
        .map(|pos| price_to_creds(*price_cache.get(&pos.ticker).unwrap_or(&0.0)) * pos.quantity)
        .sum();
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
                let current_premium = crate::options::option_premium_creds(intrinsic, &contract.expiry, contract.contracts);
                let type_str = crate::helper::option_type_str(&contract.option_type);
                if contract.side == data::OptionSide::Short {
                    let pnl = cost_basis - current_premium;
                    desc += &format!(
                        "SHORT **{} {} ${:.2}** exp {} — {} contracts\nPremium rcvd: **${:.2}** | Obligation: **${:.2}** | P&L: **${:+.2}**{}\n\n",
                        pos.ticker, type_str, contract.strike,
                        contract.expiry.format("%Y-%m-%d"), contract.contracts,
                        creds_to_price(cost_basis), creds_to_price(current_premium),
                        creds_to_price(pnl), crate::helper::fmt_pct_change(pnl, cost_basis)
                    );
                } else {
                    let pnl = current_premium - cost_basis;
                    desc += &format!(
                        "**{} {} ${:.2}** exp {} — {} contracts\nCost: **${:.2}** | Value: **${:.2}** | P&L: **${:+.2}**{}\n\n",
                        pos.ticker, type_str, contract.strike,
                        contract.expiry.format("%Y-%m-%d"), contract.contracts,
                        creds_to_price(cost_basis), creds_to_price(current_premium),
                        creds_to_price(pnl), crate::helper::fmt_pct_change(pnl, cost_basis)
                    );
                }
            } else {
                let current_creds = price_to_creds(current_price_usd);
                let current_value = current_creds * pos.quantity;
                let pnl = current_value - cost_basis;
                let pnl_pct = if cost_basis > 0.0 { pnl / cost_basis * 100.0 } else { 0.0 };
                desc += &format!(
                    "**{}** × {} — Avg: ${:.2} | Now: ${:.2}\nValue: **${:.2}** ({:.0} creds) | P&L: **${:+.2}** ({:+.1}%)\n\n",
                    pos.ticker, fmt_qty(pos.quantity),
                    creds_to_price(pos.avg_cost), current_price_usd,
                    creds_to_price(current_value), current_value,
                    creds_to_price(pnl), pnl_pct
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
                fmt_qty(order.quantity), order.ticker,
                limit_tag, order.expiry.timestamp(),
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

// ── /portfolio command ────────────────────────────────────────────────────────

/// View and manage your investment portfolios
#[poise::command(slash_command)]
pub async fn portfolio(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let fed_rate_val = *ctx.data().hysa_fed_rate.read().await;
    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    let init_portfolios = { u.read().await.stock.portfolios.clone() };
    let (init_pe, init_pc) = build_portfolio_picker(&init_portfolios).await;
    let reply = ctx.send(poise::CreateReply::default().embed(init_pe).components(init_pc)).await?;

    'picker: loop {
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
                        .title("Portfolio — Create").description(err_msg).color(data::EMBED_ERROR),
                ).components(vec![serenity::CreateActionRow::Buttons(vec![
                    serenity::CreateButton::new("port_err_back").label("↩ Back").style(serenity::ButtonStyle::Secondary),
                ])])).await?;
                let _ = reply.message().await?
                    .await_component_interaction(ctx.serenity_context())
                    .author_id(ctx.author().id)
                    .timeout(Duration::from_secs(30))
                    .await;
                continue 'picker;
            }

            { let mut ud = u.write().await; ud.stock.portfolios.push(Portfolio::new(name.clone())); }

            let success_embed = serenity::CreateEmbed::new()
                .title("Portfolio — Create")
                .description(format!("Portfolio **{name}** created!"))
                .color(data::EMBED_SUCCESS).footer(default_footer());
            let create_btns = vec![serenity::CreateActionRow::Buttons(vec![
                serenity::CreateButton::new("new_port_back").label("↩ Back").style(serenity::ButtonStyle::Secondary),
                serenity::CreateButton::new("new_port_fund").label("Fund").style(serenity::ButtonStyle::Success),
            ])];
            reply.edit(ctx, poise::CreateReply::default().embed(success_embed.clone()).components(create_btns)).await?;

            let msg2 = reply.message().await?;
            let Some(press2) = msg2
                .await_component_interaction(ctx.serenity_context())
                .author_id(ctx.author().id)
                .timeout(Duration::from_secs(45))
                .await
            else {
                reply.edit(ctx, poise::CreateReply::default().embed(success_embed).components(vec![])).await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue 'picker;
            };

            if press2.data.custom_id == "new_port_back" {
                press2.defer(ctx.http()).await?;
                continue 'picker;
            }

            let wallet_dollars = {
                let ud = u.read().await;
                creds_to_price(f64::from(ud.get_creds()))
            };
            let Some(fund_modal) = poise::execute_modal_on_component_interaction::<FundModal>(
                ctx, press2,
                Some(FundModal { dollars: String::new(), available_info: format!("${wallet_dollars:.2}") }),
                Some(Duration::from_secs(30)),
            ).await? else { continue 'picker; };

            let dollars: f64 = match fund_modal.dollars.trim().trim_start_matches('$').replace(',', "").parse() {
                Ok(v) if v > 0.0 && v <= 100_000.0 => v,
                _ => {
                    reply.edit(ctx, poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Fund").description("Amount must be between $0.01 and $100,000.00.").color(data::EMBED_ERROR),
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
                        Some(p) => { p.cash += f64::from(amount); let new_cash = p.cash; user_data.sub_creds(amount); Ok(new_cash) }
                    }
                }
            };

            match fund_result {
                Err(msg) => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Fund").description(msg).color(data::EMBED_ERROR)).components(vec![])).await?; }
                Ok(new_cash) => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Fund").description(format!("Deposited **${:.2}** into **{}**.\nNew cash balance: **${:.2}**.", dollars, name, creds_to_price(new_cash))).color(data::EMBED_SUCCESS).footer(default_footer())).components(vec![])).await?; }
            }
            return Ok(());

        // ── Delete (from picker) ──────────────────────────────────────────────
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
                        serenity::CreateEmbed::new().title("Portfolio — Delete")
                            .description(format!("No portfolio named **{del_name}** found.")).color(data::EMBED_ERROR),
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

            reply.edit(ctx, poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Portfolio — Delete")
                    .description(format!("{detail}\n\nLiquidate all positions at market price and return cash to wallet?"))
                    .color(data::EMBED_FAIL).footer(default_footer()),
            ).components(vec![serenity::CreateActionRow::Buttons(vec![
                serenity::CreateButton::new("pdel_yes").label("Liquidate & Delete").style(serenity::ButtonStyle::Danger),
                serenity::CreateButton::new("pdel_no").label("Cancel").style(serenity::ButtonStyle::Secondary),
            ])])).await?;

            let conf = reply.message().await?
                .await_component_interaction(ctx.serenity_context())
                .author_id(ctx.author().id)
                .timeout(Duration::from_secs(45))
                .await;

            if let Some(c) = conf {
                c.defer(ctx.http()).await?;
                if c.data.custom_id == "pdel_yes" {
                    let port_clone = {
                        let ud = u.read().await;
                        ud.stock.portfolios.iter().find(|p| p.name.eq_ignore_ascii_case(&del_name)).cloned()
                    };
                    if let Some(port) = port_clone {
                        let tickers: Vec<String> = port.positions.iter().map(|p| p.ticker.clone()).collect();
                        let prices = fetch_prices_map(&tickers).await;
                        let total_proceeds = liquidation_value(&port, &prices);
                        let mut ud = u.write().await;
                        ud.stock.portfolios.retain(|p| !p.name.eq_ignore_ascii_case(&del_name));
                        ud.add_creds(total_proceeds as i32);
                    }
                }
            }

        // ── Numbered portfolio (view) ─────────────────────────────────────────
        } else {
            press.defer(ctx.http()).await?;
            let idx: usize = press.data.custom_id.strip_prefix("port_").and_then(|s| s.parse().ok()).unwrap_or(0);
            let port_name = {
                let ud = u.read().await;
                match ud.stock.portfolios.get(idx) { Some(p) => p.name.clone(), None => continue 'picker, }
            };

            'view: loop {
                let (port_opt, port_orders) = {
                    let ud = u.read().await;
                    let port = ud.stock.portfolios.iter().find(|p| p.name == port_name).cloned();
                    let orders: Vec<PendingOrder> = ud.stock.pending_orders.iter()
                        .filter(|o| o.portfolio_name == port_name).cloned().collect();
                    (port, orders)
                };
                let Some(port) = port_opt else { continue 'picker; };

                let annual_rate = {
                    let ud = u.read().await;
                    if crate::helper::is_gold(&ud) { crate::helper::gold_hysa_rate(fed_rate_val) } else { BASE_HYSA_RATE }
                };
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

                let Some(action) = reply.message().await?
                    .await_component_interaction(ctx.serenity_context())
                    .author_id(ctx.author().id)
                    .timeout(Duration::from_secs(45))
                    .await
                else {
                    reply.edit(ctx, poise::CreateReply::default().embed(embed).components(vec![])).await?;
                    return Ok(());
                };

                match action.data.custom_id.as_str() {
                    "pv_back" => { action.defer(ctx.http()).await?; continue 'picker; }

                    "pv_fund" => {
                        let wallet_dollars = {
                            let ud = u.read().await;
                            creds_to_price(f64::from(ud.get_creds()))
                        };
                        let Some(modal) = poise::execute_modal_on_component_interaction::<FundModal>(
                            ctx, action,
                            Some(FundModal { dollars: String::new(), available_info: format!("${wallet_dollars:.2}") }),
                            Some(Duration::from_secs(30)),
                        ).await? else { return Ok(()); };
                        let dollars: f64 = match modal.dollars.trim().trim_start_matches('$').replace(',', "").parse() {
                            Ok(v) if v > 0.0 && v <= 100_000.0 => v,
                            _ => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Fund").description("Amount must be between $0.01 and $100,000.00.").color(data::EMBED_ERROR)).components(vec![])).await?; return Ok(()); }
                        };
                        let amount = price_to_creds(dollars) as i32;
                        let fund_result: Result<f64, String> = {
                            let mut user_data = u.write().await;
                            if user_data.get_creds() < amount {
                                Err(format!("Insufficient wallet balance. You have **{:.0}** creds but need **{:.0}**.", user_data.get_creds(), amount))
                            } else {
                                match user_data.stock.portfolios.iter_mut().find(|p| p.name == port_name) {
                                    None => Err(format!("Portfolio **{port_name}** no longer exists.")),
                                    Some(p) => { p.cash += f64::from(amount); let new_cash = p.cash; user_data.sub_creds(amount); Ok(new_cash) }
                                }
                            }
                        };
                        match fund_result {
                            Err(msg) => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Fund").description(msg).color(data::EMBED_ERROR)).components(vec![])).await?; }
                            Ok(new_cash) => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Fund").description(format!("Deposited **${:.2}** into **{}**.\nNew cash balance: **${:.2}**.", dollars, port_name, creds_to_price(new_cash))).color(data::EMBED_SUCCESS).footer(default_footer())).components(vec![])).await?; }
                        }
                        return Ok(());
                    }

                    "pv_withdraw" => {
                        let port_cash_dollars = {
                            let ud = u.read().await;
                            ud.stock.portfolios.iter()
                                .find(|p| p.name == port_name)
                                .map_or(0.0, |p| creds_to_price(p.cash))
                        };
                        let Some(modal) = poise::execute_modal_on_component_interaction::<WithdrawModal>(
                            ctx, action,
                            Some(WithdrawModal { dollars: String::new(), available_info: format!("${port_cash_dollars:.2}") }),
                            Some(Duration::from_secs(30)),
                        ).await? else { return Ok(()); };
                        let dollars: f64 = match modal.dollars.trim().trim_start_matches('$').replace(',', "").parse() {
                            Ok(v) if v > 0.0 && v <= 100_000.0 => v,
                            _ => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Withdraw").description("Amount must be between $0.01 and $100,000.00.").color(data::EMBED_ERROR)).components(vec![])).await?; return Ok(()); }
                        };
                        let amount = price_to_creds(dollars) as i32;
                        let withdraw_result: Result<f64, String> = {
                            let mut user_data = u.write().await;
                            match user_data.stock.portfolios.iter_mut().find(|p| p.name == port_name) {
                                None => Err(format!("Portfolio **{port_name}** no longer exists.")),
                                Some(p) if p.cash < f64::from(amount) => Err(format!("Insufficient cash. **{}** has **${:.2}** but tried to withdraw **${:.2}**.", port_name, creds_to_price(p.cash), dollars)),
                                Some(p) => { p.cash -= f64::from(amount); let remaining = p.cash; user_data.add_creds(amount); Ok(remaining) }
                            }
                        };
                        match withdraw_result {
                            Err(msg) => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Withdraw").description(msg).color(data::EMBED_ERROR)).components(vec![])).await?; }
                            Ok(remaining) => { reply.edit(ctx, poise::CreateReply::default().embed(serenity::CreateEmbed::new().title("Portfolio — Withdraw").description(format!("Withdrew **${:.2}** from **{}** to your wallet.\nRemaining cash: **${:.2}**.", dollars, port_name, creds_to_price(remaining))).color(data::EMBED_SUCCESS).footer(default_footer())).components(vec![])).await?; }
                        }
                        return Ok(());
                    }

                    "pv_delete" => {
                        action.defer(ctx.http()).await?;
                        let port_info = { let ud = u.read().await; ud.stock.portfolios.iter().find(|p| p.name == port_name).map(|p| (p.cash > 0.0, !p.positions.is_empty(), p.cash, p.positions.len())) };
                        let (has_cash, has_positions, cash, positions_count) = match port_info { None => continue 'picker, Some(v) => v };

                        if !has_cash && !has_positions {
                            { let mut ud = u.write().await; ud.stock.portfolios.retain(|p| p.name != port_name); }
                            continue 'picker;
                        }

                        let detail = if has_cash && has_positions { format!("**{port_name}** has **{cash:.0}** creds cash and **{positions_count}** open positions.") }
                            else if has_cash { format!("**{port_name}** has **{cash:.0}** creds cash.") }
                            else { format!("**{port_name}** has **{positions_count}** open positions.") };

                        reply.edit(ctx, poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new().title("Portfolio — Delete")
                                .description(format!("{detail}\n\nLiquidate all positions at market price and return cash to wallet?"))
                                .color(data::EMBED_FAIL).footer(default_footer()),
                        ).components(vec![serenity::CreateActionRow::Buttons(vec![
                            serenity::CreateButton::new("del_yes").label("Liquidate & Delete").style(serenity::ButtonStyle::Danger),
                            serenity::CreateButton::new("del_no").label("Cancel").style(serenity::ButtonStyle::Secondary),
                        ])])).await?;

                        let conf = reply.message().await?
                            .await_component_interaction(ctx.serenity_context())
                            .author_id(ctx.author().id)
                            .timeout(Duration::from_secs(45))
                            .await;

                        match conf {
                            None => continue 'picker,
                            Some(c) => {
                                c.defer(ctx.http()).await?;
                                if c.data.custom_id == "del_yes" {
                                    let port_clone = {
                                        let ud = u.read().await;
                                        ud.stock.portfolios.iter().find(|p| p.name == port_name).cloned()
                                    };
                                    if let Some(port) = port_clone {
                                        let tickers: Vec<String> = port.positions.iter().map(|p| p.ticker.clone()).collect();
                                        let prices = fetch_prices_map(&tickers).await;
                                        let total_proceeds = liquidation_value(&port, &prices);
                                        let mut ud = u.write().await;
                                        ud.stock.portfolios.retain(|p| p.name != port_name);
                                        ud.add_creds(total_proceeds as i32);
                                    }
                                    continue 'picker;
                                }
                                continue 'view;
                            }
                        }
                    }

                    id if id.starts_with("pv_cancel_") => {
                        action.defer(ctx.http()).await?;
                        let order_id: u32 = id.strip_prefix("pv_cancel_").and_then(|s| s.parse().ok()).unwrap_or(u32::MAX);
                        { let mut ud = u.write().await; ud.stock.pending_orders.retain(|o| o.id != order_id); }
                        continue 'view;
                    }

                    _ => {}
                }
                break 'view;
            }
        }
    }
}
