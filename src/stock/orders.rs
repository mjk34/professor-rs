//! Hidden /buy and /sell slash commands (users enter trades through /search).

use crate::api::{is_market_hours, market_data_err, order_expiry, resolve_ticker, with_logo};
use crate::data::{self, AssetType, OrderSide, PendingOrder, MAX_PENDING_ORDERS};
use crate::helper::{creds_to_price, default_footer, fmt_limit_tag, fmt_qty, price_to_creds};
use crate::trader::{apply_buy, apply_sell};
use crate::{serenity, Context, Error};
use std::time::Duration;

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
    if quantity.is_none() && amount.is_none() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description("Provide either **quantity** (shares) or **amount** (dollars), not neither.")
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }
    if quantity.is_some() && amount.is_some() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description("Provide either **quantity** (shares) or **amount** (dollars), not both.")
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let quote = if let Some(q) = resolve_ticker(&ticker_query).await { q } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description(market_data_err(&ticker_query))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();

    let price_usd = if let Some(p) = quote.regular_market_price { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description(market_data_err(&ticker))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let asset_type    = quote.asset_type();
    let price_per_unit = price_to_creds(price_usd);
    let quantity      = quantity.unwrap_or_else(|| amount.unwrap() / price_usd);

    if quantity <= 0.0 {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description("Quantity must be greater than 0.")
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let total_cost = match amount {
        Some(amt) => price_to_creds(amt),
        None => price_per_unit * quantity,
    };

    let should_queue = !is_market_hours() || limit_price.is_some_and(|lp| price_usd > lp);

    if should_queue {
        ctx.defer().await?;
        let expiry = order_expiry();
        let reason = if is_market_hours() {
            format!("Limit buy: current price **${:.2}** > limit **${:.2}**.", price_usd, limit_price.unwrap())
        } else {
            "Market is closed — order will execute at next open.".to_string()
        };
        let limit_str  = limit_price.map(|lp| format!(" @ limit **${lp:.2}**")).unwrap_or_default();
        let confirm_id = format!("buy_confirm_{}", ctx.author().id);
        let cancel_id  = format!("buy_cancel_{}", ctx.author().id);

        let reply = ctx.send(poise::CreateReply::default()
            .embed(serenity::CreateEmbed::new().title("Queue Order?")
                .description(format!(
                    "{}\n\nQueue **{} {}**{} in **{}**?\nExpires: <t:{}:f>",
                    reason, fmt_qty(quantity), ticker, limit_str, portfolio, expiry.timestamp(),
                ))
                .color(data::EMBED_DEFAULT))
            .components(vec![serenity::CreateActionRow::Buttons(vec![
                serenity::CreateButton::new(&confirm_id).label("Queue").style(serenity::ButtonStyle::Primary),
                serenity::CreateButton::new(&cancel_id).label("Cancel").style(serenity::ButtonStyle::Secondary),
            ])]),
        ).await?;

        let msg = reply.message().await?;
        let press = if let Some(p) = msg
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(60))
            .await { p } else {
            reply.edit(ctx, poise::CreateReply::default().components(vec![])).await?;
            return Ok(());
        };

        press.defer(ctx.serenity_context()).await?;

        if press.data.custom_id == cancel_id {
            reply.edit(ctx, poise::CreateReply::default()
                .embed(serenity::CreateEmbed::new().title("Cancelled").color(data::EMBED_ERROR))
                .components(vec![])).await?;
            return Ok(());
        }

        {
            let data_ref = &ctx.data().users;
            let u = data_ref.get(&ctx.author().id).unwrap();
            let mut user_data = u.write().await;

            if !user_data.stock.portfolios.iter().any(|p| p.name.eq_ignore_ascii_case(&portfolio)) {
                reply.edit(ctx, poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new().title("Queue Failed")
                        .description(format!("No portfolio named **{portfolio}** found."))
                        .color(data::EMBED_ERROR))
                    .components(vec![])).await?;
                return Ok(());
            }
            if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                reply.edit(ctx, poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new().title("Queue Failed")
                        .description(format!("You have reached the limit of **{MAX_PENDING_ORDERS}** pending orders. Cancel some in `/portfolio`."))
                        .color(data::EMBED_ERROR))
                    .components(vec![])).await?;
                return Ok(());
            }
            let id = user_data.stock.next_order_id;
            user_data.stock.next_order_id = id.wrapping_add(1);
            user_data.stock.pending_orders.push(PendingOrder {
                id, side: OrderSide::Buy, ticker: ticker.clone(), asset_name: asset_name.clone(),
                asset_type, portfolio_name: portfolio.clone(), quantity, limit_price, expiry,
            });
        }

        reply.edit(ctx, poise::CreateReply::default()
            .embed(serenity::CreateEmbed::new().title("Buy Order Queued")
                .description(format!(
                    "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                    fmt_qty(quantity), ticker, fmt_limit_tag(limit_price), portfolio, quantity * price_usd, expiry.timestamp(),
                ))
                .color(data::EMBED_SUCCESS))
            .components(vec![])).await?;
        return Ok(());
    }

    // Immediate execution
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = if let Some(i) = user_data.stock.portfolios.iter().position(|p| p.name.eq_ignore_ascii_case(&portfolio)) { i } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description(format!("No portfolio named **{portfolio}** found."))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    if user_data.stock.portfolios[port_idx].cash < total_cost {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Buy")
                .description(format!(
                    "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                    creds_to_price(total_cost), total_cost, portfolio,
                    creds_to_price(user_data.stock.portfolios[port_idx].cash),
                    user_data.stock.portfolios[port_idx].cash
                ))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    {
        let stock = &mut user_data.stock;
        apply_buy(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &asset_name, asset_type, quantity, price_per_unit, total_cost, &portfolio);
    }
    drop(user_data);

    ctx.send(poise::CreateReply::default().embed(
        with_logo(
            serenity::CreateEmbed::new().title("Buy")
                .description(format!(
                    "Bought **{} {}** ({}) for **${:.2}** ({:.0} creds)\n${:.2}/unit | Portfolio: **{}**",
                    fmt_qty(quantity), ticker, asset_name, creds_to_price(total_cost), total_cost, price_usd, portfolio
                ))
                .color(data::EMBED_SUCCESS).footer(default_footer()),
            &ticker,
        )
    )).await?;
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
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Sell")
                .description("Provide either a **quantity** or a **dollar amount**, not both.")
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let quote = if let Some(q) = resolve_ticker(&ticker_query).await { q } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Sell")
                .description(market_data_err(&ticker_query))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };
    let ticker     = quote.symbol.clone();
    let asset_name = quote.display_name();

    let price_usd = if let Some(p) = quote.regular_market_price { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Sell")
                .description(market_data_err(&ticker))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let price_per_unit = price_to_creds(price_usd);

    let (held, port_name_normalized) = {
        let data_ref = &ctx.data().users;
        let u = data_ref.get(&ctx.author().id).unwrap();
        let user_data = u.read().await;

        let port_idx = if let Some(i) = user_data.stock.portfolios.iter().position(|p| p.name.eq_ignore_ascii_case(&portfolio)) { i } else {
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Sell")
                    .description(format!("No portfolio named **{portfolio}** found."))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
        };

        let pos = user_data.stock.portfolios[port_idx].positions.iter()
            .find(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)));

        if let Some(p) = pos {
            (p.quantity, user_data.stock.portfolios[port_idx].name.clone())
        } else {
            drop(user_data);
            ctx.send(poise::CreateReply::default().embed(
                serenity::CreateEmbed::new().title("Sell")
                    .description(format!("No **{ticker}** position in portfolio **{portfolio}**."))
                    .color(data::EMBED_ERROR),
            )).await?;
            return Ok(());
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
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Sell")
                .description(format!("You only hold **{}** of **{}** but tried to sell **{}**.", fmt_qty(held), ticker, fmt_qty(quantity)))
                .color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    let should_queue = !is_market_hours() || limit_price.is_some_and(|lp| price_usd < lp);

    if should_queue {
        ctx.defer().await?;
        let expiry = order_expiry();
        let reason = if is_market_hours() {
            format!("Limit sell: current price **${:.2}** < limit **${:.2}**.", price_usd, limit_price.unwrap())
        } else {
            "Market is closed — order will execute at next open.".to_string()
        };
        let limit_str  = limit_price.map(|lp| format!(" @ limit **${lp:.2}**")).unwrap_or_default();
        let confirm_id = format!("sell_confirm_{}", ctx.author().id);
        let cancel_id  = format!("sell_cancel_{}", ctx.author().id);

        let reply = ctx.send(poise::CreateReply::default()
            .embed(serenity::CreateEmbed::new().title("Queue Order?")
                .description(format!(
                    "{}\n\nQueue **{} {}**{} from **{}**?\nExpires: <t:{}:f>",
                    reason, fmt_qty(quantity), ticker, limit_str, port_name_normalized, expiry.timestamp(),
                ))
                .color(data::EMBED_DEFAULT))
            .components(vec![serenity::CreateActionRow::Buttons(vec![
                serenity::CreateButton::new(&confirm_id).label("Queue").style(serenity::ButtonStyle::Primary),
                serenity::CreateButton::new(&cancel_id).label("Cancel").style(serenity::ButtonStyle::Secondary),
            ])]),
        ).await?;

        let msg = reply.message().await?;
        let press = if let Some(p) = msg
            .await_component_interaction(ctx.serenity_context())
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(60))
            .await { p } else {
            reply.edit(ctx, poise::CreateReply::default().components(vec![])).await?;
            return Ok(());
        };

        press.defer(ctx.serenity_context()).await?;

        if press.data.custom_id == cancel_id {
            reply.edit(ctx, poise::CreateReply::default()
                .embed(serenity::CreateEmbed::new().title("Cancelled").color(data::EMBED_ERROR))
                .components(vec![])).await?;
            return Ok(());
        }

        {
            let data_ref = &ctx.data().users;
            let u = data_ref.get(&ctx.author().id).unwrap();
            let mut user_data = u.write().await;

            if user_data.stock.pending_orders.len() >= MAX_PENDING_ORDERS {
                reply.edit(ctx, poise::CreateReply::default()
                    .embed(serenity::CreateEmbed::new().title("Queue Failed")
                        .description(format!("You have reached the limit of **{MAX_PENDING_ORDERS}** pending orders. Cancel some in `/portfolio`."))
                        .color(data::EMBED_ERROR))
                    .components(vec![])).await?;
                return Ok(());
            }
            let id = user_data.stock.next_order_id;
            user_data.stock.next_order_id = id.wrapping_add(1);
            user_data.stock.pending_orders.push(PendingOrder {
                id, side: OrderSide::Sell, ticker: ticker.clone(), asset_name: asset_name.clone(),
                asset_type: AssetType::Stock, portfolio_name: port_name_normalized.clone(),
                quantity, limit_price, expiry,
            });
        }

        reply.edit(ctx, poise::CreateReply::default()
            .embed(serenity::CreateEmbed::new().title("Sell Order Queued")
                .description(format!(
                    "**{}** {} {} — **{}**\n(total value: **${:.2}**)\nExpires: <t:{}:f>",
                    fmt_qty(quantity), ticker, fmt_limit_tag(limit_price), port_name_normalized, quantity * price_usd, expiry.timestamp(),
                ))
                .color(data::EMBED_SUCCESS))
            .components(vec![])).await?;
        return Ok(());
    }

    // Immediate execution
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = user_data.stock.portfolios.iter().position(|p| p.name == port_name_normalized).unwrap();

    let (proceeds, pnl) = {
        let stock = &mut user_data.stock;
        let pnl = apply_sell(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &asset_name, quantity, price_per_unit, &portfolio).unwrap_or(0.0);
        (price_per_unit * quantity, pnl)
    };

    let pnl_str = if pnl >= 0.0 { format!("▲ +${:.2} ({:.0} creds)", creds_to_price(pnl), pnl) }
                  else           { format!("▼ -${:.2} ({:.0} creds)", creds_to_price(pnl.abs()), pnl) };
    let color = if pnl >= 0.0 { data::EMBED_SUCCESS } else { data::EMBED_FAIL };
    drop(user_data);

    ctx.send(poise::CreateReply::default().embed(
        with_logo(
            serenity::CreateEmbed::new().title("Sell")
                .description(format!(
                    "Sold **{} {}** for **${:.2}** ({:.0} creds)\n${:.2}/unit | Realized P&L: **{}**",
                    fmt_qty(quantity), ticker, creds_to_price(proceeds), proceeds, price_usd, pnl_str
                ))
                .color(color).footer(default_footer()),
            &ticker,
        )
    )).await?;
    Ok(())
}
