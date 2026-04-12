//! `/options_buy` and `/options_sell` — long-side options commands.

use super::engine::{find_option_idx, option_premium_creds, parse_expiry, ERR_EXPIRY_PAST, ERR_INVALID_EXPIRY, ERR_MIN_CONTRACTS};
use crate::api::{fetch_price, market_data_err};
use crate::data::{self, AssetType, OptionContract, OptionSide, OptionType, TradeAction, TradeRecord, Position};
use crate::helper::{creds_to_price, default_footer, option_intrinsic, option_type_str};
use crate::{serenity, Context, Error};
use chrono::Utc;

/// Buy an options contract
#[poise::command(slash_command)]
pub async fn options_buy(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "Call or Put"] option_type: OptionType,
    #[description = "Number of contracts (1 contract = 100 shares)"] contracts: u32,
    #[description = "Portfolio to buy from"] portfolio: String,
) -> Result<(), Error> {
    if contracts == 0 {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Buy").description(ERR_MIN_CONTRACTS).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let opt_type = option_type;
    let expiry_dt = if let Some(d) = parse_expiry(&expiry) { d } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Buy").description(ERR_INVALID_EXPIRY).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    if expiry_dt < Utc::now() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Buy").description(ERR_EXPIRY_PAST).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let ticker = ticker.to_uppercase();
    let price_usd = if let Some(p) = fetch_price(&ticker).await { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Buy").description(market_data_err(&ticker)).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let intrinsic = option_intrinsic(opt_type, price_usd, strike);
    let total_cost = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let cost_per_contract = total_cost / f64::from(contracts);

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = match user_data.stock.find_portfolio_idx(&portfolio) {
        Some(i) => i,
        None => {
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Options Buy").description(format!("No portfolio named **{portfolio}** found.")).color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        }
    };

    if user_data.stock.portfolios[port_idx].cash < total_cost {
        let cash = user_data.stock.portfolios[port_idx].cash;
        drop(user_data);
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Buy")
                .description(format!(
                    "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                    creds_to_price(total_cost), total_cost, portfolio, creds_to_price(cash), cash
                ))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    {
        let port = &mut user_data.stock.portfolios[port_idx];
        port.cash -= total_cost;
        let quantity = f64::from(contracts);
        let existing_idx = find_option_idx(&port.positions, &ticker, strike, expiry_dt, opt_type, &OptionSide::Long);

        if let Some(idx) = existing_idx {
            let pos = &mut port.positions[idx];
            let total_q = pos.quantity + quantity;
            pos.avg_cost = pos.avg_cost.mul_add(pos.quantity, cost_per_contract * quantity) / total_q;
            pos.quantity = total_q;
            if let AssetType::Option(c) = &mut pos.asset_type {
                c.contracts += contracts;
            }
        } else {
            port.positions.push(Position {
                ticker: ticker.clone(),
                asset_type: AssetType::Option(OptionContract {
                    strike,
                    expiry: expiry_dt,
                    option_type: opt_type,
                    contracts,
                    side: OptionSide::Long,
                    collateral: 0.0,
                }),
                quantity,
                avg_cost: cost_per_contract,
            });
        }
    }

    let type_str = option_type_str(opt_type);
    user_data.stock.push_trade(TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("{ticker} {type_str} ${strike:.2} {expiry}"),
        action: TradeAction::Buy,
        quantity: f64::from(contracts),
        price_per_unit: cost_per_contract,
        total_creds: total_cost,
        realized_pnl: None,
        timestamp: Utc::now(),
    });
    drop(user_data);

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Options Buy")
            .description(format!(
                "Bought **{} {} ${:.2}** exp {} — {} contracts\nCost: **${:.2}** ({:.0} creds)",
                ticker, type_str, strike, expiry, contracts, creds_to_price(total_cost), total_cost
            ))
            .color(data::EMBED_SUCCESS)
            .footer(default_footer()),
    )).await?;
    Ok(())
}

/// Sell (close) an options position
#[poise::command(slash_command)]
pub async fn options_sell(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "Call or Put"] option_type: OptionType,
    #[description = "Number of contracts to sell"] contracts: u32,
    #[description = "Portfolio to sell from"] portfolio: String,
) -> Result<(), Error> {
    if contracts == 0 {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Sell").description(ERR_MIN_CONTRACTS).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let opt_type = option_type;
    let expiry_dt = if let Some(d) = parse_expiry(&expiry) { d } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Sell").description(ERR_INVALID_EXPIRY).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let ticker = ticker.to_uppercase();
    let price_usd = if let Some(p) = fetch_price(&ticker).await { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Sell").description(market_data_err(&ticker)).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let intrinsic = option_intrinsic(opt_type, price_usd, strike);
    let total_proceeds = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let proceeds_per_contract = total_proceeds / f64::from(contracts);

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = match user_data.stock.find_portfolio_idx(&portfolio) {
        Some(i) => i,
        None => {
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Options Sell").description(format!("No portfolio named **{portfolio}** found.")).color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        }
    };

    let pos_idx = match find_option_idx(&user_data.stock.portfolios[port_idx].positions, &ticker, strike, expiry_dt, opt_type, &OptionSide::Long) {
        Some(i) => i,
        None => {
            let type_str = option_type_str(opt_type);
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Sell")
                    .description(format!("No **{ticker} {type_str} ${strike:.2}** exp {expiry} in portfolio **{portfolio}**."))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        }
    };

    let held = if let AssetType::Option(c) = &user_data.stock.portfolios[port_idx].positions[pos_idx].asset_type { c.contracts } else { 0 };

    if contracts > held {
        drop(user_data);
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Sell")
                .description(format!("You only hold **{held}** contracts but tried to sell **{contracts}**."))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let avg_cost = user_data.stock.portfolios[port_idx].positions[pos_idx].avg_cost;
    let pnl = avg_cost.mul_add(-f64::from(contracts), total_proceeds);

    {
        let port = &mut user_data.stock.portfolios[port_idx];
        port.cash += total_proceeds;

        if contracts == held {
            port.positions.remove(pos_idx);
        } else {
            let pos = &mut port.positions[pos_idx];
            pos.quantity -= f64::from(contracts);
            if let AssetType::Option(c) = &mut pos.asset_type {
                c.contracts -= contracts;
            }
        }
    }

    let type_str = option_type_str(opt_type);
    user_data.stock.push_trade(TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("{ticker} {type_str} ${strike:.2} {expiry}"),
        action: TradeAction::Sell,
        quantity: f64::from(contracts),
        price_per_unit: proceeds_per_contract,
        total_creds: total_proceeds,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    });

    let pnl_str = crate::helper::fmt_pnl(pnl);
    let color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
    drop(user_data);

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Options Sell")
            .description(format!(
                "Sold **{} {} ${:.2}** exp {} — {} contracts\nProceeds: **${:.2}** | P&L: **{}**",
                ticker, type_str, strike, expiry, contracts, creds_to_price(total_proceeds), pnl_str
            ))
            .color(color)
            .footer(default_footer()),
    )).await?;
    Ok(())
}
