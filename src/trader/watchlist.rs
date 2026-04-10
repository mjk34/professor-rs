//! Watchlist command — view and manage per-user ticker watchlists.

use crate::api::{fetch_quote_detail, market_data_err};
use crate::{data, serenity, Context, Error};
use poise::serenity_prelude::futures;
use std::time::Duration;
use crate::helper::default_footer;

#[derive(Debug, poise::Modal)]
#[name = "Add to Watchlist"]
pub struct WatchlistAddModal {
    #[name = "Ticker symbol (e.g. AAPL, BTC-USD)"]
    #[placeholder = "AAPL"]
    pub ticker: String,
}

#[derive(Debug, poise::Modal)]
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
