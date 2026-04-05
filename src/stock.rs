//!---------------------------------------------------------------------!
//! Stock / portfolio trading commands                                   !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - portfolio (create, list, view, fund, withdraw, delete)    !
//!     [x] - search                                                    !
//!     [x] - buy / sell                                                !
//!     [x] - watchlist (add, remove, list)                             !
//!     [x] - trades                                                    !
//!     [x] - options (quote, buy, sell, write, cover)                  !
//!---------------------------------------------------------------------!

use crate::data::{
    self, AssetType, MemoryEntry, OptionContract, OptionSide, OptionType, Portfolio, Position,
    ProfessorMemory, TradeAction, TradeRecord, UserData, GOLD_LEVEL_THRESHOLD, TRADE_HISTORY_LIMIT,
};
use crate::{serenity, Context, Error};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use dashmap::DashMap;
use poise::serenity_prelude::{futures, futures::StreamExt, ChannelId, CreateMessage, EditMessage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock, atomic::{AtomicBool, Ordering}};
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

fn fmt_qty(q: f64) -> String {
    if q.fract() == 0.0 {
        format!("{:.0}", q)
    } else {
        format!("{:.4}", q)
    }
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

// ── FMP API structs ───────────────────────────────────────────────────────────

#[derive(Deserialize, Clone)]
struct FmpProfile {
    price: Option<f64>,
    #[serde(rename = "marketCap")]
    market_cap: Option<f64>,
    change: Option<f64>,
    #[serde(rename = "changePercentage")]
    change_percentage: Option<f64>,
    volume: Option<u64>,
    #[serde(rename = "companyName")]
    company_name: Option<String>,
    exchange: Option<String>,
    industry: Option<String>,
    sector: Option<String>,
    country: Option<String>,
    range: Option<String>,
}

#[derive(Deserialize, Clone)]
struct FmpRatios {
    #[serde(rename = "priceToEarningsRatioTTM")]
    pe_ratio: Option<f64>,
}

// ── Yahoo Finance API structs ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct YfSearchResponse {
    quotes: Vec<YfSearchQuote>,
}

#[derive(Deserialize)]
struct YfSearchQuote {
    symbol: String,
}

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
struct YfTradingSession {
    start: i64,
    end: i64,
}

#[derive(Deserialize)]
struct YfCurrentTradingPeriod {
    regular: YfTradingSession,
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
    #[serde(rename = "instrumentType")]
    instrument_type: Option<String>,
    #[serde(rename = "currentTradingPeriod")]
    current_trading_period: Option<YfCurrentTradingPeriod>,
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
    #[serde(rename = "regularMarketChangePercent")]
    regular_market_change_percent: Option<f64>,
    #[serde(rename = "quoteType")]
    quote_type: Option<String>,
    market_open: bool,
}

impl YfQuote {
    fn display_name(&self) -> String {
        self.long_name
            .clone()
            .or_else(|| self.short_name.clone())
            .unwrap_or_else(|| self.symbol.clone())
    }

    fn is_market_open(&self) -> bool {
        self.market_open
    }

    fn asset_type(&self) -> AssetType {
        match self.quote_type.as_deref() {
            Some("ETF") => AssetType::ETF,
            Some("CRYPTOCURRENCY") => AssetType::Crypto,
            _ => AssetType::Stock,
        }
    }

    fn market_status(&self) -> &'static str {
        if self.market_open { "Market: Open" } else { "Market: Closed" }
    }
}

// ── Market hours ─────────────────────────────────────────────────────────────

static TZ_OFFSET: LazyLock<i64> = LazyLock::new(|| {
    std::env::var("TZ_OFFSET_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(-4)
});

fn is_market_hours() -> bool {
    let now_eastern = Utc::now() + chrono::Duration::hours(*TZ_OFFSET);
    let hour = now_eastern.hour();
    matches!(
        now_eastern.weekday(),
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
    ) && hour >= 9 && hour < 17
}

fn fmt_pct_change(value: f64, basis: f64) -> String {
    if basis > 0.0 {
        format!(" ({:+.1}%)", value / basis * 100.0)
    } else {
        String::new()
    }
}

fn option_intrinsic(opt_type: &OptionType, price_usd: f64, strike: f64) -> f64 {
    match opt_type {
        OptionType::Call => (price_usd - strike).max(0.0),
        OptionType::Put => (strike - price_usd).max(0.0),
    }
}

/// Option premium in creds: intrinsic + $0.05/DTE time value, minimum $0.01/contract.
fn option_premium_creds(intrinsic_usd: f64, expiry: &DateTime<Utc>, contracts: u32) -> f64 {
    let dte = (*expiry - Utc::now()).num_days().max(0) as f64;
    let per_contract_usd = (intrinsic_usd + dte * 0.05).max(0.01);
    price_to_creds(per_contract_usd * contracts as f64 * 100.0)
}

fn fmt_pnl(pnl: f64) -> String {
    if pnl >= 0.0 {
        format!("▲ +${:.2}", creds_to_price(pnl))
    } else {
        format!("▼ -${:.2}", creds_to_price(pnl.abs()))
    }
}

const PROFESSOR_PORT: &str = "ProfessorPort";

fn apply_buy(
    port: &mut Portfolio,
    history: &mut VecDeque<TradeRecord>,
    ticker: &str,
    asset_name: &str,
    asset_type: AssetType,
    quantity: f64,
    price_per_unit: f64,
    portfolio_name: &str,
) {
    port.cash -= price_per_unit * quantity;

    if let Some(existing) = port.positions.iter_mut().find(|p| {
        p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_))
    }) {
        let total_qty = existing.quantity + quantity;
        existing.avg_cost =
            (existing.avg_cost * existing.quantity + price_per_unit * quantity) / total_qty;
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
        total_creds: price_per_unit * quantity,
        realized_pnl: None,
        timestamp: Utc::now(),
    };
    history.push_back(record);
    if history.len() > TRADE_HISTORY_LIMIT {
        history.pop_front();
    }
}

fn apply_sell(
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
    let pnl = proceeds - avg_cost * quantity;

    port.cash += proceeds;
    port.positions[pos_idx].quantity -= quantity;
    if port.positions[pos_idx].quantity < 1e-9 {
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

fn find_option_idx(
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

const FMP_CACHE_TTL: Duration = Duration::from_secs(300);
static FMP_CACHE: LazyLock<DashMap<String, (FmpProfile, Instant)>> =
    LazyLock::new(DashMap::new);

const FMP_RATIOS_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 15);
static FMP_RATIOS_CACHE: LazyLock<DashMap<String, (FmpRatios, Instant)>> =
    LazyLock::new(DashMap::new);

static LOGO_API_KEY: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("LOGO_API_KEY").ok());

/// Set to true when Yahoo Finance returns HTTP 429; cleared on next successful response.
static YAHOO_RATE_LIMITED: AtomicBool = AtomicBool::new(false);

fn logo_url(ticker: &str) -> Option<String> {
    let key = LOGO_API_KEY.as_deref()?;
    Some(format!("https://img.logo.dev/ticker/{}?token={}", ticker, key))
}

fn market_closed_reply(action: &str, ticker: &str) -> poise::CreateReply {
    poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title(action)
            .description(format!("Market is currently closed for **{}**.", ticker))
            .color(data::EMBED_ERROR),
    )
}

/// Returns a user-facing description when market data can't be fetched.
/// If the Yahoo Finance rate limit was recently hit, tells the user to try again tomorrow.
fn market_data_err(query: &str) -> String {
    if YAHOO_RATE_LIMITED.load(Ordering::Relaxed) {
        "Market data API is rate limited — try again tomorrow when the limit resets.".to_string()
    } else {
        format!("Could not fetch market data for **{}**.", query)
    }
}

fn with_logo(embed: serenity::CreateEmbed, ticker: &str) -> serenity::CreateEmbed {
    match logo_url(ticker) {
        Some(url) => embed.thumbnail(url),
        None => embed,
    }
}

fn looks_like_ticker(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && (s.len() <= 4 || s.contains('-') || s.contains('.'))
}

async fn resolve_ticker(query: &str) -> Option<YfQuote> {
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

async fn fetch_price(ticker: &str) -> Option<f64> {
    fetch_quote_detail(ticker)
        .await
        .and_then(|q| q.regular_market_price)
}

async fn fetch_quote_detail(ticker: &str) -> Option<YfQuote> {
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

async fn fetch_fmp_profile(ticker: &str) -> Option<FmpProfile> {
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

async fn fetch_fmp_ratios(ticker: &str) -> Option<FmpRatios> {
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
    let news = fetch_market_news().await;
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
    #[description = "Name for the new portfolio (max 32 chars)"] name: String,
) -> Result<(), Error> {
    if name.len() > 32 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Create")
                    .description("Portfolio name must be 32 characters or fewer.")
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

    // Collect portfolios and rate label under the read lock, then drop it.
    let (portfolios, rate_label) = {
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

        let rate_label = if is_gold(&user_data) {
            format!("Gold HYSA: {:.2}% APY", gold_hysa_rate(fed_rate_val))
        } else {
            format!("Base HYSA: {:.2}% APY", data::BASE_HYSA_RATE)
        };

        (user_data.stock.portfolios.clone(), rate_label)
        // read lock released here
    };

    // Fetch live prices and build description outside the lock.
    let mut desc = String::new();
    for p in &portfolios {
        let mut positions_value: f64 = 0.0;
        let mut cost_basis: f64 = 0.0;
        for pos in &p.positions {
            if let Some(price) = fetch_price(&pos.ticker).await {
                positions_value += price_to_creds(price) * pos.quantity;
            }
            cost_basis += pos.avg_cost * pos.quantity;
        }
        let total_creds = p.cash + positions_value;
        let pnl_str = if cost_basis > 0.0 {
            let pct = (positions_value - cost_basis) / cost_basis * 100.0;
            format!("{:+.2}%", pct)
        } else {
            "—".to_string()
        };
        desc += &format!(
            "**{}** — **${:.2}** (total value) | {} | {} positions\n",
            p.name,
            creds_to_price(total_creds),
            pnl_str,
            p.positions.len(),
        );
    }

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

    let mut price_cache: HashMap<String, f64> = HashMap::new();
    let mut positions_value: f64 = 0.0;
    for pos in &portfolio.positions {
        let price = if let Some(&p) = price_cache.get(&pos.ticker) {
            p
        } else {
            let p = fetch_price(&pos.ticker).await.unwrap_or(0.0);
            price_cache.insert(pos.ticker.clone(), p);
            p
        };
        positions_value += price_to_creds(price) * pos.quantity;
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

            match &pos.asset_type {
                AssetType::Option(contract) => {
                    let intrinsic = match contract.option_type {
                        OptionType::Call => (current_price_usd - contract.strike).max(0.0),
                        OptionType::Put => (contract.strike - current_price_usd).max(0.0),
                    };
                    let current_premium =
                        option_premium_creds(intrinsic, &contract.expiry, contract.contracts);
                    let type_str = option_type_str(&contract.option_type);
                    if contract.side == OptionSide::Short {
                        // Short position: cost_basis = premium received; current_premium = cost to close
                        let pnl = cost_basis - current_premium;
                        desc += &format!(
                            "SHORT **{} {} ${:.2}** exp {} — {} contracts\nPremium rcvd: **${:.2}** | Obligation: **${:.2}** | P&L: **${:+.2}**{}\n\n",
                            pos.ticker, type_str, contract.strike,
                            contract.expiry.format("%Y-%m-%d"), contract.contracts,
                            creds_to_price(cost_basis),
                            creds_to_price(current_premium),
                            creds_to_price(pnl),
                            fmt_pct_change(pnl, cost_basis)
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
                            fmt_pct_change(pnl, cost_basis)
                        );
                    }
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
    #[description = "Dollar amount to deposit (e.g. $1.00 = 100 creds)"] dollars: f64,
) -> Result<(), Error> {
    if dollars <= 0.0 || dollars > 100_000.0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Fund")
                    .description("Amount must be between $0.01 and $100,000.00.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let amount = price_to_creds(dollars) as i32;

    let data = &ctx.data().users;
    let u = data.get(&ctx.author().id).unwrap();

    let fund_result: Result<f64, String> = {
        let mut user_data = u.write().await;
        if user_data.get_creds() < amount {
            Err(format!(
                "Insufficient creds. You have **${:.2}** ({} creds) but tried to deposit **${:.2}** ({} creds).",
                creds_to_price(user_data.get_creds() as f64), user_data.get_creds(),
                dollars, amount
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
                    user_data.sub_creds(amount);
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
                            "Deposited **${:.2}** into **{}**.\nNew cash balance: **${:.2}**.",
                            creds_to_price(price_to_creds(dollars).floor()), name, creds_to_price(new_cash)
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
    #[description = "Dollar amount to withdraw (e.g. $1.00 = 100 creds)"] dollars: f64,
) -> Result<(), Error> {
    if dollars <= 0.0 || dollars > 100_000.0 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Portfolio — Withdraw")
                    .description("Amount must be between $0.01 and $100,000.00.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let amount = price_to_creds(dollars) as i32;

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
                "Insufficient cash. **{}** has **${:.2}** but tried to withdraw **${:.2}**.",
                name, creds_to_price(p.cash), dollars
            )),
            Some(p) => {
                p.cash -= amount as f64;
                let remaining = p.cash;
                user_data.add_creds(amount);
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
                            "Withdrew **${:.2}** from **{}** to your wallet.\nRemaining cash: **${:.2}**.",
                            dollars, name, creds_to_price(remaining_cash)
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

    let mut msg = reply.into_message().await?;
    let interaction = msg
        .await_component_interactions(ctx)
        .author_id(ctx.author().id)
        .timeout(Duration::from_secs(30))
        .await;

    match interaction {
        None => {
            msg.edit(
                ctx.serenity_context(),
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::new()
                            .title("Portfolio — Delete")
                            .description(format!("{}\n\nTimed out. Portfolio not deleted.", detail))
                            .color(data::EMBED_ERROR)
                            .footer(serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo")),
                    )
                    .components(vec![]),
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
                            .description(market_data_err(&items[0]))
                            .color(data::EMBED_ERROR),
                    ),
                )
                .await?;
                return Ok(());
            }
        };
        let ticker = quote.symbol.clone();
        let (profile, ratios) = tokio::join!(
            fetch_fmp_profile(&ticker),
            fetch_fmp_ratios(&ticker)
        );

        let price_usd = profile
            .as_ref()
            .and_then(|p| p.price)
            .or(quote.regular_market_price)
            .unwrap_or(0.0);
        let change = profile.as_ref().and_then(|p| p.change).unwrap_or(0.0);
        let change_pct = profile
            .as_ref()
            .and_then(|p| p.change_percentage)
            .unwrap_or(0.0);
        let color = if change >= 0.0 {
            data::EMBED_SUCCESS
        } else {
            data::EMBED_FAIL
        };
        let display_name = profile
            .as_ref()
            .and_then(|p| p.company_name.clone())
            .unwrap_or_else(|| quote.display_name());

        let arrow = if change_pct >= 0.0 { "+" } else { "" };
        let mut desc = format!("**${:.2}** ({}{:.2}%)\n", price_usd, arrow, change_pct);

        if let Some(p) = &profile {
            desc += "─────────────────────\n";
            let mut meta = Vec::new();
            if let Some(sector) = &p.sector { meta.push(sector.clone()); }
            if let Some(industry) = &p.industry { meta.push(industry.clone()); }
            if !meta.is_empty() {
                desc += &format!("{}\n", meta.join(" · "));
            }
            if let Some(country) = &p.country {
                let exchange = p.exchange.as_deref().unwrap_or("");
                desc += &format!("Country: **{}**  Exchange: **{}**\n", country.to_uppercase(), exchange.to_uppercase());
            }
            desc += "─────────────────────\n";
            if let Some(vol) = p.volume {
                desc += &format!("Volume: **{}**\n", format_large_num(vol as f64));
            }
            if let Some(mc) = p.market_cap {
                desc += &format!("Market Cap: **{}**\n", format_large_num(mc));
            }
            if let Some(pe) = ratios.as_ref().and_then(|r| r.pe_ratio).filter(|&v| v > 0.0) {
                desc += &format!("P/E (TTM): **{:.2}**\n", pe);
            }
            if let Some(r) = &p.range {
                desc += &format!("52-Week: **{}**\n", r);
            }
        }
        desc += &format!("\n{}", quote.market_status());

        let embed = with_logo(
            serenity::CreateEmbed::new()
                .title(format!("{} — {}", ticker, display_name))
                .description(desc)
                .color(color)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
            &ticker,
        );
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
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
    #[description = "Number of shares to buy (fractional ok)"] quantity: Option<f64>,
    #[description = "Dollar amount to spend (e.g. 200 to buy $200 worth)"] amount: Option<f64>,
    #[description = "Portfolio to buy into"] portfolio: String,
) -> Result<(), Error> {
    // Validate: exactly one of quantity or amount must be provided
    if quantity.is_none() && amount.is_none() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description("Provide either **quantity** (shares) or **amount** (dollars), not neither.")
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }
    if quantity.is_some() && amount.is_some() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description("Provide either **quantity** (shares) or **amount** (dollars), not both.")
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
                        .description(market_data_err(&ticker_query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();

    // [TEST] market open guard disabled
    // if !quote.is_market_open() { ctx.send(market_closed_reply("Buy", &ticker)).await?; return Ok(()); }

    let price_usd = match quote.regular_market_price {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Buy")
                        .description(market_data_err(&ticker))
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
    let quantity = quantity.unwrap_or_else(|| amount.unwrap() / price_usd);

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

    let total_cost = price_per_unit * quantity;

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = match user_data.stock.portfolios.iter().position(|p| p.name.eq_ignore_ascii_case(&portfolio)) {
        Some(i) => i,
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

    if user_data.stock.portfolios[port_idx].cash < total_cost {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buy")
                    .description(format!(
                        "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                        creds_to_price(total_cost), total_cost, portfolio,
                        creds_to_price(user_data.stock.portfolios[port_idx].cash),
                        user_data.stock.portfolios[port_idx].cash
                    ))
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    {
        let stock = &mut user_data.stock;
        apply_buy(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &asset_name, asset_type, quantity, price_per_unit, &portfolio);
    }
    drop(user_data);

    let embed = with_logo(
        serenity::CreateEmbed::new()
            .title("Buy")
            .description(format!(
                "Bought **{} {}** ({}) for **${:.2}** ({:.0} creds)\n${:.2}/unit | Portfolio: **{}**",
                fmt_qty(quantity), ticker, asset_name, creds_to_price(total_cost), total_cost, price_usd, portfolio
            ))
            .color(data::EMBED_SUCCESS)
            .footer(serenity::CreateEmbedFooter::new(
                "@~ powered by UwUntu & RustyBamboo",
            )),
        &ticker,
    );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
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
) -> Result<(), Error> {
    if quantity.is_some() && amount.is_some() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Sell")
                    .description("Provide either a **quantity** or a **dollar amount**, not both.")
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
                        .description(market_data_err(&ticker_query))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let ticker = quote.symbol.clone();
    let asset_name = quote.display_name();

    // [TEST] market open guard disabled
    // if !quote.is_market_open() { ctx.send(market_closed_reply("Sell", &ticker)).await?; return Ok(()); }

    let price_usd = match quote.regular_market_price {
        Some(p) => p,
        None => {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Sell")
                        .description(market_data_err(&ticker))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let price_per_unit = price_to_creds(price_usd);
    // quantity resolved below after position lookup for "sell all" case

    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;

    let port_idx = match user_data.stock.portfolios.iter().position(|p| p.name.eq_ignore_ascii_case(&portfolio)) {
        Some(i) => i,
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

    let pos_idx = match user_data.stock.portfolios[port_idx]
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
                        .description(format!("No **{}** position in portfolio **{}**.", ticker, portfolio))
                        .color(data::EMBED_ERROR),
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let quantity = if let Some(q) = quantity {
        q
    } else if let Some(a) = amount {
        a / price_usd
    } else {
        user_data.stock.portfolios[port_idx].positions[pos_idx].quantity
    };

    if user_data.stock.portfolios[port_idx].positions[pos_idx].quantity < quantity - 1e-9 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Sell")
                    .description(format!(
                        "You only hold **{}** of **{}** but tried to sell **{}**.",
                        fmt_qty(user_data.stock.portfolios[port_idx].positions[pos_idx].quantity), ticker, fmt_qty(quantity)
                    ))
                    .color(data::EMBED_ERROR),
            ),
        )
        .await?;
        return Ok(());
    }

    let (proceeds, pnl) = {
        let stock = &mut user_data.stock;
        let pnl = apply_sell(&mut stock.portfolios[port_idx], &mut stock.trade_history, &ticker, &asset_name, quantity, price_per_unit, &portfolio).unwrap_or(0.0);
        (price_per_unit * quantity, pnl)
    };

    let pnl_str = if pnl >= 0.0 {
        format!("▲ +${:.2} ({:.0} creds)", creds_to_price(pnl), pnl)
    } else {
        format!("▼ -${:.2} ({:.0} creds)", creds_to_price(pnl.abs()), pnl)
    };
    let color = if pnl >= 0.0 {
        data::EMBED_SUCCESS
    } else {
        data::EMBED_FAIL
    };
    drop(user_data);

    let embed = with_logo(
        serenity::CreateEmbed::new()
            .title("Sell")
            .description(format!(
                "Sold **{} {}** for **${:.2}** ({:.0} creds)\n${:.2}/unit | Realized P&L: **{}**",
                fmt_qty(quantity), ticker, creds_to_price(proceeds), proceeds, price_usd, pnl_str
            ))
            .color(color)
            .footer(serenity::CreateEmbedFooter::new(
                "@~ powered by UwUntu & RustyBamboo",
            )),
        &ticker,
    );
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

// ── Watchlist ─────────────────────────────────────────────────────────────────

/// Manage your watchlist
#[poise::command(
    slash_command,
    subcommands("watchlist_add", "watchlist_remove", "watchlist_list", "watchlist_clear")
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
                        .description(market_data_err(&query))
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
            "**{}** — {} | ${:.2} | {} **{:.2}%**",
            ticker,
            quote.display_name(),
            price_usd,
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

/// Clear all tickers from your watchlist
#[poise::command(slash_command, rename = "clear")]
async fn watchlist_clear(ctx: Context<'_>) -> Result<(), Error> {
    let data_ref = &ctx.data().users;
    let u = data_ref.get(&ctx.author().id).unwrap();
    let mut user_data = u.write().await;
    let count = user_data.stock.watchlist.len();
    user_data.stock.watchlist.clear();
    drop(user_data);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Watchlist")
                .description(format!("Cleared **{}** ticker(s) from your watchlist.", count))
                .color(data::EMBED_SUCCESS),
        ),
    )
    .await?;
    Ok(())
}

// ── Trade History ─────────────────────────────────────────────────────────────

fn build_summary_embed(trades: &std::collections::VecDeque<TradeRecord>) -> serenity::CreateEmbed {
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
            name, count, creds_to_price(*gains), creds_to_price(losses.abs()), creds_to_price(net), fmt_pct_change(net, *basis)
        );
    }
    desc += &format!("\n**Total Net P&L: ${:+.2}{}**", creds_to_price(total_net), fmt_pct_change(total_net, total_basis));

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
        let cost = t.total_creds - pnl;
        desc += &format!(
            "{} **{}** [{}] × {} | P&L: **${:+.2}{}**\n",
            t.timestamp.format("%m/%d"),
            t.ticker,
            t.portfolio,
            fmt_qty(t.quantity),
            creds_to_price(pnl),
            fmt_pct_change(pnl, cost)
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
            .map(|p| {
                let cost = t.total_creds - p;
                format!(" | P&L: **${:+.2}{}**", creds_to_price(p), fmt_pct_change(p, cost))
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
                    intrinsic * 100.0, time_value_usd * 100.0,
                    premium_per_contract_usd, premium_creds,
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

    let pnl_str = fmt_pnl(pnl);
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
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
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
                        .title("Options Write")
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
                        .title("Options Write")
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
                    .title("Options Write")
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
                    ctx.send(
                        poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Options Write")
                                .description(format!(
                                    "Covered call requires **{:.0} shares** of **{}** but **{}** only holds **{:.0} shares**.",
                                    required, ticker, portfolio, shares_held
                                ))
                                .color(data::EMBED_ERROR),
                        ),
                    )
                    .await?;
                    return Ok(());
                }
            }
            OptionType::Put => {
                let required_cash = price_to_creds(strike * contracts as f64 * 100.0);
                if port.cash < required_cash {
                    ctx.send(
                        poise::CreateReply::default().embed(
                            serenity::CreateEmbed::new()
                                .title("Options Write")
                                .description(format!(
                                    "Cash-secured put requires **${:.2}** ({:.0} creds) in **{}** but only **${:.2}** ({:.0} creds) available.",
                                    creds_to_price(required_cash), required_cash, portfolio,
                                    creds_to_price(port.cash), port.cash
                                ))
                                .color(data::EMBED_ERROR),
                        ),
                    )
                    .await?;
                    return Ok(());
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
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
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
                        .title("Options Cover")
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
                        .title("Options Cover")
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

        let held = if let AssetType::Option(c) = &port.positions[pos_idx].asset_type {
            c.contracts
        } else {
            0
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

        if port.cash < cost_to_close {
            ctx.send(
                poise::CreateReply::default().embed(
                    serenity::CreateEmbed::new()
                        .title("Options Cover")
                        .description(format!(
                            "Insufficient cash. Need **${:.2}** ({:.0} creds) but **{}** has **${:.2}** ({:.0} creds).",
                            creds_to_price(cost_to_close), cost_to_close,
                            portfolio, creds_to_price(port.cash), port.cash
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

    let pnl_str = fmt_pnl(pnl);
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
                    fmt_pct_change(pnl, premium_received)
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

// ── Professor AI Portfolio ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FinnhubNewsItem {
    headline: String,
    source: String,
    datetime: i64,
}

#[derive(Deserialize)]
struct TradeCall {
    #[serde(rename = "fn")]
    func: String,
    ticker: String,
    #[allow(dead_code)]
    asset_type: Option<String>,
    amount_usd: Option<f64>,
    sell_pct: Option<f64>,
}

#[derive(Deserialize)]
struct ProfessorResponse {
    reason: String,
    trades: Vec<TradeCall>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ClaudeMessage>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

pub async fn is_market_open() -> bool {
    fetch_quote_detail("SPY").await
        .and_then(|q| Some(q.is_market_open()))
        .unwrap_or(false)
}

async fn fetch_market_news() -> Vec<String> {
    let api_key = match std::env::var("FINNHUB_API_KEY") {
        Ok(k) => k,
        Err(_) => { tracing::warn!("FINNHUB_API_KEY not set"); return vec![]; }
    };
    let resp = HTTP_CLIENT
        .get("https://finnhub.io/api/v1/news")
        .query(&[("category", "general"), ("token", api_key.as_str())])
        .send().await;
    let resp = match resp { Ok(r) => r, Err(e) => { tracing::warn!("Finnhub fetch failed: {e}"); return vec![]; } };
    let bytes = match resp.bytes().await {
        Ok(b) => b, Err(e) => { tracing::warn!("Finnhub body read failed: {e}"); return vec![]; }
    };
    let mut items: Vec<FinnhubNewsItem> = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Finnhub parse failed: {e} — body: {}", String::from_utf8_lossy(&bytes).chars().take(200).collect::<String>());
            return vec![];
        }
    };
    items.sort_by(|a, b| b.datetime.cmp(&a.datetime));
    items.into_iter().take(15).map(|n| format!("{} — {}", n.headline, n.source)).collect()
}

async fn call_claude(system: &str, user: &str) -> String {
    let api_key = match std::env::var("CLAUDE_API_KEY") {
        Ok(k) => k,
        Err(_) => { tracing::warn!("CLAUDE_API_KEY not set"); return String::new(); }
    };
    let body = ClaudeRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 1024,
        system: system.to_string(),
        messages: vec![ClaudeMessage { role: "user".to_string(), content: user.to_string() }],
    };
    let resp = HTTP_CLIENT
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body).send().await;
    let resp = match resp { Ok(r) => r, Err(e) => { tracing::warn!("Claude request failed: {e}"); return String::new(); } };
    let parsed: ClaudeResponse = match resp.json().await {
        Ok(v) => v, Err(e) => { tracing::warn!("Claude parse failed: {e}"); return String::new(); }
    };
    parsed.content.into_iter().next().map(|c| c.text).unwrap_or_default()
}

async fn morning_pulse(headlines: &[String], memory: &ProfessorMemory) -> String {
    if headlines.is_empty() { return String::new(); }
    let headlines_str = headlines.join("\n");
    let user_prompt = format!(
        "Split these headlines into macro events and individual stock news.\n\
         For each, write one bullet. Then list tickers to watch.\n\n\
         MACRO (rates, geopolitics, commodities, indices):\n- ...\n\n\
         STOCKS (individual company news):\n- ...\n\n\
         WATCH: [comma-separated tickers relevant to today's news]\n\
         SENTIMENT: risk-on | risk-off | neutral\n\n\
         Headlines:\n{headlines_str}"
    );
    call_claude(&memory.core_behavior, &user_prompt).await
}

fn parse_watch_tickers(pulse: &str) -> Vec<String> {
    for line in pulse.lines() {
        if line.trim_start().starts_with("WATCH:") {
            let s = line.trim_start_matches("WATCH:").trim();
            return s.split(',').map(|t| t.trim().to_uppercase()).filter(|t| !t.is_empty()).collect();
        }
    }
    vec![]
}

fn parse_sentiment(pulse: &str) -> String {
    for line in pulse.lines() {
        if line.trim_start().starts_with("SENTIMENT:") {
            return line.trim_start_matches("SENTIMENT:").trim().to_string();
        }
    }
    "neutral".to_string()
}

async fn midday_check(pulse: &str, positions_str: &str, memory: &ProfessorMemory) -> String {
    if pulse.is_empty() && positions_str.is_empty() { return String::new(); }
    let user_prompt = format!(
        "Today's market:\n{pulse}\n\n\
         Your current positions (live prices):\n{positions_str}\n\n\
         Score each position: HOLD | ADD | REDUCE — one line each with a brief reason."
    );
    call_claude(&memory.core_behavior, &user_prompt).await
}

async fn trading_session(
    entries_str: &str,
    pulse: &str,
    sentiment: &str,
    midday_scores: &str,
    portfolio_str: &str,
    cash_usd: f64,
    memory: &ProfessorMemory,
) -> Option<ProfessorResponse> {
    let max_per_trade = cash_usd * 0.30;
    let user_prompt = format!(
        "Your recent trade log (last 7 days):\n{entries_str}\n\n\
         Today's market:\n{pulse}\nSentiment: {sentiment}\n\n\
         Positions and midday scores:\n{midday_scores}\n\n\
         Portfolio:\n{portfolio_str}\n\n\
         AVAILABLE FUNCTIONS:\n\
         apply_buy(ticker, asset_type: \"Stock\"|\"ETF\"|\"Crypto\", amount_usd)\n\
         apply_sell(ticker, sell_pct: 0.0-1.0)\n\n\
         CONSTRAINTS: cash={cash_usd:.2}, max_per_trade={max_per_trade:.2}, max_trades=3, min_position=50\n\n\
         Only HIGH conviction trades — default to hold.\n\
         Return ONLY JSON: {{\"reason\":\"...\",\"trades\":[{{\"fn\":\"apply_buy\",\"ticker\":\"XOM\",\"asset_type\":\"Stock\",\"amount_usd\":120.0}}]}}\n\
         No trades: {{\"reason\":\"...\",\"trades\":[]}}"
    );
    let raw = call_claude(&memory.core_behavior, &user_prompt).await;
    if raw.is_empty() { return None; }
    let json_str = raw.trim()
        .trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    match serde_json::from_str::<ProfessorResponse>(json_str) {
        Ok(r) => Some(r),
        Err(e) => { tracing::warn!("Professor parse failed: {e} (response length: {} chars)", raw.len()); None }
    }
}

pub async fn professor_daily_session(
    users: &UsersMap,
    http: &Arc<serenity::Http>,
    bot_chat: &str,
    bot_user_id: serenity::UserId,
) {
    // [TEST] market open guard disabled
    // let market_open = is_market_open().await;

    let u = match users.get(&bot_user_id) {
        Some(u) => u,
        None => { tracing::warn!("Professor UserData not found"); return; }
    };

    // Write lock acquired here, snapshot taken, lock released before any async calls
    let (uwu_creds, claim_creds, memory, portfolio_snapshot, held_tickers) = {
        let mut ud = u.write().await;

        // Ensure professor_memory is initialized (handles data loaded before this field existed)
        if ud.professor_memory.is_none() {
            let core = std::fs::read_to_string("MEMORY.txt").unwrap_or_else(|_| {
                "You are Professor, a Discord bot managing your own investment portfolio. \
                 Prefer diversified long-term holds. Only make HIGH conviction trades. \
                 Never exceed 30% of cash per trade. Maximum 3 trades per session.".to_string()
            });
            ud.professor_memory = Some(data::ProfessorMemory {
                core_behavior: core,
                entries: std::collections::VecDeque::new(),
            });
        }

        // Ensure ProfessorPort exists; if missing, create it funded from wallet balance
        if !ud.stock.portfolios.iter().any(|p| p.name == PROFESSOR_PORT) {
            let wallet = ud.get_creds().max(0);
            ud.sub_creds(wallet);
            let mut port = data::Portfolio::new(PROFESSOR_PORT.to_string());
            port.cash = wallet as f64;
            ud.stock.portfolios.push(port);
            tracing::info!("Professor: created missing ProfessorPort with {wallet} creds cash");
        }

        let uwu_creds = crate::basic::simulate_uwu(&mut ud);
        let claim_creds = crate::basic::simulate_claim(&mut ud);

        // Sweep full wallet into portfolio cash (covers initial 100k + any daily earnings)
        let wallet = ud.get_creds().max(0);
        if wallet > 0 {
            ud.sub_creds(wallet);
            if let Some(port) = ud.stock.portfolios.iter_mut().find(|p| p.name == PROFESSOR_PORT) {
                port.cash += wallet as f64;
            }
        }

        let memory = ud.professor_memory.clone().unwrap_or_default();
        let (snap, tickers) = ud.stock.portfolios.iter()
            .find(|p| p.name == PROFESSOR_PORT)
            .map(|port| {
                let snap = format!("Cash: ${:.2}\nPositions:\n{}", creds_to_price(port.cash),
                    port.positions.iter().filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                    .map(|p| format!("  {}: {:.4}sh @ ${:.2}", p.ticker, p.quantity, creds_to_price(p.avg_cost)))
                    .collect::<Vec<_>>().join("\n"));
                let tickers = port.positions.iter()
                    .filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                    .map(|p| p.ticker.clone()).collect::<Vec<_>>();
                (snap, tickers)
            })
            .unwrap_or_default();
        (uwu_creds, claim_creds, memory, snap, tickers)
    };


    // [TEST] market closed early return disabled
    // if !market_open { ... }

    let headlines = fetch_market_news().await;
    let pulse = morning_pulse(&headlines, &memory).await;
    let watch_tickers = parse_watch_tickers(&pulse);
    let sentiment = parse_sentiment(&pulse);

    let mut all_tickers = held_tickers.clone();
    for t in &watch_tickers { if !all_tickers.contains(t) { all_tickers.push(t.clone()); } }
    let price_results = futures::future::join_all(
        all_tickers.iter().map(|t| { let t = t.clone(); async move { (t.clone(), fetch_quote_detail(&t).await) } })
    ).await;
    let mut prices: HashMap<String, (f64, String, AssetType)> = HashMap::new();
    for (ticker, q) in price_results {
        if let Some(q) = q {
            if let Some(price) = q.regular_market_price {
                prices.insert(ticker, (price, q.display_name(), q.asset_type()));
            }
        }
    }

    let positions_with_prices = {
        let ur = u.read().await;
        ur.stock.portfolios.iter().find(|p| p.name == PROFESSOR_PORT).map(|port| {
            port.positions.iter().filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
            .map(|p| {
                let cur = prices.get(&p.ticker).map(|(pr,_,_)| *pr).unwrap_or(0.0);
                let avg = creds_to_price(p.avg_cost);
                let pct = if avg > 0.0 { (cur - avg) / avg * 100.0 } else { 0.0 };
                format!("  {}: {:.4}sh @ ${:.2}, now ${:.2} ({:+.1}%)", p.ticker, p.quantity, avg, cur, pct)
            }).collect::<Vec<_>>().join("\n")
        }).unwrap_or_default()
    };

    let midday_scores = midday_check(&pulse, &positions_with_prices, &memory).await;

    let entries_str = if memory.entries.is_empty() {
        "No previous entries yet.".to_string()
    } else {
        memory.entries.iter().map(|e| format!("[{}] {}", e.date.format("%a %b %d"), e.content))
        .collect::<Vec<_>>().join("\n")
    };

    let cash_usd = {
        let ur = u.read().await;
        ur.stock.portfolios.iter().find(|p| p.name == PROFESSOR_PORT)
        .map(|p| creds_to_price(p.cash)).unwrap_or(0.0)
    };

    let response = trading_session(&entries_str, &pulse, &sentiment, &midday_scores, &portfolio_snapshot, cash_usd, &memory).await;

    // Re-acquire write lock to execute trades and persist memory entry
    struct ExecutedTrade { action: String, ticker: String, amount_usd: f64, price_usd: f64, pnl: Option<f64> }
    let mut executed: Vec<ExecutedTrade> = vec![];
    let reason = response.as_ref().map(|r| r.reason.clone()).unwrap_or_else(|| "No data from Claude today.".to_string());

    if let Some(ref resp) = response {
        let cash_limit_creds = price_to_creds(cash_usd * 0.30);
        let mut ud = u.write().await;
        let port_idx = match ud.stock.portfolios.iter().position(|p| p.name == PROFESSOR_PORT) {
            Some(i) => i,
            None => { tracing::warn!("Professor: ProfessorPort missing at trade execution — skipping trades"); return; }
        };
        for trade in resp.trades.iter().take(3) {
            let Some((price_usd, asset_name, asset_type)) = prices.get(&trade.ticker).map(|(p,n,at)| (*p, n.clone(), at.clone())) else { continue; };
            let price_creds = price_to_creds(price_usd);
            // Split disjoint borrows each iteration — portfolios and trade_history are separate fields
            let stock = &mut ud.stock;
            let (portfolios, history) = (&mut stock.portfolios, &mut stock.trade_history);
            let port = &mut portfolios[port_idx];
            match trade.func.as_str() {
                "apply_buy" => {
                    let amount_usd = trade.amount_usd.unwrap_or(0.0);
                    if amount_usd < 50.0 { continue; }
                    let cost_creds = price_to_creds(amount_usd);
                    if port.cash < cost_creds || cost_creds > cash_limit_creds { continue; }
                    let quantity = amount_usd / price_usd;
                    apply_buy(port, history, &trade.ticker, &asset_name, asset_type, quantity, price_creds, PROFESSOR_PORT);
                    executed.push(ExecutedTrade { action: "BUY".to_string(), ticker: trade.ticker.clone(), amount_usd, price_usd, pnl: None });
                }
                "apply_sell" => {
                    let sell_pct = trade.sell_pct.unwrap_or(0.0).clamp(0.0, 1.0);
                    if sell_pct == 0.0 { continue; }
                    let qty = match port.positions.iter().find(|p| p.ticker == trade.ticker && !matches!(&p.asset_type, AssetType::Option(_))) {
                        Some(p) => p.quantity * sell_pct, None => continue,
                    };
                    let Some(pnl) = apply_sell(port, history, &trade.ticker, &asset_name, qty, price_creds, PROFESSOR_PORT) else { continue; };
                    executed.push(ExecutedTrade { action: "SELL".to_string(), ticker: trade.ticker.clone(), amount_usd: price_usd * qty, price_usd, pnl: Some(pnl) });
                }
                _ => {}
            }
        }

        if let Some(mem) = ud.professor_memory.as_mut() {
            mem.entries.push_back(MemoryEntry { date: Utc::now(), content: reason.clone() });
            let cutoff = Utc::now() - chrono::Duration::days(7);
            mem.entries.retain(|e| e.date > cutoff);
        }
    }


    let trade_lines = if executed.is_empty() {
        "No trades today — holding conviction.".to_string()
    } else {
        executed.iter().map(|t| {
            if t.action == "BUY" {
                format!("✦ BOUGHT **{}** @ ${:.2} — ${:.2}", t.ticker, t.price_usd, t.amount_usd)
            } else {
                let pnl_str = t.pnl.map(fmt_pnl).unwrap_or_default();
                format!("✦ SOLD **{}** @ ${:.2} — ${:.2}  {}", t.ticker, t.price_usd, t.amount_usd, pnl_str)
            }
        }).collect::<Vec<_>>().join("\n")
    };

    let uwu_line = if uwu_creds > 0 { format!("+{uwu_creds} creds") } else if uwu_creds < 0 { format!("{uwu_creds} creds (crit fail)") } else { "cooldown".to_string() };
    let claim_line = if claim_creds > 0 { format!("+{claim_creds} creds") } else { "not yet (need 3 uwu rolls)".to_string() };

    let portfolio_after = {
        let ur = u.read().await;
        ur.stock.portfolios.iter().find(|p| p.name == PROFESSOR_PORT).map(|port| {
            let total_usd = port.positions.iter().filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                .map(|p| prices.get(&p.ticker).map(|(pr,_,_)| *pr).unwrap_or(0.0) * p.quantity)
                .sum::<f64>() + creds_to_price(port.cash);
            let pos_lines = port.positions.iter().filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                .map(|p| {
                    let cur = prices.get(&p.ticker).map(|(pr,_,_)| *pr).unwrap_or(0.0);
                    let avg = creds_to_price(p.avg_cost);
                    let pct = if avg > 0.0 { (cur-avg)/avg*100.0 } else { 0.0 };
                    format!("{}: {:.4}sh  {:+.1}%", p.ticker, p.quantity, pct)
                }).collect::<Vec<_>>().join("\n");
            format!("Cash: ${:.2}\n{}\nTotal Value: ${:.2}", creds_to_price(port.cash), pos_lines, total_usd)
        }).unwrap_or_default()
    };

    let desc = format!(
        "**Daily Income**\n/uwu: {uwu_line}\n/claim: {claim_line}\n\n\
         **Market Today**\n{pulse}\n\n\
         **Portfolio Changes**\n{trade_lines}\n\n\
         *{reason}*\n\n\
         **Portfolio After**\n{portfolio_after}"
    );

    let embed = serenity::CreateEmbed::new()
        .title(format!("Professor's Daily Report — {}", Utc::now().format("%b %d, %Y")))
        .description(desc)
        .color(data::EMBED_CYAN)
        .footer(serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo"));

    let channel_id: u64 = match bot_chat.parse() {
        Ok(id) => id,
        Err(_) => { tracing::warn!("Invalid bot_chat channel id: {bot_chat}"); return; }
    };
    if let Err(e) = ChannelId::new(channel_id).send_message(http, CreateMessage::new().embed(embed)).await {
        tracing::warn!("Failed to post professor summary: {e}");
    }
}

#[poise::command(slash_command, description_localized("en-US", "View Professor's AI portfolio"))]
pub async fn professor(ctx: Context<'_>) -> Result<(), Error> {
    let bot_user_id = ctx.data().bot_user_id;
    let u = match ctx.data().users.get(&bot_user_id) {
        Some(u) => u,
        None => { ctx.say("Professor's portfolio hasn't been initialized yet.").await?; return Ok(()); }
    };
    let user_data = u.read().await;
    let port = match user_data.stock.portfolios.iter().find(|p| p.name == PROFESSOR_PORT) {
        Some(p) => p,
        None => { ctx.say("Professor doesn't have a portfolio yet.").await?; return Ok(()); }
    };

    let pos_lines = {
        let stocks: Vec<String> = port.positions.iter()
            .filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
            .map(|p| format!("**{}**: {:.4}sh @ avg ${:.2}", p.ticker, p.quantity, creds_to_price(p.avg_cost)))
            .collect();
        if stocks.is_empty() { "No positions yet.".to_string() } else { stocks.join("\n") }
    };

    let memory_str = user_data.professor_memory.as_ref()
        .and_then(|m| m.entries.back())
        .map(|e| format!("_{}_\n{}", e.date.format("%b %d, %Y"), e.content))
        .unwrap_or_else(|| "_No entries yet._".to_string());

    let desc = format!(
        "**Cash:** ${:.2}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\
         **Positions:**\n{pos_lines}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\
         **Latest Thoughts:**\n{memory_str}",
        creds_to_price(port.cash)
    );

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Professor's Portfolio")
            .description(desc)
            .color(data::EMBED_CYAN)
            .footer(serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo")),
    )).await?;
    Ok(())
}

/// [MOD] Manually trigger Professor's daily session
#[poise::command(slash_command)]
pub async fn test_professor(ctx: Context<'_>) -> Result<(), Error> {
    if !crate::clips::check_mod(ctx).await? {
        ctx.say("You don't have permission to run this.").await?;
        return Ok(());
    }

    ctx.say("Triggering Professor's daily session...").await?;

    let users = Arc::clone(&ctx.data().users);
    let http = ctx.serenity_context().http.clone();
    let bot_chat = ctx.data().bot_chat.clone();
    let bot_user_id = ctx.data().bot_user_id;

    tokio::spawn(async move {
        professor_daily_session(&users, &http, &bot_chat, bot_user_id).await;
    });

    Ok(())
}
