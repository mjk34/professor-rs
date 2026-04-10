//!---------------------------------------------------------------------!
//! HTTP infrastructure, market data helpers, and maintenance tasks     !
//!---------------------------------------------------------------------!

use crate::data::{
    self, AssetType, OptionContract, OptionSide, OrderSide, PendingOrder, TradeAction, TradeRecord,
};
use crate::helper::{creds_to_price, fmt_pnl, fmt_qty, option_intrinsic, option_type_str, price_to_creds};
use crate::serenity;
use chrono::{DateTime, Datelike, Timelike, Utc, Weekday};
use dashmap::DashMap;
use poise::serenity_prelude::{futures, ChannelId, CreateMessage};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub type UsersMap = Arc<DashMap<serenity::UserId, Arc<RwLock<crate::data::UserData>>>>;

// ── FMP API structs ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct FmpProfile {
    pub price: Option<f64>,
    #[serde(rename = "marketCap")]
    pub market_cap: Option<f64>,
    pub change: Option<f64>,
    #[serde(rename = "changePercentage")]
    pub change_percentage: Option<f64>,
    pub volume: Option<u64>,
    #[serde(rename = "companyName")]
    pub company_name: Option<String>,
    pub exchange: Option<String>,
    pub industry: Option<String>,
    pub sector: Option<String>,
    pub country: Option<String>,
    pub range: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FmpRatios {
    #[serde(rename = "priceToEarningsRatioTTM")]
    pub pe_ratio: Option<f64>,
}

// ── Yahoo Finance API structs ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct YfSearchResponse {
    pub quotes: Vec<YfSearchQuote>,
}

#[derive(Debug, Deserialize)]
pub struct YfSearchQuote {
    pub symbol: String,
}

#[derive(Debug, Deserialize)]
pub struct YfChartResponse {
    pub chart: YfChartOuter,
}

#[derive(Debug, Deserialize)]
pub struct YfChartOuter {
    pub result: Option<Vec<YfChartEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct YfChartEntry {
    pub meta: YfChartMeta,
}

#[derive(Debug, Deserialize)]
pub struct YfTradingSession {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Deserialize)]
pub struct YfCurrentTradingPeriod {
    pub regular: YfTradingSession,
}

#[derive(Debug, Deserialize)]
pub struct YfChartMeta {
    pub symbol: String,
    #[serde(rename = "longName")]
    pub long_name: Option<String>,
    #[serde(rename = "shortName")]
    pub short_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    pub regular_market_price: Option<f64>,
    #[serde(rename = "chartPreviousClose")]
    pub chart_previous_close: Option<f64>,
    #[serde(rename = "instrumentType")]
    pub instrument_type: Option<String>,
    #[serde(rename = "currentTradingPeriod")]
    pub current_trading_period: Option<YfCurrentTradingPeriod>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct YfQuote {
    pub symbol: String,
    #[serde(rename = "longName")]
    pub long_name: Option<String>,
    #[serde(rename = "shortName")]
    pub short_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    pub regular_market_price: Option<f64>,
    #[serde(rename = "regularMarketChangePercent")]
    pub regular_market_change_percent: Option<f64>,
    #[serde(rename = "quoteType")]
    pub quote_type: Option<String>,
    pub market_open: bool,
}

impl YfQuote {
    pub fn display_name(&self) -> String {
        self.long_name
            .clone()
            .or_else(|| self.short_name.clone())
            .unwrap_or_else(|| self.symbol.clone())
    }

    pub const fn is_market_open(&self) -> bool {
        self.market_open
    }

    pub fn asset_type(&self) -> AssetType {
        match self.quote_type.as_deref() {
            Some("ETF") => AssetType::ETF,
            Some("CRYPTOCURRENCY") => AssetType::Crypto,
            _ => AssetType::Stock,
        }
    }

    pub const fn market_status(&self) -> &'static str {
        if self.market_open { "Market: Open" } else { "Market: Closed" }
    }
}

// ── Market hours ─────────────────────────────────────────────────────────────

/// Eastern timezone offset in hours, read from `TZ_OFFSET_HOURS` env var. Defaults to -4 (EDT).
/// Set to -5 for EST (winter). Used to determine NYSE market hours.
pub static TZ_OFFSET: LazyLock<i64> = LazyLock::new(|| {
    std::env::var("TZ_OFFSET_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(-4)
});

pub fn is_market_hours() -> bool {
    let now_eastern = Utc::now() + chrono::Duration::hours(*TZ_OFFSET);
    let hour = now_eastern.hour();
    matches!(
        now_eastern.weekday(),
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
    ) && (9..16).contains(&hour)
}

// ── HTTP statics ──────────────────────────────────────────────────────────────

/// Shared HTTP client with a 10-second timeout and a browser-like user-agent (required by Yahoo Finance).
pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; professor-rs/1.0)")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

/// How long a Yahoo Finance quote is cached before re-fetching (60 seconds — balances freshness vs. rate limits).
const QUOTE_CACHE_TTL: Duration = Duration::from_secs(60);
pub static QUOTE_CACHE: LazyLock<DashMap<String, (YfQuote, Instant)>> =
    LazyLock::new(DashMap::new);

/// How long an FMP company profile is cached before re-fetching (5 minutes — profile data changes infrequently).
const FMP_CACHE_TTL: Duration = Duration::from_secs(300);
pub static FMP_CACHE: LazyLock<DashMap<String, (FmpProfile, Instant)>> =
    LazyLock::new(DashMap::new);

/// How long FMP valuation ratios are cached before re-fetching (15 days — rarely changes).
const FMP_RATIOS_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 15);
pub static FMP_RATIOS_CACHE: LazyLock<DashMap<String, (FmpRatios, Instant)>> =
    LazyLock::new(DashMap::new);

pub static LOGO_API_KEY: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("LOGO_API_KEY").ok());
pub static FMP_API_KEY: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("FMP_API_KEY").ok());
pub static FRED_API_KEY: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("FRED_API_KEY").ok());

/// Set to true when Yahoo Finance returns HTTP 429; cleared on next successful response.
pub static YAHOO_RATE_LIMITED: AtomicBool = AtomicBool::new(false);

// ── Logo / embed helpers ──────────────────────────────────────────────────────

pub fn logo_url(ticker: &str) -> Option<String> {
    let key = LOGO_API_KEY.as_deref()?;
    Some(format!("https://img.logo.dev/ticker/{ticker}?token={key}"))
}


/// Returns a user-facing description when market data can't be fetched.
/// If the Yahoo Finance rate limit was recently hit, tells the user to try again tomorrow.
pub fn market_data_err(query: &str) -> String {
    if YAHOO_RATE_LIMITED.load(Ordering::Relaxed) {
        "Market data API is rate limited — try again tomorrow when the limit resets.".to_string()
    } else {
        format!("Could not fetch market data for **{query}**.")
    }
}

pub fn with_logo(embed: serenity::CreateEmbed, ticker: &str) -> serenity::CreateEmbed {
    match logo_url(ticker) {
        Some(url) => embed.thumbnail(url),
        None => embed,
    }
}

pub fn looks_like_ticker(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && (s.len() <= 4 || s.contains('-') || s.contains('.'))
}

// ── Ticker resolution ─────────────────────────────────────────────────────────

pub async fn resolve_ticker(query: &str) -> Option<YfQuote> {
    let upper = query.trim().to_uppercase();

    let ticker = if looks_like_ticker(&upper) {
        upper
    } else {
        async {
            let resp = HTTP_CLIENT
                .get("https://query1.finance.yahoo.com/v1/finance/search")
                .query(&[("q", query.trim()), ("quotesCount", "1"), ("newsCount", "0")])
                .send()
                .await
                .ok()?
                .json::<YfSearchResponse>()
                .await
                .ok()?;
            resp.quotes.into_iter().next().map(|q| q.symbol)
        }
        .await
        .unwrap_or(upper)
    };

    fetch_quote_detail(&ticker).await
}

pub async fn fetch_price(ticker: &str) -> Option<f64> {
    fetch_quote_detail(ticker)
        .await
        .and_then(|q| q.regular_market_price)
}

pub async fn fetch_quote_detail(ticker: &str) -> Option<YfQuote> {
    // Validate ticker before interpolating into URL — guards against SSRF from user or AI input
    if ticker.is_empty() || ticker.len() > 20 || !ticker.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        tracing::warn!(ticker = ?ticker, "fetch_quote_detail: rejected invalid ticker");
        return None;
    }

    if let Some(entry) = QUOTE_CACHE.get(ticker) {
        if !is_market_hours() || entry.1.elapsed() < QUOTE_CACHE_TTL {
            return Some(entry.0.clone());
        }
    }

    let http_resp = HTTP_CLIENT
        .get(format!(
            "https://query2.finance.yahoo.com/v8/finance/chart/{ticker}"
        ))
        .query(&[("interval", "1d"), ("range", "1d")])
        .send()
        .await
        .ok()?;

    if http_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        tracing::warn!(ticker = %ticker, "Yahoo Finance rate limit hit (429)");
        YAHOO_RATE_LIMITED.store(true, Ordering::Relaxed);
        return None;
    }

    let resp = http_resp.json::<YfChartResponse>().await.ok()?;
    YAHOO_RATE_LIMITED.store(false, Ordering::Relaxed);

    let meta = resp.chart.result?.into_iter().next()?.meta;

    let price_prev = meta.regular_market_price.zip(meta.chart_previous_close);
    let change_pct = price_prev.map(|(p, c)| (p - c) / c * 100.0);

    let market_open = meta
        .current_trading_period
        .is_some_and(|p| {
            let now = Utc::now().timestamp();
            now >= p.regular.start && now < p.regular.end
        });

    let quote = YfQuote {
        symbol: meta.symbol,
        long_name: meta.long_name,
        short_name: meta.short_name,
        regular_market_price: meta.regular_market_price,
        regular_market_change_percent: change_pct,
        quote_type: meta.instrument_type,
        market_open,
    };
    QUOTE_CACHE.insert(quote.symbol.clone(), (quote.clone(), Instant::now()));
    Some(quote)
}

pub async fn fetch_fmp_profile(ticker: &str) -> Option<FmpProfile> {
    if let Some(entry) = FMP_CACHE.get(ticker) {
        if !is_market_hours() || entry.1.elapsed() < FMP_CACHE_TTL {
            return Some(entry.0.clone());
        }
    }

    let api_key = FMP_API_KEY.as_deref()?;
    let mut profiles = HTTP_CLIENT
        .get(format!(
            "https://financialmodelingprep.com/stable/profile?symbol={ticker}&apikey={api_key}"
        ))
        .send()
        .await
        .ok()?
        .json::<Vec<FmpProfile>>()
        .await
        .ok()?;

    let profile = profiles.pop()?;
    FMP_CACHE.insert(ticker.to_string(), (profile.clone(), Instant::now()));
    Some(profile)
}

pub async fn fetch_fmp_ratios(ticker: &str) -> Option<FmpRatios> {
    if let Some(entry) = FMP_RATIOS_CACHE.get(ticker) {
        if !is_market_hours() || entry.1.elapsed() < FMP_RATIOS_CACHE_TTL {
            return Some(entry.0.clone());
        }
    }

    let api_key = FMP_API_KEY.as_deref()?;
    let mut list = HTTP_CLIENT
        .get(format!(
            "https://financialmodelingprep.com/stable/ratios-ttm?symbol={ticker}&apikey={api_key}"
        ))
        .send()
        .await
        .ok()?
        .json::<Vec<FmpRatios>>()
        .await
        .ok()?;

    let ratios = list.pop()?;
    FMP_RATIOS_CACHE.insert(ticker.to_string(), (ratios.clone(), Instant::now()));
    Some(ratios)
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

pub async fn fetch_fed_funds_rate() -> Option<f64> {
    let api_key = FRED_API_KEY.as_deref()?;
    let resp = HTTP_CLIENT
        .get("https://api.stlouisfed.org/fred/series/observations")
        .query(&[
            ("series_id", "DFF"),
            ("api_key", api_key),
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

// ── API health checks ─────────────────────────────────────────────────────────

pub async fn api_health_check() {
    // FRED
    if let Some(r) = fetch_fed_funds_rate().await { tracing::info!(rate = r, "[API] FRED ✓") } else { tracing::warn!("[API] FRED ✗ — failed to fetch fed funds rate") }

    // FMP — probe with a known ticker
    if let Some(p) = fetch_fmp_profile("SPY").await { tracing::info!(spy_price = p.price.unwrap_or(0.0), "[API] FMP ✓") } else { tracing::warn!("[API] FMP ✗ — failed to fetch SPY profile") }

    // FINNHUB — fetch market news
    let news = crate::professor::fetch_market_news().await;
    if news.is_empty() {
        tracing::warn!("[API] FINNHUB ✗ — no headlines returned (check key or rate limit)");
    } else {
        tracing::info!(count = news.len(), "[API] FINNHUB ✓ — headlines fetched");
    }

    // CLAUDE — key presence only (no paid call on startup)
    if std::env::var("CLAUDE_API_KEY").is_ok() { tracing::info!("[API] CLAUDE ✓ — key present") } else { tracing::warn!("[API] CLAUDE ✗ — CLAUDE_API_KEY not set") }

    // LOGO.DEV — key presence only (URL-embedded, no HTTP call needed)
    if std::env::var("LOGO_API_KEY").is_ok() { tracing::info!("[API] LOGO ✓ — key present") } else { tracing::warn!("[API] LOGO ✗ — LOGO_API_KEY not set (stock thumbnails disabled)") }
}

// ── Maintenance functions ─────────────────────────────────────────────────────

pub async fn refresh_market_rate(rate: &Arc<RwLock<f64>>) {
    if let Some(r_val) = fetch_fed_funds_rate().await {
        let mut r = rate.write().await;
        *r = r_val;
        tracing::info!(rate = r_val, "HYSA fed rate updated");
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
        let annual_rate = if crate::helper::is_gold(&user_data) {
            crate::helper::gold_hysa_rate(fed_rate)
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
    }

    // Phase 2: fetch prices for unique tickers concurrently (no locks held)
    let unique_tickers: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        to_process.iter().filter_map(|i| if seen.insert(i.ticker.as_str()) { Some(i.ticker.clone()) } else { None }).collect()
    };
    let prices = fetch_prices_map(&unique_tickers).await;

    // Phase 3: apply changes under write lock (no await while holding)
    for info in to_process {
        let u = match users.get(&info.user_id) {
            Some(u) => u,
            None => continue,
        };

        let price_usd = *prices.get(&info.ticker).unwrap_or(&0.0);
        let intrinsic = option_intrinsic(&info.contract.option_type, price_usd, info.contract.strike);
        let intrinsic_creds =
            price_to_creds(intrinsic * f64::from(info.contract.contracts) * 100.0);
        let cost_basis = info.avg_cost * info.quantity; // for long: cost paid; for short: premium received
        let itm = intrinsic > 0.0;
        let type_str = option_type_str(&info.contract.option_type);
        let is_short = info.contract.side == OptionSide::Short;

        let (cash_delta, pnl, msg) = if is_short {
            // Writer: premium already collected upfront; now settle obligation
            let obligation = intrinsic_creds; // amount owed if ITM
            let pnl = cost_basis - obligation; // premium - obligation
            let msg = if itm {
                format!(
                    "<@{}> Options expired **ITM** — SHORT **{}** {} | Paid **${:.2}** obligation (P&L: **${:+.2}**)",
                    info.user_id, info.ticker, type_str,
                    creds_to_price(obligation), creds_to_price(pnl)
                )
            } else {
                format!(
                    "<@{}> Options expired **OTM** (worthless) — SHORT **{}** {} | Kept **${:.2}** premium",
                    info.user_id, info.ticker, type_str, creds_to_price(cost_basis)
                )
            };
            (-obligation, pnl, msg) // debit the obligation; premium was already in cash
        } else {
            // Buyer: receive intrinsic if ITM, lose cost_basis if OTM
            let pnl = intrinsic_creds - cost_basis;
            let msg = if itm {
                format!(
                    "<@{}> Options expired **ITM** — **{}** {} | Received **${:.2}** (P&L: **${:+.2}**)",
                    info.user_id, info.ticker, type_str,
                    creds_to_price(intrinsic_creds), creds_to_price(pnl)
                )
            } else {
                format!(
                    "<@{}> Options expired **OTM** (worthless) — **{}** {} | Lost **${:.2}**",
                    info.user_id, info.ticker, type_str, creds_to_price(cost_basis)
                )
            };
            (intrinsic_creds, pnl, msg)
        };

        {
            let mut user_data = u.write().await;
            if let Some(portfolio) = user_data
                .stock
                .portfolios
                .iter_mut()
                .find(|p| p.name == info.portfolio_name)
            {
                portfolio.cash += cash_delta;
                portfolio.positions.retain(|p| {
                    if p.ticker != info.ticker {
                        return true;
                    }
                    #[expect(clippy::float_cmp, reason = "strike prices are stored/compared as exact values we set")]
                    if let AssetType::Option(c) = &p.asset_type {
                        !(c.strike == info.contract.strike
                            && c.expiry == info.contract.expiry
                            && c.option_type == info.contract.option_type
                            && c.side == info.contract.side)
                    } else {
                        true
                    }
                });
            }

            let record = TradeRecord {
                portfolio: info.portfolio_name.clone(),
                ticker: info.ticker.clone(),
                asset_name: format!(
                    "{}{} {} ${:.2} {}",
                    if is_short { "SHORT " } else { "" },
                    info.ticker,
                    type_str,
                    info.contract.strike,
                    info.contract.expiry.format("%Y-%m-%d")
                ),
                action: TradeAction::Sell,
                quantity: info.quantity,
                price_per_unit: intrinsic_creds / info.quantity.max(1.0),
                total_creds: intrinsic_creds,
                realized_pnl: Some(pnl),
                timestamp: now,
            };
            user_data.stock.push_trade(record);
        }

        let _ = channel
            .send_message(http, CreateMessage::new().content(msg))
            .await;
    }
}

/// Fetches prices for a list of tickers concurrently and returns a ticker → USD price map.
/// Tickers that fail to fetch are included with a value of 0.0.
pub async fn fetch_prices_map(tickers: &[String]) -> HashMap<String, f64> {
    futures::future::join_all(
        tickers.iter().map(|t| { let t = t.clone(); async move { let p = fetch_price(&t).await.unwrap_or(0.0); (t, p) } })
    ).await.into_iter().collect()
}

pub async fn is_market_open() -> bool {
    fetch_quote_detail("SPY").await
        .is_some_and(|q| q.is_market_open())
}

/// Returns the expiry for a new pending order: end of today at 20:00 UTC if market
/// hasn't closed yet, otherwise end of the next weekday at 20:00 UTC.
pub fn order_expiry() -> DateTime<Utc> {
    let now = Utc::now();
    let today_close = now
        .date_naive()
        .and_hms_opt(20, 0, 0)
        .unwrap()
        .and_utc();

    // Use today if market hasn't closed yet (before 20:00 UTC on a weekday)
    if now < today_close && !matches!(now.weekday(), Weekday::Sat | Weekday::Sun) {
        return today_close;
    }

    // Walk forward to find the next weekday
    let mut day = now.date_naive() + chrono::Duration::days(1);
    loop {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            break;
        }
        day += chrono::Duration::days(1);
    }
    day.and_hms_opt(20, 0, 0).unwrap().and_utc()
}

/// Sweep pending orders: execute those whose conditions are met, expire stale ones.
pub async fn sweep_pending_orders(
    users: &UsersMap,
    http: &Arc<serenity::Http>,
    bot_chat: &str,
) {
    let channel = ChannelId::new(
        bot_chat.parse::<u64>().expect("bot_chat must be a valid u64"),
    );
    let now = Utc::now();

    // ── Phase 1: snapshot eligible orders (no await inside lock) ─────────────
    struct OrderSnapshot {
        user_id: serenity::UserId,
        order: PendingOrder,
    }
    let mut snapshots: Vec<OrderSnapshot> = Vec::new();

    for entry in users.iter() {
        let user_id = *entry.key();
        let guard = entry.value().read().await;
        for order in &guard.stock.pending_orders {
            snapshots.push(OrderSnapshot { user_id, order: order.clone() });
        }
    }

    if snapshots.is_empty() {
        return;
    }

    // ── Phase 2: fetch prices concurrently (no locks held) ──────────────────
    let unique_tickers: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        snapshots.iter().filter_map(|s| if seen.insert(s.order.ticker.as_str()) { Some(s.order.ticker.clone()) } else { None }).collect()
    };

    let mut prices = fetch_prices_map(&unique_tickers).await;
    prices.retain(|_, p| *p > 0.0);

    // ── Phase 3: execute or expire under write lock ──────────────────────────
    for snap in &snapshots {
        let price_usd = match prices.get(&snap.order.ticker) {
            Some(&p) => p,
            None => continue, // can't price it, skip this cycle
        };

        let expired = now >= snap.order.expiry;
        let triggered = match snap.order.side {
            OrderSide::Buy => match snap.order.limit_price {
                Some(lp) => price_usd <= lp,
                None => true, // market order — execute at open
            },
            OrderSide::Sell => match snap.order.limit_price {
                Some(lp) => price_usd >= lp,
                None => true,
            },
        };

        if !triggered && !expired {
            continue;
        }

        let entry = match users.get(&snap.user_id) {
            Some(e) => e,
            None => continue,
        };
        let mut user_data = entry.value().write().await;

        // Confirm the order is still present (could have been cancelled)
        let order_idx = match user_data.stock.pending_orders.iter().position(|o| o.id == snap.order.id) {
            Some(i) => i,
            None => continue,
        };

        if expired {
            user_data.stock.pending_orders.remove(order_idx);
            drop(user_data);
            let msg = format!(
                "<@{}> Your **{} {}** order (#{}) expired.",
                snap.user_id, snap.order.side.label(), snap.order.ticker, snap.order.id,
            );
            let _ = channel.send_message(http, CreateMessage::new().content(msg)).await;
            continue;
        }

        // Triggered — execute
        let order = user_data.stock.pending_orders.remove(order_idx);
        let price_per_unit = price_to_creds(price_usd);

        let msg = match order.side {
            OrderSide::Buy => {
                let total_cost = price_per_unit * order.quantity;
                let port_idx = user_data.stock.portfolios.iter().position(|p| p.name == order.portfolio_name);
                match port_idx {
                    Some(idx) if user_data.stock.portfolios[idx].cash >= total_cost => {
                        let stock = &mut user_data.stock;
                        crate::trader::apply_buy(
                            &mut stock.portfolios[idx],
                            &mut stock.trade_history,
                            &order.ticker,
                            &order.asset_name,
                            order.asset_type.clone(),
                            order.quantity,
                            price_per_unit,
                            total_cost,
                            &order.portfolio_name,
                        );
                        format!(
                            "<@{}> Limit buy filled: **{} {}** @ **${:.2}**/unit (${:.2} total) in **{}**.",
                            snap.user_id, fmt_qty(order.quantity), order.ticker, price_usd,
                            creds_to_price(total_cost), order.portfolio_name,
                        )
                    }
                    Some(_) => {
                        format!(
                            "<@{}> Limit buy **{}** (#{}) cancelled — insufficient cash in **{}**.",
                            snap.user_id, order.ticker, order.id, order.portfolio_name,
                        )
                    }
                    None => {
                        format!(
                            "<@{}> Limit buy **{}** (#{}) cancelled — portfolio **{}** not found.",
                            snap.user_id, order.ticker, order.id, order.portfolio_name,
                        )
                    }
                }
            }
            OrderSide::Sell => {
                let port_idx = user_data.stock.portfolios.iter().position(|p| p.name == order.portfolio_name);
                match port_idx {
                    Some(idx) => {
                        let held = user_data.stock.portfolios[idx].positions.iter()
                            .find(|p| p.ticker == order.ticker && !matches!(&p.asset_type, AssetType::Option(_)))
                            .map_or(0.0, |p| p.quantity);
                        let qty = if (order.quantity - held).abs() < 5e-5 { held } else { order.quantity };
                        if held < qty - 1e-9 {
                            format!(
                                "<@{}> Limit sell **{}** (#{}) cancelled — only hold {} but order was for {}.",
                                snap.user_id, order.ticker, order.id, fmt_qty(held), fmt_qty(qty),
                            )
                        } else {
                            let proceeds = price_per_unit * qty;
                            let stock = &mut user_data.stock;
                            let pnl = crate::trader::apply_sell(
                                &mut stock.portfolios[idx],
                                &mut stock.trade_history,
                                &order.ticker,
                                &order.asset_name,
                                qty,
                                price_per_unit,
                                &order.portfolio_name,
                            ).unwrap_or(0.0);
                            format!(
                                "<@{}> Limit sell filled: **{} {}** @ **${:.2}**/unit (${:.2}) | P&L: **{}** | Portfolio: **{}**.",
                                snap.user_id, fmt_qty(qty), order.ticker, price_usd,
                                creds_to_price(proceeds), fmt_pnl(pnl), order.portfolio_name,
                            )
                        }
                    }
                    None => {
                        format!(
                            "<@{}> Limit sell **{}** (#{}) cancelled — portfolio **{}** not found.",
                            snap.user_id, order.ticker, order.id, order.portfolio_name,
                        )
                    }
                }
            }
        };

        drop(user_data);
        let _ = channel.send_message(http, CreateMessage::new().content(msg)).await;
    }
}

impl OrderSide {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}
