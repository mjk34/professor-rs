//! /options_write and /options_cover — short-side (sell-to-open) options commands.

use super::engine::{find_option_idx, naked_margin_usd, option_premium_creds, parse_expiry, ERR_EXPIRY_PAST, ERR_INVALID_EXPIRY, ERR_INVALID_OPTION_TYPE, ERR_MIN_CONTRACTS, SHARES_PER_CONTRACT};
use crate::api::{fetch_price, market_data_err};
use crate::data::{self, AssetType, OptionContract, OptionSide, OptionType, TradeAction, TradeRecord, Position};
use crate::helper::{creds_to_price, default_footer, option_intrinsic, option_type_str, parse_option_type, price_to_creds};
use crate::{serenity, Context, Error};
use chrono::Utc;

/// Write (sell to open) a covered call or cash-secured put
#[poise::command(slash_command)]
pub async fn options_write(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "call or put"] option_type: String,
    #[description = "Number of contracts to write (1 contract = 100 shares)"] contracts: u32,
    #[description = "Portfolio to write from"] portfolio: String,
) -> Result<(), Error> {
    if contracts == 0 {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Write").description(ERR_MIN_CONTRACTS).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let opt_type = if let Some(t) = parse_option_type(&option_type) { t } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Write").description(ERR_INVALID_OPTION_TYPE).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let expiry_dt = if let Some(d) = parse_expiry(&expiry) { d } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Write").description(ERR_INVALID_EXPIRY).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    if expiry_dt < Utc::now() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Write").description(ERR_EXPIRY_PAST).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let ticker = ticker.to_uppercase();
    let price_usd = if let Some(p) = fetch_price(&ticker).await { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Write").description(market_data_err(&ticker)).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let premium = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let premium_per_contract = premium / f64::from(contracts);

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    {
        let port = if let Some(p) = user_data.stock.portfolios.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&portfolio)) { p } else {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Options Write").description(format!("No portfolio named **{portfolio}** found.")).color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        };

        let premium_usd = creds_to_price(premium);
        let mut collateral_locked = 0.0f64;

        match opt_type {
            OptionType::Call => {
                let shares_held = port.positions.iter()
                    .filter(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)))
                    .map(|p| p.quantity)
                    .sum::<f64>();
                let required = f64::from(contracts) * 100.0;
                if shares_held + 5e-5 < required {
                    let margin_usd = naked_margin_usd(&opt_type, price_usd, strike, contracts, premium_usd);
                    let margin_creds = price_to_creds(margin_usd);
                    let available = port.cash - port.locked_cash();
                    if available < margin_creds {
                        ctx.send(poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Options Write — Naked Call")
                                .description(format!(
                                    "Naked call requires **${:.2}** margin ({:.0} creds) in **{}** but only **${:.2}** ({:.0} creds) is free.\n\n*Alternatively, hold **{:.0} shares** of **{}** to write a covered call.*",
                                    margin_usd, margin_creds, portfolio,
                                    creds_to_price(available), available,
                                    required, ticker,
                                ))
                                .color(data::EMBED_ERROR),
                        )).await?;
                        return Ok(());
                    }
                    collateral_locked = margin_creds;
                }
            }
            OptionType::Put => {
                let required_cash = price_to_creds(strike * f64::from(contracts) * SHARES_PER_CONTRACT);
                let available = port.cash - port.locked_cash();
                if available < required_cash {
                    let margin_usd = naked_margin_usd(&opt_type, price_usd, strike, contracts, premium_usd);
                    let margin_creds = price_to_creds(margin_usd);
                    if available < margin_creds {
                        ctx.send(poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Options Write — Naked Put")
                                .description(format!(
                                    "Naked put requires **${:.2}** margin ({:.0} creds) in **{}** but only **${:.2}** ({:.0} creds) is free.\n\n*Alternatively, hold **${:.2}** ({:.0} creds) to write a cash-secured put.*",
                                    margin_usd, margin_creds, portfolio,
                                    creds_to_price(available), available,
                                    creds_to_price(required_cash), required_cash,
                                ))
                                .color(data::EMBED_ERROR),
                        )).await?;
                        return Ok(());
                    }
                    collateral_locked = margin_creds;
                }
            }
        }

        port.cash += premium;
        let existing_idx = find_option_idx(&port.positions, &ticker, strike, expiry_dt, &opt_type, &OptionSide::Short);

        if let Some(idx) = existing_idx {
            let pos = &mut port.positions[idx];
            let total_q = pos.quantity + f64::from(contracts);
            pos.avg_cost = pos.avg_cost.mul_add(pos.quantity, premium_per_contract * f64::from(contracts)) / total_q;
            pos.quantity = total_q;
            if let AssetType::Option(c) = &mut pos.asset_type {
                c.contracts += contracts;
                c.collateral += collateral_locked;
            }
        } else {
            port.positions.push(Position {
                ticker: ticker.clone(),
                asset_type: AssetType::Option(OptionContract {
                    strike,
                    expiry: expiry_dt,
                    option_type: opt_type.clone(),
                    contracts,
                    side: OptionSide::Short,
                    collateral: collateral_locked,
                }),
                quantity: f64::from(contracts),
                avg_cost: premium_per_contract,
            });
        }
    }

    let type_str = option_type_str(&opt_type);
    user_data.stock.push_trade(TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("SHORT {ticker} {type_str} ${strike:.2} {expiry}"),
        action: TradeAction::Sell,
        quantity: f64::from(contracts),
        price_per_unit: premium_per_contract,
        total_creds: premium,
        realized_pnl: None,
        timestamp: Utc::now(),
    });
    drop(user_data);

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Options Write")
            .description(format!(
                "Written **{}× {} {} ${:.2}** exp {}\nCollected **${:.2}** ({:.0} creds)",
                contracts, ticker, type_str, strike, expiry,
                creds_to_price(premium), premium
            ))
            .color(data::EMBED_CYAN)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

/// Cover (buy to close) a written options position
#[poise::command(slash_command)]
pub async fn options_cover(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "call or put"] option_type: String,
    #[description = "Number of contracts to cover"] contracts: u32,
    #[description = "Portfolio to cover from"] portfolio: String,
) -> Result<(), Error> {
    if contracts == 0 {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Cover").description(ERR_MIN_CONTRACTS).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let opt_type = if let Some(t) = parse_option_type(&option_type) { t } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Cover").description(ERR_INVALID_OPTION_TYPE).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let expiry_dt = if let Some(d) = parse_expiry(&expiry) { d } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Cover").description(ERR_INVALID_EXPIRY).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let ticker = ticker.to_uppercase();
    let price_usd = if let Some(p) = fetch_price(&ticker).await { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Cover").description(market_data_err(&ticker)).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let cost_to_close = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let cost_per_contract = cost_to_close / f64::from(contracts);

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let type_str = option_type_str(&opt_type);
    let (pnl, premium_received) = {
        let port = if let Some(p) = user_data.stock.portfolios.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&portfolio)) { p } else {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Options Cover").description(format!("No portfolio named **{portfolio}** found.")).color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        };

        let pos_idx = if let Some(i) = find_option_idx(&port.positions, &ticker, strike, expiry_dt, &opt_type, &OptionSide::Short) { i } else {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Cover")
                    .description(format!("No SHORT **{ticker} {type_str} ${strike:.2}** exp {expiry} in portfolio **{portfolio}**."))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        };

        let (held, collateral_total) = if let AssetType::Option(c) = &port.positions[pos_idx].asset_type {
            (c.contracts, c.collateral)
        } else {
            (0, 0.0)
        };

        if contracts > held {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Cover")
                    .description(format!("You only wrote **{held}** contracts but tried to cover **{contracts}**."))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        }

        let collateral_to_release = collateral_total * (f64::from(contracts) / f64::from(held));
        let available = port.cash - port.locked_cash() + collateral_to_release;

        if available < cost_to_close {
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Cover")
                    .description(format!(
                        "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** only has **${:.2}** ({:.0} creds) available.",
                        creds_to_price(cost_to_close), cost_to_close,
                        portfolio, creds_to_price(available), available,
                    ))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        }

        let avg_cost = port.positions[pos_idx].avg_cost;
        let premium_received = avg_cost * f64::from(contracts);
        let pnl = premium_received - cost_to_close;
        port.cash -= cost_to_close;

        if contracts == held {
            port.positions.remove(pos_idx);
        } else {
            let pos = &mut port.positions[pos_idx];
            pos.quantity -= f64::from(contracts);
            if let AssetType::Option(c) = &mut pos.asset_type {
                c.contracts -= contracts;
                c.collateral -= collateral_to_release;
            }
        }

        (pnl, premium_received)
    };

    let type_str = option_type_str(&opt_type);
    user_data.stock.push_trade(TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("SHORT {ticker} {type_str} ${strike:.2} {expiry}"),
        action: TradeAction::Buy,
        quantity: f64::from(contracts),
        price_per_unit: cost_per_contract,
        total_creds: cost_to_close,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    });

    let pnl_str = crate::helper::fmt_pnl(pnl);
    let color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
    drop(user_data);

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Options Cover")
            .description(format!(
                "Covered **{}× {} {} ${:.2}** exp {}\nCost to close: **${:.2}** | P&L: **{}**{}",
                contracts, ticker, type_str, strike, expiry,
                creds_to_price(cost_to_close), pnl_str,
                crate::helper::fmt_pct_change(pnl, premium_received)
            ))
            .color(color)
            .footer(default_footer()),
    )).await?;
    Ok(())
}
