//!---------------------------------------------------------------------!
//! Options trading commands                                            !
//!---------------------------------------------------------------------!

use crate::api::{fetch_price, market_data_err};
use crate::data::{self, AssetType, OptionContract, OptionSide, OptionType, Position, TradeAction, TradeRecord, TRADE_HISTORY_LIMIT};
use crate::helper::{creds_to_price, default_footer, option_intrinsic, option_type_str, parse_option_type, price_to_creds};
use crate::{serenity, Context, Error};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};

const SHARES_PER_CONTRACT: f64 = 100.0;
const TIME_VALUE_PER_DTE: f64 = 0.05;
const CALL_MARGIN_RATIO: f64 = 0.20;
const PUT_MARGIN_RATIO: f64 = 0.10;

const ERR_INVALID_OPTION_TYPE: &str = "Option type must be `call` or `put`.";
const ERR_INVALID_EXPIRY: &str = "Invalid expiry date. Use YYYY-MM-DD format.";
const ERR_EXPIRY_PAST: &str = "Expiry date is in the past.";
const ERR_MIN_CONTRACTS: &str = "Contracts must be at least 1.";

pub fn option_premium_creds(intrinsic_usd: f64, expiry: &DateTime<Utc>, contracts: u32) -> f64 {
    let dte = (*expiry - Utc::now()).num_days().max(0) as f64;
    let per_contract_usd = (intrinsic_usd + dte * TIME_VALUE_PER_DTE).max(0.01);
    price_to_creds(per_contract_usd * contracts as f64 * SHARES_PER_CONTRACT)
}

pub fn naked_margin_usd(opt_type: &OptionType, price_usd: f64, strike: f64, contracts: u32, premium_usd: f64) -> f64 {
    let notional = SHARES_PER_CONTRACT * contracts as f64;
    let otm_usd = match opt_type {
        OptionType::Call => (strike - price_usd).max(0.0),
        OptionType::Put  => (price_usd - strike).max(0.0),
    } * notional;
    let min_basis = match opt_type {
        OptionType::Call => price_usd,
        OptionType::Put  => strike,
    };
    f64::max(
        CALL_MARGIN_RATIO * price_usd * notional + premium_usd - otm_usd,
        PUT_MARGIN_RATIO * min_basis * notional + premium_usd,
    )
}

pub fn find_option_idx(
    positions: &[Position],
    ticker: &str,
    strike: f64,
    expiry: DateTime<Utc>,
    opt_type: &OptionType,
    side: &OptionSide,
) -> Option<usize> {
    positions.iter().position(|p| {
        if p.ticker != ticker {
            return false;
        }
        if let AssetType::Option(c) = &p.asset_type {
            c.strike == strike && c.expiry == expiry && c.option_type == *opt_type && c.side == *side
        } else {
            false
        }
    })
}

pub fn parse_expiry(date_str: &str) -> Option<chrono::DateTime<Utc>> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .map(|dt| Utc.from_utc_datetime(&dt))
}

// ── Options commands ──────────────────────────────────────────────────────────

/// Get the intrinsic value of an options contract
#[poise::command(slash_command)]
pub async fn options_quote(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "call or put"] option_type: String,
) -> Result<(), Error> {
    let opt_type = match parse_option_type(&option_type) {
        Some(t) => t,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Quote")
                        .description(ERR_INVALID_OPTION_TYPE)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let expiry_dt = match parse_expiry(&expiry) {
        Some(d) => d,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Quote")
                        .description(ERR_INVALID_EXPIRY)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    if expiry_dt < Utc::now() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Quote")
                    .description(ERR_EXPIRY_PAST)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let ticker = ticker.to_uppercase();
    let price_usd = match fetch_price(&ticker).await {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Quote")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let dte = (expiry_dt - Utc::now()).num_days().max(0);
    let time_value_usd = dte as f64 * 0.05;
    let premium_per_contract_usd = (intrinsic + time_value_usd).max(0.01) * 100.0;
    let premium_creds = price_to_creds(premium_per_contract_usd);
    let itm = intrinsic > 0.0;
    let type_str = option_type_str(&opt_type);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Quote")
                .description(format!(
                    "**{} {} ${:.2}** exp {} ({} DTE)\n\nUnderlying: **${:.2}**\nIntrinsic: **${:.2}/contract** | Time value: **${:.2}/contract**\nPremium: **${:.2}/contract** ({:.0} creds)\nStatus: **{}**",
                    ticker, type_str, strike, expiry, dte,
                    price_usd,
                    intrinsic * SHARES_PER_CONTRACT, time_value_usd * SHARES_PER_CONTRACT,
                    premium_per_contract_usd, premium_creds,
                    if itm { "In The Money (ITM)" } else { "Out of The Money (OTM)" }
                ))
                .color(if itm { data::EMBED_SUCCESS } else { data::EMBED_ERROR })
                .footer(default_footer()),
        ),
    )
    .await?;
    Ok(())
}

/// Buy an options contract
#[poise::command(slash_command)]
pub async fn options_buy(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "call or put"] option_type: String,
    #[description = "Number of contracts (1 contract = 100 shares)"] contracts: u32,
    #[description = "Portfolio to buy from"] portfolio: String,
) -> Result<(), Error> {
    if contracts == 0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Buy")
                    .description(ERR_MIN_CONTRACTS)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let opt_type = match parse_option_type(&option_type) {
        Some(t) => t,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Buy")
                        .description(ERR_INVALID_OPTION_TYPE)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let expiry_dt = match parse_expiry(&expiry) {
        Some(d) => d,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Buy")
                        .description(ERR_INVALID_EXPIRY)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    if expiry_dt < Utc::now() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Buy")
                    .description(ERR_EXPIRY_PAST)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let ticker = ticker.to_uppercase();
    let price_usd = match fetch_price(&ticker).await {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Buy")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let total_cost = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let cost_per_contract = total_cost / contracts as f64;

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    {
        let port = match user_data
            .stock
            .portfolios
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&portfolio))
        {
            Some(p) => p,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Options Buy")
                            .description(format!("No portfolio named **{}** found.", portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        if port.cash < total_cost {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Buy")
                        .description(format!(
                            "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                            creds_to_price(total_cost), total_cost, portfolio, creds_to_price(port.cash), port.cash
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        port.cash -= total_cost;
        let quantity = contracts as f64;

        let existing_idx =
            find_option_idx(&port.positions, &ticker, strike, expiry_dt, &opt_type, &OptionSide::Long);

        if let Some(idx) = existing_idx {
            let pos = &mut port.positions[idx];
            let total_q = pos.quantity + quantity;
            pos.avg_cost =
                (pos.avg_cost * pos.quantity + cost_per_contract * quantity) / total_q;
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
                    option_type: opt_type.clone(),
                    contracts,
                    side: OptionSide::Long,
                    collateral: 0.0,
                }),
                quantity,
                avg_cost: cost_per_contract,
            });
        }
    }

    let type_str = option_type_str(&opt_type);
    let record = TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("{} {} ${:.2} {}", ticker, type_str, strike, expiry),
        action: TradeAction::Buy,
        quantity: contracts as f64,
        price_per_unit: cost_per_contract,
        total_creds: total_cost,
        realized_pnl: None,
        timestamp: Utc::now(),
    };
    user_data.stock.trade_history.push_back(record);
    if user_data.stock.trade_history.len() > TRADE_HISTORY_LIMIT {
        user_data.stock.trade_history.pop_front();
    }
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Buy")
                .description(format!(
                    "Bought **{} {} ${:.2}** exp {} — {} contracts\nCost: **${:.2}** ({:.0} creds)",
                    ticker, type_str, strike, expiry, contracts,
                    creds_to_price(total_cost), total_cost
                ))
                .color(data::EMBED_SUCCESS)
                .footer(default_footer()),
        ),
    )
    .await?;
    Ok(())
}

/// Sell (close) an options position
#[poise::command(slash_command)]
pub async fn options_sell(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "call or put"] option_type: String,
    #[description = "Number of contracts to sell"] contracts: u32,
    #[description = "Portfolio to sell from"] portfolio: String,
) -> Result<(), Error> {
    if contracts == 0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Sell")
                    .description(ERR_MIN_CONTRACTS)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let opt_type = match parse_option_type(&option_type) {
        Some(t) => t,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Sell")
                        .description(ERR_INVALID_OPTION_TYPE)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let expiry_dt = match parse_expiry(&expiry) {
        Some(d) => d,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Sell")
                        .description(ERR_INVALID_EXPIRY)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let ticker = ticker.to_uppercase();
    let price_usd = match fetch_price(&ticker).await {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Sell")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let total_proceeds = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let proceeds_per_contract = total_proceeds / contracts as f64;

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let pnl = {
        let port = match user_data
            .stock
            .portfolios
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&portfolio))
        {
            Some(p) => p,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Options Sell")
                            .description(format!("No portfolio named **{}** found.", portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let pos_idx = match find_option_idx(&port.positions, &ticker, strike, expiry_dt, &opt_type, &OptionSide::Long) {
            Some(i) => i,
            None => {
                let type_str = option_type_str(&opt_type);
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Options Sell")
                            .description(format!(
                                "No **{} {} ${:.2}** exp {} in portfolio **{}**.",
                                ticker, type_str, strike, expiry, portfolio
                            ))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let held = if let AssetType::Option(c) = &port.positions[pos_idx].asset_type {
            c.contracts
        } else {
            0
        };

        if contracts > held {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Sell")
                        .description(format!(
                            "You only hold **{}** contracts but tried to sell **{}**.",
                            held, contracts
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        let avg_cost = port.positions[pos_idx].avg_cost;
        let pnl = total_proceeds - avg_cost * contracts as f64;
        port.cash += total_proceeds;

        if contracts == held {
            port.positions.remove(pos_idx);
        } else {
            let pos = &mut port.positions[pos_idx];
            pos.quantity -= contracts as f64;
            if let AssetType::Option(c) = &mut pos.asset_type {
                c.contracts -= contracts;
            }
        }

        pnl
    };

    let type_str = option_type_str(&opt_type);
    let record = TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("{} {} ${:.2} {}", ticker, type_str, strike, expiry),
        action: TradeAction::Sell,
        quantity: contracts as f64,
        price_per_unit: proceeds_per_contract,
        total_creds: total_proceeds,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    };
    user_data.stock.trade_history.push_back(record);
    if user_data.stock.trade_history.len() > TRADE_HISTORY_LIMIT {
        user_data.stock.trade_history.pop_front();
    }

    let pnl_str = crate::helper::fmt_pnl(pnl);
    let color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Sell")
                .description(format!(
                    "Sold **{} {} ${:.2}** exp {} — {} contracts\nProceeds: **${:.2}** | P&L: **{}**",
                    ticker, type_str, strike, expiry, contracts, creds_to_price(total_proceeds), pnl_str
                ))
                .color(color)
                .footer(default_footer()),
        ),
    )
    .await?;

    Ok(())
}

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
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Write")
                    .description(ERR_MIN_CONTRACTS)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let opt_type = match parse_option_type(&option_type) {
        Some(t) => t,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Write")
                        .description(ERR_INVALID_OPTION_TYPE)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let expiry_dt = match parse_expiry(&expiry) {
        Some(d) => d,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Write")
                        .description(ERR_INVALID_EXPIRY)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    if expiry_dt < Utc::now() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Write")
                    .description(ERR_EXPIRY_PAST)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let ticker = ticker.to_uppercase();
    let price_usd = match fetch_price(&ticker).await {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Write")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let premium = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let premium_per_contract = premium / contracts as f64;

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    {
        let port = match user_data
            .stock
            .portfolios
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&portfolio))
        {
            Some(p) => p,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Options Write")
                            .description(format!("No portfolio named **{}** found.", portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let premium_usd = creds_to_price(premium);
        let mut collateral_locked = 0.0f64;

        match opt_type {
            OptionType::Call => {
                let shares_held = port
                    .positions
                    .iter()
                    .filter(|p| {
                        p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_))
                    })
                    .map(|p| p.quantity)
                    .sum::<f64>();
                let required = contracts as f64 * 100.0;
                if shares_held < required {
                    let margin_usd = naked_margin_usd(&opt_type, price_usd, strike, contracts, premium_usd);
                    let margin_creds = price_to_creds(margin_usd);
                    let available = port.cash - port.locked_cash();
                    if available < margin_creds {
                        ctx.send(
                            poise::CreateReply::default().embed(
                                serenity::CreateEmbed::new()
                                    .title("Options Write — Naked Call")
                                    .description(format!(
                                        "Naked call requires **${:.2}** margin ({:.0} creds) in **{}** but only **${:.2}** ({:.0} creds) is free.\n\n*Alternatively, hold **{:.0} shares** of **{}** to write a covered call.*",
                                        margin_usd, margin_creds, portfolio,
                                        creds_to_price(available), available,
                                        required, ticker,
                                    ))
                                    .color(data::EMBED_ERROR),
                            ),
                        )
                        .await?;
                        return Ok(());
                    }
                    collateral_locked = margin_creds;
                }
            }
            OptionType::Put => {
                let required_cash = price_to_creds(strike * contracts as f64 * SHARES_PER_CONTRACT);
                let available = port.cash - port.locked_cash();
                if available < required_cash {
                    let margin_usd = naked_margin_usd(&opt_type, price_usd, strike, contracts, premium_usd);
                    let margin_creds = price_to_creds(margin_usd);
                    if available < margin_creds {
                        ctx.send(
                            poise::CreateReply::default().embed(
                                serenity::CreateEmbed::new()
                                    .title("Options Write — Naked Put")
                                    .description(format!(
                                        "Naked put requires **${:.2}** margin ({:.0} creds) in **{}** but only **${:.2}** ({:.0} creds) is free.\n\n*Alternatively, hold **${:.2}** ({:.0} creds) to write a cash-secured put.*",
                                        margin_usd, margin_creds, portfolio,
                                        creds_to_price(available), available,
                                        creds_to_price(required_cash), required_cash,
                                    ))
                                    .color(data::EMBED_ERROR),
                            ),
                        )
                        .await?;
                        return Ok(());
                    }
                    collateral_locked = margin_creds;
                }
            }
        }

        port.cash += premium;

        let existing_idx =
            find_option_idx(&port.positions, &ticker, strike, expiry_dt, &opt_type, &OptionSide::Short);

        if let Some(idx) = existing_idx {
            let pos = &mut port.positions[idx];
            let total_q = pos.quantity + contracts as f64;
            pos.avg_cost =
                (pos.avg_cost * pos.quantity + premium_per_contract * contracts as f64) / total_q;
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
                quantity: contracts as f64,
                avg_cost: premium_per_contract,
            });
        }
    }

    let type_str = option_type_str(&opt_type);
    let record = TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("SHORT {} {} ${:.2} {}", ticker, type_str, strike, expiry),
        action: TradeAction::Sell,
        quantity: contracts as f64,
        price_per_unit: premium_per_contract,
        total_creds: premium,
        realized_pnl: None,
        timestamp: Utc::now(),
    };
    user_data.stock.trade_history.push_back(record);
    if user_data.stock.trade_history.len() > TRADE_HISTORY_LIMIT {
        user_data.stock.trade_history.pop_front();
    }
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Write")
                .description(format!(
                    "Written **{}× {} {} ${:.2}** exp {}\nCollected **${:.2}** ({:.0} creds)",
                    contracts, ticker, type_str, strike, expiry,
                    creds_to_price(premium), premium
                ))
                .color(data::EMBED_CYAN)
                .footer(default_footer()),
        ),
    )
    .await?;
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
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Options Cover")
                    .description(ERR_MIN_CONTRACTS)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let opt_type = match parse_option_type(&option_type) {
        Some(t) => t,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Cover")
                        .description(ERR_INVALID_OPTION_TYPE)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let expiry_dt = match parse_expiry(&expiry) {
        Some(d) => d,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Cover")
                        .description(ERR_INVALID_EXPIRY)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let ticker = ticker.to_uppercase();
    let price_usd = match fetch_price(&ticker).await {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Cover")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = option_intrinsic(&opt_type, price_usd, strike);
    let cost_to_close = option_premium_creds(intrinsic, &expiry_dt, contracts);
    let cost_per_contract = cost_to_close / contracts as f64;

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let type_str = option_type_str(&opt_type);
    let (pnl, premium_received) = {
        let port = match user_data
            .stock
            .portfolios
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&portfolio))
        {
            Some(p) => p,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Options Cover")
                            .description(format!("No portfolio named **{}** found.", portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let pos_idx = match find_option_idx(&port.positions, &ticker, strike, expiry_dt, &opt_type, &OptionSide::Short) {
            Some(i) => i,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Options Cover")
                            .description(format!(
                                "No SHORT **{} {} ${:.2}** exp {} in portfolio **{}**.",
                                ticker, type_str, strike, expiry, portfolio
                            ))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let (held, collateral_total) = if let AssetType::Option(c) = &port.positions[pos_idx].asset_type {
            (c.contracts, c.collateral)
        } else {
            (0, 0.0)
        };

        if contracts > held {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Cover")
                        .description(format!(
                            "You only wrote **{}** contracts but tried to cover **{}**.",
                            held, contracts
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        let collateral_to_release = collateral_total * (contracts as f64 / held as f64);
        let available = port.cash - port.locked_cash() + collateral_to_release;

        if available < cost_to_close {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Cover")
                        .description(format!(
                            "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** only has **${:.2}** ({:.0} creds) available.",
                            creds_to_price(cost_to_close), cost_to_close,
                            portfolio, creds_to_price(available), available,
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        let avg_cost = port.positions[pos_idx].avg_cost; // premium received per contract
        let premium_received = avg_cost * contracts as f64;
        let pnl = premium_received - cost_to_close;
        port.cash -= cost_to_close;

        if contracts == held {
            port.positions.remove(pos_idx);
        } else {
            let pos = &mut port.positions[pos_idx];
            pos.quantity -= contracts as f64;
            if let AssetType::Option(c) = &mut pos.asset_type {
                c.contracts -= contracts;
                c.collateral -= collateral_to_release;
            }
        }

        (pnl, premium_received)
    };

    let type_str = option_type_str(&opt_type);
    let record = TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: format!("SHORT {} {} ${:.2} {}", ticker, type_str, strike, expiry),
        action: TradeAction::Buy,
        quantity: contracts as f64,
        price_per_unit: cost_per_contract,
        total_creds: cost_to_close,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    };
    user_data.stock.trade_history.push_back(record);
    if user_data.stock.trade_history.len() > TRADE_HISTORY_LIMIT {
        user_data.stock.trade_history.pop_front();
    }

    let pnl_str = crate::helper::fmt_pnl(pnl);
    let color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
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
        ),
    )
    .await?;

    Ok(())
}
