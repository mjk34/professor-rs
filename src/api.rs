//!---------------------------------------------------------------------!
//! HTTP infrastructure, market data helpers, and maintenance tasks     !
//!---------------------------------------------------------------------!

use crate::data::{
    self, AssetType, OptionContract, OptionSide, TradeAction, TradeRecord,
    TRADE_HISTORY_LIMIT,
};
use crate::helper::{creds_to_price, option_intrinsic, option_type_str, price_to_creds};
use crate::serenity;
use chrono::{Datelike, Timelike, Utc, Weekday};
use dashmap::DashMap;
use poise::serenity_prelude::{futures, ChannelId, CreateMessage};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub type UsersMap = Arc<DashMap<serenity::UserId, Arc<RwLock<crate::data::UserData>>>>;

// ── FMP API structs ───────────────────────────────────────────────────────────

#[derive(Deserialize, Clone)]
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

#[derive(Deserialize, Clone)]
pub struct FmpRatios {
    #[serde(rename = "priceToEarningsRatioTTM")]
    pub pe_ratio: Option<f64>,
}

// ── Yahoo Finance API structs ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct YfSearchResponse {
    pub quotes: Vec<YfSearchQuote>,
}

#[derive(Deserialize)]
pub struct YfSearchQuote {
    pub symbol: String,
}

#[derive(Deserialize)]
pub struct YfChartResponse {
    pub chart: YfChartOuter,
}

#[derive(Deserialize)]
pub struct YfChartOuter {
    pub result: Option<Vec<YfChartEntry>>,
}

#[derive(Deserialize)]
pub struct YfChartEntry {
    pub meta: YfChartMeta,
}

#[derive(Deserialize)]
pub struct YfTradingSession {
    pub start: i64,
    pub end: i64,
}

#[derive(Deserialize)]
pub struct YfCurrentTradingPeriod {
    pub regular: YfTradingSession,
}

#[derive(Deserialize)]
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

#[derive(Deserialize, Clone)]
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

    pub fn is_market_open(&self) -> bool {
        self.market_open
    }

    pub fn asset_type(&self) -> AssetType {
        match self.quote_type.as_deref() {
            Some("ETF") => AssetType::ETF,
            Some("CRYPTOCURRENCY") => AssetType::Crypto,
            _ => AssetType::Stock,
        }
    }

    pub fn market_status(&self) -> &'static str {
        if self.market_open { "Market: Open" } else { "Market: Closed" }
    }
}

// ── Market hours ─────────────────────────────────────────────────────────────

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
    ) && hour >= 9 && hour < 16
}

// ── HTTP statics ──────────────────────────────────────────────────────────────

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; professor-rs/1.0)")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

const QUOTE_CACHE_TTL: Duration = Duration::from_secs(60);
pub static QUOTE_CACHE: LazyLock<DashMap<String, (YfQuote, Instant)>> =
    LazyLock::new(DashMap::new);

const FMP_CACHE_TTL: Duration = Duration::from_secs(300);
pub static FMP_CACHE: LazyLock<DashMap<String, (FmpProfile, Instant)>> =
    LazyLock::new(DashMap::new);

const FMP_RATIOS_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 15);
pub static FMP_RATIOS_CACHE: LazyLock<DashMap<String, (FmpRatios, Instant)>> =
    LazyLock::new(DashMap::new);

pub static LOGO_API_KEY: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("LOGO_API_KEY").ok());

/// Set to true when Yahoo Finance returns HTTP 429; cleared on next successful response.
pub static YAHOO_RATE_LIMITED: AtomicBool = AtomicBool::new(false);

// ── Logo / embed helpers ──────────────────────────────────────────────────────

pub fn logo_url(ticker: &str) -> Option<String> {
    let key = LOGO_API_KEY.as_deref()?;
    Some(format!("https://img.logo.dev/ticker/{}?token={}", ticker, key))
}

pub fn market_closed_reply(action: &str, ticker: &str) -> poise::CreateReply {
    poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title(action)
            .description(format!("Market is currently closed for **{}**.", ticker))
            .color(data::EMBED_ERROR),
    )
}

/// Returns a user-facing description when market data can't be fetched.
/// If the Yahoo Finance rate limit was recently hit, tells the user to try again tomorrow.
pub fn market_data_err(query: &str) -> String {
    if YAHOO_RATE_LIMITED.load(Ordering::Relaxed) {
        "Market data API is rate limited — try again tomorrow when the limit resets.".to_string()
    } else {
        format!("Could not fetch market data for **{}**.", query)
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
        tracing::warn!("fetch_quote_detail: rejected invalid ticker {:?}", ticker);
        return None;
    }

    if let Some(entry) = QUOTE_CACHE.get(ticker) {
        if !is_market_hours() || entry.1.elapsed() < QUOTE_CACHE_TTL {
            return Some(entry.0.clone());
        }
    }

    let http_resp = HTTP_CLIENT
        .get(format!(
            "https://query2.finance.yahoo.com/v8/finance/chart/{}",
            ticker
        ))
        .query(&[("interval", "1d"), ("range", "1d")])
        .send()
        .await
        .ok()?;

    if http_resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        tracing::warn!("Yahoo Finance rate limit hit (429) for ticker {ticker}");
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
        .map(|p| {
            let now = Utc::now().timestamp();
            now >= p.regular.start && now < p.regular.end
        })
        .unwrap_or(false);

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

    let api_key = std::env::var("FMP_API_KEY").ok()?;
    let mut profiles = HTTP_CLIENT
        .get(format!(
            "https://financialmodelingprep.com/stable/profile?symbol={}&apikey={}",
            ticker, api_key
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

    let api_key = std::env::var("FMP_API_KEY").ok()?;
    let mut list = HTTP_CLIENT
        .get(format!(
            "https://financialmodelingprep.com/stable/ratios-ttm?symbol={}&apikey={}",
            ticker, api_key
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

// ── API health checks ─────────────────────────────────────────────────────────

pub async fn api_health_check() {
    // FRED
    match fetch_fed_funds_rate().await {
        Some(r) => tracing::info!("[API] FRED ✓ — fed funds rate: {:.2}%", r),
        None    => tracing::warn!("[API] FRED ✗ — failed to fetch fed funds rate"),
    }

    // FMP — probe with a known ticker
    match fetch_fmp_profile("SPY").await {
        Some(p) => tracing::info!("[API] FMP ✓ — SPY price: ${:.2}", p.price.unwrap_or(0.0)),
        None    => tracing::warn!("[API] FMP ✗ — failed to fetch SPY profile"),
    }

    // FINNHUB — fetch market news
    let news = crate::professor::fetch_market_news().await;
    if !news.is_empty() {
        tracing::info!("[API] FINNHUB ✓ — {} headlines fetched", news.len());
    } else {
        tracing::warn!("[API] FINNHUB ✗ — no headlines returned (check key or rate limit)");
    }

    // CLAUDE — key presence only (no paid call on startup)
    match std::env::var("CLAUDE_API_KEY") {
        Ok(_)  => tracing::info!("[API] CLAUDE ✓ — key present"),
        Err(_) => tracing::warn!("[API] CLAUDE ✗ — CLAUDE_API_KEY not set"),
    }

    // LOGO.DEV — key presence only (URL-embedded, no HTTP call needed)
    match std::env::var("LOGO_API_KEY") {
        Ok(_)  => tracing::info!("[API] LOGO ✓ — key present"),
        Err(_) => tracing::warn!("[API] LOGO ✗ — LOGO_API_KEY not set (stock thumbnails disabled)"),
    }
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
        // read lock released here
    }

    // Phase 2: fetch prices for unique tickers concurrently (no locks held)
    let unique_tickers: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        to_process.iter().map(|i| i.ticker.clone()).filter(|t| seen.insert(t.clone())).collect()
    };
    let prices: HashMap<String, f64> = futures::future::join_all(
        unique_tickers.iter().map(|t| { let t = t.clone(); async move { let p = fetch_price(&t).await.unwrap_or(0.0); (t, p) } })
    ).await.into_iter().collect();

    // Phase 3: apply changes under write lock (no await while holding)
    for info in to_process {
        let u = match users.get(&info.user_id) {
            Some(u) => u,
            None => continue,
        };

        let price_usd = *prices.get(&info.ticker).unwrap_or(&0.0);
        let intrinsic = option_intrinsic(&info.contract.option_type, price_usd, info.contract.strike);
        let intrinsic_creds =
            price_to_creds(intrinsic * info.contract.contracts as f64 * 100.0);
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

pub async fn is_market_open() -> bool {
    fetch_quote_detail("SPY").await
        .and_then(|q| Some(q.is_market_open()))
        .unwrap_or(false)
}
