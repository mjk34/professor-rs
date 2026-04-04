//!---------------------------------------------------------------------!
//! Stock / portfolio trading commands                                   !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - portfolio (create, list, view, fund, withdraw, delete)    !
//!     [x] - search                                                    !
//!     [x] - buy / sell                                                !
//!     [x] - watchlist (add, remove, list)                             !
//!     [x] - trades                                                    !
//!     [x] - options_quote / options_buy / options_sell                !
//!---------------------------------------------------------------------!

use crate::data::{
    self, AssetType, OptionContract, OptionType, Portfolio, Position, TradeAction, TradeRecord,
    UserData, GOLD_LEVEL_THRESHOLD, TRADE_HISTORY_LIMIT,
};
use crate::{serenity, Context, Error};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use dashmap::DashMap;
use poise::serenity_prelude::{futures::StreamExt, ChannelId, CreateMessage, EditMessage};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

type UsersMap = Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>;

// ── Pricing helpers ──────────────────────────────────────────────────────────

fn price_to_creds(usd: f64) -> f64 {
    usd * 100.0
}

fn creds_to_price(creds: f64) -> f64 {
    creds / 100.0
}

fn gold_hysa_rate(fed_rate: f64) -> f64 {
    (fed_rate * 0.92).max(0.5)
}

fn is_gold(user_data: &UserData) -> bool {
    user_data.get_level() >= GOLD_LEVEL_THRESHOLD
}

fn format_large_num(n: f64) -> String {
    if n >= 1e12 {
        format!("${:.2}T", n / 1e12)
    } else if n >= 1e9 {
        format!("${:.2}B", n / 1e9)
    } else if n >= 1e6 {
        format!("${:.2}M", n / 1e6)
    } else {
        format!("${:.2}", n)
    }
}

// ── Yahoo Finance API structs ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct YfChartResponse {
    chart: YfChartOuter,
}

#[derive(Deserialize)]
struct YfChartOuter {
    result: Option<Vec<YfChartEntry>>,
}

#[derive(Deserialize)]
struct YfChartEntry {
    meta: YfChartMeta,
}

#[derive(Deserialize)]
struct YfChartMeta {
    symbol: String,
    #[serde(rename = "longName")]
    long_name: Option<String>,
    #[serde(rename = "shortName")]
    short_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "chartPreviousClose")]
    chart_previous_close: Option<f64>,
    #[serde(rename = "regularMarketVolume")]
    regular_market_volume: Option<u64>,
    #[serde(rename = "52WeekHigh")]
    fifty_two_week_high: Option<f64>,
    #[serde(rename = "52WeekLow")]
    fifty_two_week_low: Option<f64>,
    #[serde(rename = "marketCap")]
    market_cap: Option<f64>,
    #[serde(rename = "instrumentType")]
    instrument_type: Option<String>,
}

#[derive(Deserialize, Clone)]
struct YfQuote {
    symbol: String,
    #[serde(rename = "longName")]
    long_name: Option<String>,
    #[serde(rename = "shortName")]
    short_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "regularMarketChange")]
    regular_market_change: Option<f64>,
    #[serde(rename = "regularMarketChangePercent")]
    regular_market_change_percent: Option<f64>,
    #[serde(rename = "regularMarketVolume")]
    regular_market_volume: Option<u64>,
    #[serde(rename = "marketCap")]
    market_cap: Option<f64>,
    #[serde(rename = "trailingPE")]
    trailing_pe: Option<f64>,
    #[serde(rename = "fiftyTwoWeekHigh")]
    fifty_two_week_high: Option<f64>,
    #[serde(rename = "fiftyTwoWeekLow")]
    fifty_two_week_low: Option<f64>,
    #[serde(rename = "quoteType")]
    quote_type: Option<String>,
}

impl YfQuote {
    fn display_name(&self) -> String {
        self.long_name
            .clone()
            .or_else(|| self.short_name.clone())
            .unwrap_or_else(|| self.symbol.clone())
    }
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; professor-rs/1.0)")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

const QUOTE_CACHE_TTL: Duration = Duration::from_secs(60);
static QUOTE_CACHE: LazyLock<DashMap<String, (YfQuote, Instant)>> =
    LazyLock::new(DashMap::new);

async fn resolve_ticker(query: &str) -> Option<YfQuote> {
    fetch_quote_detail(query.trim().to_uppercase().as_str()).await
}

async fn fetch_price(ticker: &str) -> Option<f64> {
    fetch_quote_detail(ticker)
        .await
        .and_then(|q| q.regular_market_price)
}

async fn fetch_quote_detail(ticker: &str) -> Option<YfQuote> {
    if let Some(entry) = QUOTE_CACHE.get(ticker) {
        if entry.1.elapsed() < QUOTE_CACHE_TTL {
            return Some(entry.0.clone());
        }
    }

    let resp = HTTP_CLIENT
        .get(format!(
            "https://query2.finance.yahoo.com/v8/finance/chart/{}",
            ticker
        ))
        .query(&[("interval", "1d"), ("range", "1d")])
        .send()
        .await
        .ok()?
        .json::<YfChartResponse>()
        .await
        .ok()?;

    let meta = resp.chart.result?.into_iter().next()?.meta;

    let price_prev = meta.regular_market_price.zip(meta.chart_previous_close);
    let change = price_prev.map(|(p, c)| p - c);
    let change_pct = price_prev.map(|(p, c)| (p - c) / c * 100.0);

    let quote = YfQuote {
        symbol: meta.symbol,
        long_name: meta.long_name,
        short_name: meta.short_name,
        regular_market_price: meta.regular_market_price,
        regular_market_change: change,
        regular_market_change_percent: change_pct,
        regular_market_volume: meta.regular_market_volume,
        market_cap: meta.market_cap,
        trailing_pe: None,
        fifty_two_week_high: meta.fifty_two_week_high,
        fifty_two_week_low: meta.fifty_two_week_low,
        quote_type: meta.instrument_type,
    };
    QUOTE_CACHE.insert(quote.symbol.clone(), (quote.clone(), Instant::now()));
    Some(quote)
}

fn parse_expiry(date_str: &str) -> Option<chrono::DateTime<Utc>> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| Utc.from_utc_datetime(&dt))
}

fn parse_option_type(s: &str) -> Option<OptionType> {
    match s.to_lowercase().as_str() {
        "call" | "c" => Some(OptionType::Call),
        "put" | "p" => Some(OptionType::Put),
        _ => None,
    }
}

fn option_type_str(ot: &OptionType) -> &'static str {
    match ot {
        OptionType::Call => "CALL",
        OptionType::Put => "PUT",
    }
}

// ── FRED API ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FredResponse {
    observations: Vec<FredObservation>,
}

#[derive(Deserialize)]
struct FredObservation {
    value: String,
}

async fn fetch_fed_funds_rate() -> Option<f64> {
    let api_key = std::env::var("FRED_API_KEY").ok()?;
    let resp = HTTP_CLIENT
        .get("https://api.stlouisfed.org/fred/series/observations")
        .query(&[
            ("series_id", "DFF"),
            ("api_key", &api_key),
            ("sort_order", "desc"),
            ("limit", "1"),
            ("file_type", "json"),
        ])
        .send()
        .await
        .ok()?
        .json::<FredResponse>()
        .await
        .ok()?;

    resp.observations
        .into_iter()
        .next()
        .and_then(|o| o.value.parse::<f64>().ok())
}

// ── Maintenance functions ─────────────────────────────────────────────────────

pub async fn refresh_market_rate(rate: &Arc<RwLock<f64>>) {
    if let Some(r_val) = fetch_fed_funds_rate().await {
        let mut r = rate.write().await;
        *r = r_val;
        tracing::info!("HYSA fed rate updated: {:.2}%", r_val);
    } else {
        tracing::warn!("Failed to fetch FRED fed funds rate; keeping current value");
    }
}

pub async fn apply_monthly_interest(users: &UsersMap, rate: &Arc<RwLock<f64>>) {
    let now = Utc::now();
    if now.day() != 1 {
        return;
    }

    let fed_rate = *rate.read().await;

    for entry in users.iter() {
        let (_, u) = entry.pair();
        let mut user_data = u.write().await;
        let annual_rate = if is_gold(&user_data) {
            gold_hysa_rate(fed_rate)
        } else {
            data::BASE_HYSA_RATE
        };

        for portfolio in &mut user_data.stock.portfolios {
            if portfolio.cash <= 0.0 {
                continue;
            }
            let last = portfolio.last_interest_credited;
            if last.year() == now.year() && last.month() == now.month() {
                continue;
            }
            let interest = (annual_rate / 100.0 / 12.0) * portfolio.cash;
            portfolio.cash += interest;
            portfolio.last_interest_credited = now;
            tracing::info!(
                "Credited {:.2} interest to portfolio '{}'",
                interest,
                portfolio.name
            );
        }
    }
}

pub async fn sweep_expired_options(
    users: &UsersMap,
    http: &Arc<serenity::Http>,
    bot_chat: &str,
) {
    let now = Utc::now();
    let Ok(channel_id) = bot_chat.parse::<u64>() else {
        return;
    };
    let channel = ChannelId::new(channel_id);

    // Phase 1: collect expired positions under read lock (no await while holding)
    struct ExpiredInfo {
        user_id: serenity::UserId,
        portfolio_name: String,
        ticker: String,
        contract: OptionContract,
        avg_cost: f64,
        quantity: f64,
    }

    let mut to_process: Vec<ExpiredInfo> = Vec::new();

    for entry in users.iter() {
        let (user_id, u) = entry.pair();
        let user_data = u.read().await;
        for portfolio in &user_data.stock.portfolios {
            for pos in &portfolio.positions {
                if let AssetType::Option(contract) = &pos.asset_type {
                    if contract.expiry < now {
                        to_process.push(ExpiredInfo {
                            user_id: *user_id,
                            portfolio_name: portfolio.name.clone(),
                            ticker: pos.ticker.clone(),
                            contract: contract.clone(),
                            avg_cost: pos.avg_cost,
                            quantity: pos.quantity,
                        });
                    }
                }
            }
        }
        // read lock released here
    }

    // Phase 2: fetch prices for unique tickers (no locks held)
    let mut prices: HashMap<String, f64> = HashMap::new();
    for info in &to_process {
        if !prices.contains_key(&info.ticker) {
            prices.insert(
                info.ticker.clone(),
                fetch_price(&info.ticker).await.unwrap_or(0.0),
            );
        }
    }

    // Phase 3: apply changes under write lock (no await while holding)
    for info in to_process {
        let u = match users.get(&info.user_id) {
            Some(u) => u,
            None => continue,
        };

        let price_usd = *prices.get(&info.ticker).unwrap_or(&0.0);
        let intrinsic = match info.contract.option_type {
            OptionType::Call => (price_usd - info.contract.strike).max(0.0),
            OptionType::Put => (info.contract.strike - price_usd).max(0.0),
        };
        let proceeds_creds = price_to_creds(intrinsic * info.contract.contracts as f64 * 100.0);
        let cost_basis = info.avg_cost * info.quantity;
        let pnl = proceeds_creds - cost_basis;
        let itm = intrinsic > 0.0;
        let type_str = option_type_str(&info.contract.option_type);

        let msg = if itm {
            format!(
                "<@{}> Options expired **ITM** — **{}** {} | Received **{:.0} creds** (P&L: {:+.0})",
                info.user_id, info.ticker, type_str, proceeds_creds, pnl
            )
        } else {
            format!(
                "<@{}> Options expired **OTM** (worthless) — **{}** {} | Lost **{:.0} creds**",
                info.user_id, info.ticker, type_str, cost_basis
            )
        };

        {
            let mut user_data = u.write().await;
            if let Some(portfolio) = user_data
                .stock
                .portfolios
                .iter_mut()
                .find(|p| p.name == info.portfolio_name)
            {
                portfolio.cash += proceeds_creds;
                portfolio.positions.retain(|p| {
                    if p.ticker != info.ticker {
                        return true;
                    }
                    if let AssetType::Option(c) = &p.asset_type {
                        !(c.strike == info.contract.strike
                            && c.expiry == info.contract.expiry
                            && c.option_type == info.contract.option_type)
                    } else {
                        true
                    }
                });
            }

            let record = TradeRecord {
                portfolio: info.portfolio_name.clone(),
                ticker: info.ticker.clone(),
                asset_name: format!(
                    "{} {} ${:.2} {}",
                    info.ticker,
                    type_str,
                    info.contract.strike,
                    info.contract.expiry.format("%Y-%m-%d")
                ),
                action: TradeAction::Sell,
                quantity: info.quantity,
                price_per_unit: proceeds_creds / info.quantity.max(1.0),
                total_creds: proceeds_creds,
                realized_pnl: Some(pnl),
                timestamp: now,
            };
            user_data.stock.trade_history.push_back(record);
            if user_data.stock.trade_history.len() > TRADE_HISTORY_LIMIT {
                user_data.stock.trade_history.pop_front();
            }
            // write lock released here
        }

        let _ = channel
            .send_message(http, CreateMessage::new().content(msg))
            .await;
    }
}

// ── Portfolio commands ─────────────────────────────────────────────────────────

/// Manage your investment portfolios
#[poise::command(
    slash_command,
    subcommands(
        "portfolio_create",
        "portfolio_list",
        "portfolio_view",
        "portfolio_fund",
        "portfolio_withdraw",
        "portfolio_delete"
    )
)]
pub async fn portfolio(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Create a new portfolio
#[poise::command(slash_command, rename = "create")]
async fn portfolio_create(
    ctx: Context<'_>,
    #[description = "Name for the new portfolio (max 20 chars)"] name: String,
) -> Result<(), Error> {
    if name.len() > 20 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Create")
                    .description("Portfolio name must be 20 characters or fewer.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    // Validate under read lock, mutate under write lock, then send — no lock held across await.
    let err = {
        let user_data = u.read().await;
        if user_data
            .stock
            .portfolios
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&name))
        {
            Some(format!("A portfolio named **{}** already exists.", name))
        } else if user_data.stock.portfolios.len() >= 5 {
            Some("You can have at most **5** portfolios.".to_string())
        } else {
            None
        }
        // read lock released here
    };

    if let Some(msg) = err {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Create")
                    .description(msg)
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    {
        let mut user_data = u.write().await;
        user_data.stock.portfolios.push(Portfolio::new(name.clone()));
        // write lock released here
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Portfolio — Create")
                .description(format!(
                    "Portfolio **{}** created!\n\nFund it with `/portfolio fund {} <amount>`.",
                    name, name
                ))
                .color(data::EMBED_SUCCESS)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

/// List all your portfolios
#[poise::command(slash_command, rename = "list")]
async fn portfolio_list(ctx: Context<'_>) -> Result<(), Error> {
    // Read hysa_fed_rate first so we never await while holding user_data lock.
    let fed_rate_val = *ctx.data().hysa_fed_rate.read().await;

    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    // Collect everything we need under the read lock, then drop it.
    let (desc, rate_label) = {
        let user_data = u.read().await;

        if user_data.stock.portfolios.is_empty() {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — List")
                        .description(
                            "You have no portfolios. Create one with `/portfolio create <name>`.",
                        )
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        let annual_rate = if is_gold(&user_data) {
            gold_hysa_rate(fed_rate_val)
        } else {
            data::BASE_HYSA_RATE
        };
        let daily_rate = annual_rate / 100.0 / 365.0;

        let mut desc = String::new();
        for p in &user_data.stock.portfolios {
            let daily_accrual = daily_rate * p.cash;
            desc += &format!(
                "**{}** — Cash: **${:.2}** ({:.0} creds) | {} positions | ~{:.1} creds/day\n",
                p.name,
                creds_to_price(p.cash),
                p.cash,
                p.positions.len(),
                daily_accrual
            );
        }

        let rate_label = if is_gold(&user_data) {
            format!("Gold HYSA: {:.2}% APY", gold_hysa_rate(fed_rate_val))
        } else {
            format!("Base HYSA: {:.2}% APY", data::BASE_HYSA_RATE)
        };

        (desc, rate_label)
        // read lock released here
    };

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Portfolio — List")
                .description(desc)
                .color(data::EMBED_CYAN)
                .footer(serenity::CreateEmbedFooter::new(format!(
                    "{} | @~ powered by UwUntu & RustyBamboo",
                    rate_label
                ))),
        ),
    )
    .await?;
    Ok(())
}

/// View a portfolio's positions and current P&L
#[poise::command(slash_command, rename = "view")]
async fn portfolio_view(
    ctx: Context<'_>,
    #[description = "Portfolio name"] name: String,
) -> Result<(), Error> {
    // Read hysa_fed_rate first so we never await while holding user_data lock.
    let fed_rate_val = *ctx.data().hysa_fed_rate.read().await;

    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    // Collect everything under the read lock, then drop it before price fetches.
    let (portfolio, annual_rate) = {
        let user_data = u.read().await;

        let portfolio = match user_data
            .stock
            .portfolios
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            Some(p) => p.clone(),
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — View")
                            .description(format!("No portfolio named **{}** found.", name))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let annual_rate = if is_gold(&user_data) {
            gold_hysa_rate(fed_rate_val)
        } else {
            data::BASE_HYSA_RATE
        };

        (portfolio, annual_rate)
        // read lock released here
    };

    let daily_accrual = (annual_rate / 100.0 / 365.0) * portfolio.cash;

    let mut desc = format!(
        "**Cash:** ${:.2} ({:.0} creds)\n**Daily interest:** ~{:.1} creds\n\n",
        creds_to_price(portfolio.cash),
        portfolio.cash,
        daily_accrual
    );

    if portfolio.positions.is_empty() {
        desc += "*No open positions.*";
    } else {
        desc += "**Positions:**\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";
        for pos in &portfolio.positions {
            let current_price_usd = fetch_price(&pos.ticker).await.unwrap_or(0.0);
            let cost_basis = pos.avg_cost * pos.quantity;

            match &pos.asset_type {
                AssetType::Option(contract) => {
                    let intrinsic = match contract.option_type {
                        OptionType::Call => (current_price_usd - contract.strike).max(0.0),
                        OptionType::Put => (contract.strike - current_price_usd).max(0.0),
                    };
                    let current_value =
                        price_to_creds(intrinsic * 100.0) * contract.contracts as f64;
                    let pnl = current_value - cost_basis;
                    let pnl_pct = if cost_basis > 0.0 {
                        pnl / cost_basis * 100.0
                    } else {
                        0.0
                    };
                    desc += &format!(
                        "**{} {} ${:.2}** exp {} — {} contracts\nCost: {:.0} | Value: {:.0} | P&L: {:+.0} ({:+.1}%)\n\n",
                        pos.ticker,
                        option_type_str(&contract.option_type),
                        contract.strike,
                        contract.expiry.format("%Y-%m-%d"),
                        contract.contracts,
                        cost_basis,
                        current_value,
                        pnl,
                        pnl_pct
                    );
                }
                _ => {
                    let current_creds = price_to_creds(current_price_usd);
                    let current_value = current_creds * pos.quantity;
                    let pnl = current_value - cost_basis;
                    let pnl_pct = if cost_basis > 0.0 {
                        pnl / cost_basis * 100.0
                    } else {
                        0.0
                    };
                    desc += &format!(
                        "**{}** × {:.4} — Avg: {:.0}¢ | Now: {:.0}¢\nValue: {:.0} creds | P&L: {:+.0} ({:+.1}%)\n\n",
                        pos.ticker,
                        pos.quantity,
                        pos.avg_cost,
                        current_creds,
                        current_value,
                        pnl,
                        pnl_pct
                    );
                }
            }
        }
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(format!("Portfolio — {}", portfolio.name))
                .description(desc)
                .color(data::EMBED_CYAN)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

/// Move creds from your wallet into a portfolio
#[poise::command(slash_command, rename = "fund")]
async fn portfolio_fund(
    ctx: Context<'_>,
    #[description = "Portfolio name"] name: String,
    #[description = "Amount of creds to deposit"] amount: u32,
) -> Result<(), Error> {
    if amount == 0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Fund")
                    .description("Amount must be greater than 0.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    let fund_result: Result<f64, String> = {
        let mut user_data = u.write().await;
        if user_data.get_creds() < amount as i32 {
            Err(format!(
                "Insufficient creds. You have **{}** but tried to deposit **{}**.",
                user_data.get_creds(),
                amount
            ))
        } else {
            match user_data
                .stock
                .portfolios
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
            {
                None => Err(format!("No portfolio named **{}** found.", name)),
                Some(p) => {
                    p.cash += amount as f64;
                    let new_cash = p.cash;
                    user_data.sub_creds(amount as i32);
                    Ok(new_cash)
                }
            }
        }
        // write lock released here
    };

    match fund_result {
        Err(msg) => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Fund")
                        .description(msg)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
        }
        Ok(new_cash) => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Fund")
                        .description(format!(
                            "Deposited **{}** creds into **{}**.\nNew cash balance: **{:.0}** creds.",
                            amount, name, new_cash
                        ))
                        .color(data::EMBED_SUCCESS)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;
        }
    }
    Ok(())
}

/// Withdraw uninvested cash from a portfolio to your wallet
#[poise::command(slash_command, rename = "withdraw")]
async fn portfolio_withdraw(
    ctx: Context<'_>,
    #[description = "Portfolio name"] name: String,
    #[description = "Amount of creds to withdraw"] amount: u32,
) -> Result<(), Error> {
    if amount == 0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Withdraw")
                    .description("Amount must be greater than 0.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    let withdraw_result: Result<f64, String> = {
        let mut user_data = u.write().await;
        match user_data
            .stock
            .portfolios
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            None => Err(format!("No portfolio named **{}** found.", name)),
            Some(p) if p.cash < amount as f64 => Err(format!(
                "Insufficient cash. **{}** has **{:.0}** creds but tried to withdraw **{}**.",
                name, p.cash, amount
            )),
            Some(p) => {
                p.cash -= amount as f64;
                let remaining = p.cash;
                user_data.add_creds(amount as i32);
                Ok(remaining)
            }
        }
        // write lock released here
    };

    match withdraw_result {
        Err(msg) => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Withdraw")
                        .description(msg)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
        }
        Ok(remaining_cash) => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Withdraw")
                        .description(format!(
                            "Withdrew **{}** creds from **{}** to your wallet.\nRemaining cash: **{:.0}** creds.",
                            amount, name, remaining_cash
                        ))
                        .color(data::EMBED_SUCCESS)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;
        }
    }
    Ok(())
}

/// Delete a portfolio (prompts to liquidate if non-empty)
#[poise::command(slash_command, rename = "delete")]
async fn portfolio_delete(
    ctx: Context<'_>,
    #[description = "Portfolio name"] name: String,
) -> Result<(), Error> {
    // Phase 1: gather info under read lock
    let (has_cash, has_positions, cash, positions_count) = {
        let data = &ctx.data().users;
        let u = data.get(&ctx.author().id).unwrap();
        let user_data = u.read().await;

        match user_data
            .stock
            .portfolios
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Delete")
                            .description(format!("No portfolio named **{}** found.", name))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
            Some(p) => (p.cash > 0.0, !p.positions.is_empty(), p.cash, p.positions.len()),
        }
        // read lock released
    };

    // Empty portfolio: delete immediately
    if !has_cash && !has_positions {
        let data = &ctx.data().users;
        let u = data.get(&ctx.author().id).unwrap();
        {
            let mut user_data = u.write().await;
            user_data
                .stock
                .portfolios
                .retain(|p| !p.name.eq_ignore_ascii_case(&name));
            // write lock released here
        }
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Delete")
                    .description(format!("Portfolio **{}** deleted.", name))
                    .color(data::EMBED_SUCCESS)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    // Non-empty: prompt to liquidate
    let detail = if has_cash && has_positions {
        format!(
            "**{}** has **{:.0}** creds cash and **{}** open positions.",
            name, cash, positions_count
        )
    } else if has_cash {
        format!("**{}** has **{:.0}** creds cash.", name, cash)
    } else {
        format!("**{}** has **{}** open positions.", name, positions_count)
    };

    let buttons = vec![
        serenity::CreateButton::new("liquidate-yes")
            .label("Liquidate & Delete")
            .style(poise::serenity_prelude::ButtonStyle::Danger),
        serenity::CreateButton::new("liquidate-no")
            .label("Cancel")
            .style(poise::serenity_prelude::ButtonStyle::Secondary),
    ];

    let reply = ctx
        .send(
            poise::CreateReply::default()
                .embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Delete")
                        .description(format!(
                            "{}\n\nLiquidate all positions at market price and return cash to wallet?",
                            detail
                        ))
                        .color(data::EMBED_FAIL)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                )
                .components(vec![serenity::CreateActionRow::Buttons(buttons)]),
        )
        .await?;

    let msg = reply.into_message().await?;
    let interaction = msg
        .await_component_interactions(ctx)
        .author_id(ctx.author().id)
        .timeout(Duration::from_secs(30))
        .await;

    match interaction {
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Portfolio — Delete")
                        .description("Timed out. Portfolio not deleted.")
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
        }
        Some(i) => {
            i.create_response(
                ctx.serenity_context(),
                serenity::CreateInteractionResponse::Acknowledge,
            )
            .await?;

            if i.data.custom_id == "liquidate-yes" {
                // Collect position info under read lock
                let data = &ctx.data().users;
                let u = data.get(&ctx.author().id).unwrap();
                let (portfolio_cash, positions_for_fetch) = {
                    let user_data = u.read().await;
                    match user_data
                        .stock
                        .portfolios
                        .iter()
                        .find(|p| p.name.eq_ignore_ascii_case(&name))
                    {
                        None => {
                            ctx.send(
                                poise::CreateReply::default().embed(
                                    serenity::CreateEmbed::new()
                                        .title("Portfolio — Delete")
                                        .description("Portfolio no longer exists.")
                                        .color(data::EMBED_ERROR),
                                ),
                            )
                            .await?;
                            return Ok(());
                        }
                        Some(p) => (p.cash, p.positions.clone()),
                    }
                    // read lock released
                };

                // Fetch prices (no locks held)
                let mut total_proceeds = portfolio_cash;
                for pos in &positions_for_fetch {
                    let price_usd = fetch_price(&pos.ticker).await.unwrap_or(0.0);
                    let value = match &pos.asset_type {
                        AssetType::Option(contract) => {
                            let intrinsic = match contract.option_type {
                                OptionType::Call => (price_usd - contract.strike).max(0.0),
                                OptionType::Put => (contract.strike - price_usd).max(0.0),
                            };
                            price_to_creds(intrinsic * 100.0) * contract.contracts as f64
                        }
                        _ => price_to_creds(price_usd) * pos.quantity,
                    };
                    total_proceeds += value;
                }

                // Apply under write lock
                {
                    let mut user_data = u.write().await;
                    user_data
                        .stock
                        .portfolios
                        .retain(|p| !p.name.eq_ignore_ascii_case(&name));
                    user_data.add_creds(total_proceeds as i32);
                }

                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Delete")
                            .description(format!(
                                "Portfolio **{}** liquidated and deleted.\n**{:.0}** creds returned to your wallet.",
                                name, total_proceeds
                            ))
                            .color(data::EMBED_SUCCESS)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await?;
            } else {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Delete")
                            .description("Cancelled. Portfolio not deleted.")
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
            }
        }
    }

    Ok(())
}

// ── Search ─────────────────────────────────────────────────────────────────────

/// Look up a stock, ETF, or crypto by ticker symbol
#[poise::command(slash_command)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Ticker symbol — comma-separated for multiple (e.g. NVDA, AAPL, BTC-USD)"]
    query: String,
) -> Result<(), Error> {
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
                            .description(format!("Could not resolve **{}**.", items[0]))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };
        let ticker = quote.symbol.clone();

        let price_usd = quote.regular_market_price.unwrap_or(0.0);
        let change = quote.regular_market_change.unwrap_or(0.0);
        let change_pct = quote.regular_market_change_percent.unwrap_or(0.0);
        let color = if change >= 0.0 {
            data::EMBED_SUCCESS
        } else {
            data::EMBED_FAIL
        };

        let mut desc = format!(
            "**${:.2}** / {:.0} creds\nDay change: **{:+.2} ({:+.2}%)**\n",
            price_usd,
            price_to_creds(price_usd),
            change,
            change_pct
        );
        if let Some(vol) = quote.regular_market_volume {
            desc += &format!("Volume: **{}**\n", format_large_num(vol as f64));
        }
        if let Some(mc) = quote.market_cap {
            desc += &format!("Market cap: **{}**\n", format_large_num(mc));
        }
        if let (Some(lo), Some(hi)) = (quote.fifty_two_week_low, quote.fifty_two_week_high) {
            desc += &format!("52-week: **${:.2} – ${:.2}**\n", lo, hi);
        }
        if let Some(pe) = quote.trailing_pe {
            desc += &format!("P/E ratio: **{:.2}**\n", pe);
        }

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(format!("{} — {}", ticker, quote.display_name()))
                    .description(desc)
                    .color(color)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    } else {
        // Compact multi-asset view (max 10)
        let mut rows = Vec::new();
        for q in items.into_iter().take(10) {
            let Some(quote) = resolve_ticker(q).await else {
                rows.push(format!("`{}` — could not resolve", q));
                continue;
            };
            let ticker = quote.symbol.clone();
            let price_usd = quote.regular_market_price.unwrap_or(0.0);
            let change_pct = quote.regular_market_change_percent.unwrap_or(0.0);
            let arrow = if change_pct >= 0.0 { "▲" } else { "▼" };
            rows.push(format!(
                "**{}** — {} | ${:.2} / {:.0}¢ | {} {:.2}%",
                ticker,
                quote.display_name(),
                price_usd,
                price_to_creds(price_usd),
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
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
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
    #[description = "Quantity to buy (fractional ok)"] quantity: f64,
    #[description = "Portfolio to buy into"] portfolio: String,
) -> Result<(), Error> {
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

    let quote = match resolve_ticker(&ticker_query).await {
        Some(q) => q,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(format!("Could not resolve **{}**.", ticker_query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();

    let price_usd = match quote.regular_market_price {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(format!("No price data available for **{}**.", ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let asset_type = match quote.quote_type.as_deref() {
        Some("ETF") => AssetType::ETF,
        Some("CRYPTOCURRENCY") => AssetType::Crypto,
        _ => AssetType::Stock,
    };

    let price_per_unit = price_to_creds(price_usd);
    let total_cost = price_per_unit * quantity;

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let remaining_cash = {
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
                            .title("Buy")
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
                        .title("Buy")
                        .description(format!(
                            "Insufficient cash. Need **{:.0}** creds but **{}** has **{:.0}**.",
                            total_cost, portfolio, port.cash
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        port.cash -= total_cost;

        // Weighted avg cost update
        if let Some(existing) = port.positions.iter_mut().find(|p| {
            p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_))
        }) {
            let total_qty = existing.quantity + quantity;
            existing.avg_cost =
                (existing.avg_cost * existing.quantity + price_per_unit * quantity) / total_qty;
            existing.quantity = total_qty;
        } else {
            port.positions.push(Position {
                ticker: ticker.clone(),
                asset_type,
                quantity,
                avg_cost: price_per_unit,
            });
        }

        port.cash
    };

    let record = TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: asset_name.clone(),
        action: TradeAction::Buy,
        quantity,
        price_per_unit,
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
                .title("Buy")
                .description(format!(
                    "Bought **{:.4} {}** ({}) for **{:.0}** creds\n${:.2}/unit | Portfolio **{}** cash: **{:.0}** creds",
                    quantity, ticker, asset_name, total_cost, price_usd, portfolio, remaining_cash
                ))
                .color(data::EMBED_SUCCESS)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

/// Sell a stock, ETF, or crypto
#[poise::command(slash_command)]
pub async fn sell(
    ctx: Context<'_>,
    #[description = "Ticker symbol (e.g. AAPL, BTC-USD)"] ticker_query: String,
    #[description = "Quantity to sell (fractional ok)"] quantity: f64,
    #[description = "Portfolio to sell from"] portfolio: String,
) -> Result<(), Error> {
    if quantity <= 0.0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Sell")
                    .description("Quantity must be greater than 0.")
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
                        .description(format!("Could not resolve **{}**.", ticker_query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();
    let price_usd = match quote.regular_market_price {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(format!("No price data available for **{}**.", ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let price_per_unit = price_to_creds(price_usd);

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let (proceeds, pnl) = {
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
                            .title("Sell")
                            .description(format!("No portfolio named **{}** found.", portfolio))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        let pos_idx = match port
            .positions
            .iter()
            .position(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)))
        {
            Some(i) => i,
            None => {
                ctx.send(
                    poise::CreateReply::default().embed(
                        serenity::CreateEmbed::new()
                            .title("Sell")
                            .description(format!(
                                "No **{}** position in portfolio **{}**.",
                                ticker, portfolio
                            ))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };

        if port.positions[pos_idx].quantity < quantity - 1e-9 {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(format!(
                            "You only hold **{:.4}** of **{}** but tried to sell **{:.4}**.",
                            port.positions[pos_idx].quantity, ticker, quantity
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        let avg_cost = port.positions[pos_idx].avg_cost;
        let proceeds = price_per_unit * quantity;
        let pnl = proceeds - avg_cost * quantity;

        port.cash += proceeds;
        port.positions[pos_idx].quantity -= quantity;
        if port.positions[pos_idx].quantity < 1e-9 {
            port.positions.remove(pos_idx);
        }

        (proceeds, pnl)
    };

    let record = TradeRecord {
        portfolio: portfolio.clone(),
        ticker: ticker.clone(),
        asset_name: asset_name.clone(),
        action: TradeAction::Sell,
        quantity,
        price_per_unit,
        total_creds: proceeds,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    };
    user_data.stock.trade_history.push_back(record);
    if user_data.stock.trade_history.len() > TRADE_HISTORY_LIMIT {
        user_data.stock.trade_history.pop_front();
    }

    let pnl_str = if pnl >= 0.0 {
        format!("▲ +{:.0}", pnl)
    } else {
        format!("▼ {:.0}", pnl)
    };
    let color = if pnl >= 0.0 {
        data::EMBED_SUCCESS
    } else {
        data::EMBED_FAIL
    };
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Sell")
                .description(format!(
                    "Sold **{:.4} {}** for **{:.0}** creds (${:.2}/unit)\nRealized P&L: **{}** creds",
                    quantity, ticker, proceeds, price_usd, pnl_str
                ))
                .color(color)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

// ── Watchlist ─────────────────────────────────────────────────────────────────

/// Manage your watchlist
#[poise::command(
    slash_command,
    subcommands("watchlist_add", "watchlist_remove", "watchlist_list")
)]
pub async fn watchlist(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Add a ticker or name to your watchlist
#[poise::command(slash_command, rename = "add")]
async fn watchlist_add(
    ctx: Context<'_>,
    #[description = "Ticker symbol to add"] query: String,
) -> Result<(), Error> {
    let quote = match resolve_ticker(&query).await {
        Some(q) => q,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Watchlist — Add")
                        .description(format!("Could not resolve **{}**.", query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let name = quote.display_name();

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();

    let add_result: Result<(), String> = {
        let mut user_data = u.write().await;
        if user_data.stock.watchlist.contains(&ticker) {
            Err(format!("**{}** is already on your watchlist.", ticker))
        } else if user_data.stock.watchlist.len() >= 20 {
            Err("Watchlist is full (max 20 tickers).".to_string())
        } else {
            user_data.stock.watchlist.push(ticker.clone());
            Ok(())
        }
        // write lock released here
    };

    match add_result {
        Err(msg) => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Watchlist — Add")
                        .description(msg)
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
        }
        Ok(()) => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Watchlist — Add")
                        .description(format!("Added **{} — {}** to your watchlist.", ticker, name))
                        .color(data::EMBED_SUCCESS)
                        .footer(serenity::CreateEmbedFooter::new(
                            "@~ powered by UwUntu & RustyBamboo",
                        )),
                ),
            )
            .await?;
        }
    }
    Ok(())
}

/// Remove a ticker from your watchlist
#[poise::command(slash_command, rename = "remove")]
async fn watchlist_remove(
    ctx: Context<'_>,
    #[description = "Ticker symbol to remove"] query: String,
) -> Result<(), Error> {
    // Try to resolve; fall back to uppercased input if resolution fails
    let ticker = resolve_ticker(&query)
        .await
        .map(|q| q.symbol)
        .unwrap_or_else(|| query.to_uppercase());

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();

    let removed = {
        let mut user_data = u.write().await;
        let before = user_data.stock.watchlist.len();
        user_data.stock.watchlist.retain(|t| t != &ticker);
        user_data.stock.watchlist.len() < before
        // write lock released here
    };

    if removed {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Watchlist — Remove")
                    .description(format!("Removed **{}** from your watchlist.", ticker))
                    .color(data::EMBED_SUCCESS)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Watchlist — Remove")
                    .description(format!("**{}** is not on your watchlist.", ticker))
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
    }
    Ok(())
}

/// View your watchlist with current prices
#[poise::command(slash_command, rename = "list")]
async fn watchlist_list(ctx: Context<'_>) -> Result<(), Error> {
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let tickers = {
        let user_data = u.read().await;
        if user_data.stock.watchlist.is_empty() {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Watchlist")
                        .description("Your watchlist is empty. Add assets with `/watchlist add <ticker>`.")
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
        user_data.stock.watchlist.clone()
        // read lock released
    };

    let mut rows = Vec::new();
    for ticker in &tickers {
        let Some(quote) = fetch_quote_detail(ticker).await else {
            rows.push(format!("`{}` — fetch failed", ticker));
            continue;
        };
        let price_usd = quote.regular_market_price.unwrap_or(0.0);
        let change_pct = quote.regular_market_change_percent.unwrap_or(0.0);
        let arrow = if change_pct >= 0.0 { "▲" } else { "▼" };
        rows.push(format!(
            "**{}** — {} | ${:.2} / {:.0}¢ | {} {:.2}%",
            ticker,
            quote.display_name(),
            price_usd,
            price_to_creds(price_usd),
            arrow,
            change_pct.abs()
        ));
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Watchlist")
                .description(rows.join("\n"))
                .color(data::EMBED_CYAN)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;
    Ok(())
}

// ── Trade History ─────────────────────────────────────────────────────────────

fn build_summary_embed(trades: &std::collections::VecDeque<TradeRecord>) -> serenity::CreateEmbed {
    let mut map: HashMap<&str, (f64, f64, u32)> = HashMap::new();
    for t in trades {
        let entry = map.entry(t.portfolio.as_str()).or_insert((0.0, 0.0, 0));
        entry.2 += 1;
        if let Some(pnl) = t.realized_pnl {
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
    let mut sorted: Vec<_> = map.iter().collect();
    sorted.sort_by_key(|(k, _)| *k);
    for (name, (gains, losses, count)) in sorted {
        let net = gains + losses;
        total_net += net;
        desc += &format!(
            "**{}** — {} trades | +{:.0} gains | {:.0} losses | Net: {:+.0}\n",
            name, count, gains, losses, net
        );
    }
    desc += &format!("\n**Total Net P&L: {:+.0} creds**", total_net);

    serenity::CreateEmbed::new()
        .title("Trade History — Summary")
        .description(desc)
        .color(data::EMBED_CYAN)
        .footer(serenity::CreateEmbedFooter::new(
            "@~ powered by UwUntu & RustyBamboo",
        ))
}

fn build_filtered_embed(trades: &std::collections::VecDeque<TradeRecord>, gains_only: bool) -> serenity::CreateEmbed {
    let title = if gains_only {
        "Trade History — Gains"
    } else {
        "Trade History — Losses"
    };
    let filtered: Vec<_> = trades
        .iter()
        .filter(|t| {
            t.realized_pnl
                .map(|p| if gains_only { p > 0.0 } else { p < 0.0 })
                .unwrap_or(false)
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
        desc += &format!(
            "{} **{}** [{}] × {:.4} | P&L: {:+.0}\n",
            t.timestamp.format("%m/%d"),
            t.ticker,
            t.portfolio,
            t.quantity,
            pnl
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
        .footer(serenity::CreateEmbedFooter::new(
            "@~ powered by UwUntu & RustyBamboo",
        ))
}

fn build_all_trades_embed(trades: &std::collections::VecDeque<TradeRecord>) -> serenity::CreateEmbed {
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
            .map(|p| format!(" | {:+.0}", p))
            .unwrap_or_default();
        desc += &format!(
            "{} `{}` **{}** × {:.4} — {:.0}¢{}\n",
            t.timestamp.format("%m/%d"),
            action,
            t.ticker,
            t.quantity,
            t.total_creds,
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
        .footer(serenity::CreateEmbedFooter::new(
            "@~ powered by UwUntu & RustyBamboo",
        ))
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
            .timeout(Duration::from_secs(5 * 60))
            .author_id(author_id)
            .stream();

        while let Some(interaction) = interactions.next().await {
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
                        .embed(embed)
                        .components(trade_buttons()),
                )
                .await
                .ok();
        }

        // Remove buttons on timeout
        msg.edit(&ctx_serenity, EditMessage::default().components(Vec::new()))
            .await
            .ok();
    });

    Ok(())
}

// ── Options ───────────────────────────────────────────────────────────────────

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
                        .description("Option type must be `call` or `put`.")
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
                        .description("Invalid expiry date. Use YYYY-MM-DD format.")
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
                    .description("Expiry date is in the past.")
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
                        .description(format!("Could not fetch price for **{}**.", ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = match opt_type {
        OptionType::Call => (price_usd - strike).max(0.0),
        OptionType::Put => (strike - price_usd).max(0.0),
    };
    let intrinsic_per_contract = intrinsic * 100.0;
    let intrinsic_creds = price_to_creds(intrinsic_per_contract);
    let itm = intrinsic > 0.0;
    let type_str = option_type_str(&opt_type);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Quote")
                .description(format!(
                    "**{} {} ${:.2}** exp {}\n\nUnderlying: **${:.2}**\nIntrinsic value: **${:.2}/contract** ({:.0} creds)\nStatus: **{}**",
                    ticker, type_str, strike, expiry,
                    price_usd, intrinsic_per_contract, intrinsic_creds,
                    if itm { "In The Money (ITM)" } else { "Out of The Money (OTM)" }
                ))
                .color(if itm { data::EMBED_SUCCESS } else { data::EMBED_ERROR })
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
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
                    .description("Contracts must be at least 1.")
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
                        .description("Option type must be `call` or `put`.")
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
                        .description("Invalid expiry date. Use YYYY-MM-DD format.")
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
                    .description("Expiry date is in the past.")
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
                        .description(format!("Could not fetch price for **{}**.", ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = match opt_type {
        OptionType::Call => (price_usd - strike).max(0.0),
        OptionType::Put => (strike - price_usd).max(0.0),
    };
    // OTM minimum: 1 cred/contract
    let cost_per_contract = price_to_creds(intrinsic * 100.0).max(1.0);
    let total_cost = cost_per_contract * contracts as f64;

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
                            "Insufficient cash. Need **{:.0}** creds but have **{:.0}**.",
                            total_cost, port.cash
                        ))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }

        port.cash -= total_cost;
        let quantity = contracts as f64;

        // Check for existing matching position
        let existing_idx = port.positions.iter().position(|p| {
            if p.ticker != ticker {
                return false;
            }
            if let AssetType::Option(c) = &p.asset_type {
                c.strike == strike && c.expiry == expiry_dt && c.option_type == opt_type
            } else {
                false
            }
        });

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
                    "Bought **{} {} ${:.2}** exp {} — {} contracts\nCost: **{:.0}** creds ({:.0}/contract)",
                    ticker, type_str, strike, expiry, contracts, total_cost, cost_per_contract
                ))
                .color(data::EMBED_SUCCESS)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
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
                    .description("Contracts must be at least 1.")
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
                        .description("Option type must be `call` or `put`.")
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
                        .description("Invalid expiry date. Use YYYY-MM-DD format.")
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
                        .description(format!("Could not fetch price for **{}**.", ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let intrinsic = match opt_type {
        OptionType::Call => (price_usd - strike).max(0.0),
        OptionType::Put => (strike - price_usd).max(0.0),
    };
    let proceeds_per_contract = price_to_creds(intrinsic * 100.0);
    let total_proceeds = proceeds_per_contract * contracts as f64;

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

        let pos_idx = match port.positions.iter().position(|p| {
            if p.ticker != ticker {
                return false;
            }
            if let AssetType::Option(c) = &p.asset_type {
                c.strike == strike && c.expiry == expiry_dt && c.option_type == opt_type
            } else {
                false
            }
        }) {
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

    let pnl_str = if pnl >= 0.0 {
        format!("▲ +{:.0}", pnl)
    } else {
        format!("▼ {:.0}", pnl)
    };
    let color = if pnl >= 0.0 {
        data::EMBED_SUCCESS
    } else {
        data::EMBED_FAIL
    };
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Options Sell")
                .description(format!(
                    "Sold **{} {} ${:.2}** exp {} — {} contracts\nProceeds: **{:.0}** creds | P&L: **{}** creds",
                    ticker, type_str, strike, expiry, contracts, total_proceeds, pnl_str
                ))
                .color(color)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}
