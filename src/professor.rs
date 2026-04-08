//!---------------------------------------------------------------------!
//! Professor AI portfolio logic and commands                           !
//!---------------------------------------------------------------------!

use crate::api::{fetch_quote_detail, UsersMap, HTTP_CLIENT};
use crate::data::{self, AssetType, MemoryEntry, ProfessorMemory};
use crate::helper::{creds_to_price, default_footer, fmt_qty, price_to_creds};
use crate::trader::{apply_buy, apply_sell};
use crate::{serenity, Context, Error};
use chrono::Utc;
use poise::serenity_prelude::futures;
use poise::serenity_prelude::ChannelId;
use poise::serenity_prelude::CreateMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub const PROFESSOR_PORT: &str = "ProfessorPort";
const MAX_TRADE_CASH_RATIO: f64 = 0.30;
const MIN_TRADE_USD: f64 = 50.0;
const MAX_MEMORY_ENTRIES: usize = 30;

/// Daily cache for morning_pulse — reused within the same UTC day to avoid redundant Claude calls.
pub static PULSE_CACHE: LazyLock<tokio::sync::RwLock<Option<(chrono::NaiveDate, String)>>> =
    LazyLock::new(|| tokio::sync::RwLock::new(None));

/// Daily cache for midday_check — reused within the same UTC day.
pub static MIDDAY_CACHE: LazyLock<tokio::sync::RwLock<Option<(chrono::NaiveDate, String)>>> =
    LazyLock::new(|| tokio::sync::RwLock::new(None));

// ── Finnhub / Claude structs ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FinnhubNewsItem {
    pub headline: String,
    pub source: String,
    pub datetime: i64,
}

#[derive(Serialize)]
pub struct ClaudeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<ClaudeMessage>,
}

#[derive(Deserialize)]
pub struct ClaudeContent {
    pub text: String,
}

#[derive(Deserialize)]
pub struct ClaudeResponse {
    pub content: Vec<ClaudeContent>,
}

#[derive(Deserialize)]
pub struct TradeCall {
    #[serde(rename = "fn")]
    pub func: String,
    pub ticker: String,
    #[allow(dead_code)]
    pub asset_type: Option<String>,
    pub amount_usd: Option<f64>,
    pub sell_pct: Option<f64>,
}

#[derive(Deserialize)]
pub struct ProfessorResponse {
    pub reason: String,
    pub trades: Vec<TradeCall>,
}

// ── Market news ───────────────────────────────────────────────────────────────

pub async fn fetch_market_news() -> Vec<String> {
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

pub async fn call_claude(system: &str, user: &str) -> String {
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
    let bytes = match resp.bytes().await {
        Ok(b) => b, Err(e) => { tracing::warn!("Claude body read failed: {e}"); return String::new(); }
    };
    let parsed: ClaudeResponse = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Claude parse failed: {e} — body: {}", String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>());
            return String::new();
        }
    };
    parsed.content.into_iter().next().map(|c| c.text).unwrap_or_default()
}

pub async fn morning_pulse(headlines: &[String], memory: &ProfessorMemory) -> String {
    let today = Utc::now().date_naive();
    {
        let cache = PULSE_CACHE.read().await;
        if let Some((date, ref result)) = *cache {
            if date == today {
                tracing::info!("[Professor] morning_pulse: using cached result from today");
                return result.clone();
            }
        }
    }
    if headlines.is_empty() { return String::new(); }
    let headlines_str = headlines.join("\n");
    let user_prompt = format!(
        "Summarize these headlines as macro events. Max 3 bullets. Be concise.\n\n\
         MACRO (rates, geopolitics, commodities, indices):\n- ...\n\n\
         WATCH: [up to 8 comma-separated tickers, at most 2 per sector (energy, tech, health, precious metals, etc.)]\n\
         SENTIMENT: risk-on | risk-off | neutral #emotion1 #emotion2 #emotion3\n\n\
         Headlines:\n{headlines_str}"
    );
    let result = call_claude(&memory.core_behavior, &user_prompt).await;
    if !result.is_empty() {
        *PULSE_CACHE.write().await = Some((today, result.clone()));
    }
    result
}

pub async fn midday_check(pulse: &str, positions_str: &str, memory: &ProfessorMemory) -> String {
    let today = Utc::now().date_naive();
    {
        let cache = MIDDAY_CACHE.read().await;
        if let Some((date, ref result)) = *cache {
            if date == today {
                tracing::info!("[Professor] midday_check: using cached result from today");
                return result.clone();
            }
        }
    }
    if pulse.is_empty() && positions_str.is_empty() { return String::new(); }
    let user_prompt = format!(
        "Today's market:\n{pulse}\n\n\
         Your current positions (live prices):\n{positions_str}\n\n\
         Score each position: HOLD | ADD | REDUCE — one line each with a brief reason."
    );
    let result = call_claude(&memory.core_behavior, &user_prompt).await;
    // Only cache midday if there were positions to score — an empty positions_str produces a
    // meaningless result that would incorrectly persist for subsequent runs with actual holdings.
    if !result.is_empty() && !positions_str.is_empty() {
        *MIDDAY_CACHE.write().await = Some((today, result.clone()));
    }
    result
}

pub fn parse_watch_tickers(pulse: &str) -> Vec<String> {
    for line in pulse.lines() {
        if line.trim_start().starts_with("WATCH:") {
            let s = line.trim_start_matches("WATCH:").trim();
            return s.split(',').map(|t| t.trim().to_uppercase()).filter(|t| !t.is_empty()).take(8).collect();
        }
    }
    vec![]
}

pub fn parse_sentiment(pulse: &str) -> String {
    for line in pulse.lines() {
        if line.trim_start().starts_with("SENTIMENT:") {
            return line.trim_start_matches("SENTIMENT:").trim().to_string();
        }
    }
    "neutral".to_string()
}

pub async fn trading_session(
    entries_str: &str,
    pulse: &str,
    sentiment: &str,
    midday_scores: &str,
    portfolio_str: &str,
    cash_usd: f64,
    memory: &ProfessorMemory,
) -> Option<ProfessorResponse> {
    let max_per_trade = cash_usd * MAX_TRADE_CASH_RATIO;
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
         No trades: {{\"reason\":\"...\",\"trades\":[]}}\n\
         Keep \"reason\" under 350 characters, plain text, no markdown."
    );
    let raw = call_claude(&memory.core_behavior, &user_prompt).await;
    if raw.is_empty() { return None; }
    // Extract JSON object robustly — Claude sometimes wraps response in prose or fences
    let json_str = match (raw.find('{'), raw.rfind('}')) {
        (Some(start), Some(end)) if end > start => &raw[start..=end],
        _ => {
            tracing::warn!("Professor parse failed: no JSON object found (response length: {} chars) — body: {}", raw.len(), raw.chars().take(300).collect::<String>());
            return None;
        }
    };
    match serde_json::from_str::<ProfessorResponse>(json_str) {
        Ok(r) => Some(r),
        Err(e) => { tracing::warn!("Professor parse failed: {e} — json: {}", json_str.chars().take(300).collect::<String>()); None }
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
    let (uwu_creds, claim_creds, memory, portfolio_snapshot, held_tickers, pre_trade_cash_usd, pre_trade_positions) = {
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
        let (snap, tickers, pre_trade_cash_usd, pre_trade_positions) = ud.stock.portfolios.iter()
            .find(|p| p.name == PROFESSOR_PORT)
            .map(|port| {
                let snap = format!("Cash: ${:.2}\nPositions:\n{}", creds_to_price(port.cash),
                    port.positions.iter().filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                    .map(|p| format!("  {}: {:.4}sh @ ${:.2}", p.ticker, p.quantity, creds_to_price(p.avg_cost)))
                    .collect::<Vec<_>>().join("\n"));
                let tickers = port.positions.iter()
                    .filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                    .map(|p| p.ticker.clone()).collect::<Vec<_>>();
                let cash_usd = creds_to_price(port.cash);
                let positions: Vec<(String, f64)> = port.positions.iter()
                    .filter(|p| !matches!(&p.asset_type, AssetType::Option(_)))
                    .map(|p| (p.ticker.clone(), p.quantity))
                    .collect();
                (snap, tickers, cash_usd, positions)
            })
            .unwrap_or_else(|| (String::new(), vec![], 0.0, vec![]));
        (uwu_creds, claim_creds, memory, snap, tickers, pre_trade_cash_usd, pre_trade_positions)
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
        all_tickers.iter().map(|t| { let t = t.clone(); async move { let result = fetch_quote_detail(&t).await; (t, result) } })
    ).await;
    let mut prices: HashMap<String, (f64, String, AssetType)> = HashMap::new();
    for (ticker, q) in price_results {
        if let Some(q) = q {
            if let Some(price) = q.regular_market_price {
                prices.insert(ticker, (price, q.display_name(), q.asset_type()));
            }
        }
    }

    let value_before: f64 = pre_trade_cash_usd + pre_trade_positions.iter()
        .map(|(t, q)| prices.get(t).map(|(p, _, _)| *p).unwrap_or(0.0) * q)
        .sum::<f64>();

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

    // Fetch prices for any trade tickers Claude returned that weren't already in the watch list
    if let Some(ref resp) = response {
        let missing: Vec<String> = resp.trades.iter()
            .map(|t| t.ticker.clone())
            .filter(|t| !prices.contains_key(t))
            .collect();
        if !missing.is_empty() {
            tracing::info!("[Professor] fetching prices for {} unlisted trade ticker(s): {:?}", missing.len(), missing);
            let extra = futures::future::join_all(
                missing.iter().map(|t| { let t = t.clone(); async move { let result = fetch_quote_detail(&t).await; (t, result) } })
            ).await;
            for (ticker, q) in extra {
                if let Some(q) = q {
                    if let Some(price) = q.regular_market_price {
                        prices.insert(ticker, (price, q.display_name(), q.asset_type()));
                    }
                }
            }
        }
    }

    // Re-acquire write lock to execute trades and persist memory entry
    struct ExecutedTrade { action: String, ticker: String, amount_usd: f64, price_usd: f64, pnl: Option<f64> }
    let mut executed: Vec<ExecutedTrade> = vec![];
    let reason = response.as_ref().map(|r| r.reason.clone()).unwrap_or_else(|| "No data from Claude today.".to_string());

    // Debug: log Claude's full response
    tracing::info!("[Professor] ───────────────────────────────────────");
    match &response {
        None => tracing::warn!("[Professor] Claude returned no parseable response"),
        Some(r) => {
            tracing::info!("[Professor] Claude reason: {}", r.reason);
            if r.trades.is_empty() {
                tracing::info!("[Professor] Claude trades: [] (hold)");
            } else {
                for (i, t) in r.trades.iter().enumerate() {
                    tracing::info!(
                        "[Professor] trade[{}]: fn={} ticker={} amount_usd={:?} sell_pct={:?}",
                        i, t.func, t.ticker, t.amount_usd, t.sell_pct
                    );
                }
            }
        }
    }

    if let Some(ref resp) = response {
        let cash_limit_creds = price_to_creds(cash_usd * MAX_TRADE_CASH_RATIO);
        tracing::info!("[Professor] cash_usd={:.2} cash_limit_creds={:.0}", cash_usd, cash_limit_creds);
        let mut ud = u.write().await;
        let port_idx = match ud.stock.portfolios.iter().position(|p| p.name == PROFESSOR_PORT) {
            Some(i) => i,
            None => { tracing::warn!("Professor: ProfessorPort missing at trade execution — skipping trades"); return; }
        };
        for trade in resp.trades.iter().take(3) {
            let Some((price_usd, asset_name, asset_type)) = prices.get(&trade.ticker).map(|(p,n,at)| (*p, n.clone(), at.clone())) else {
                tracing::warn!("[Professor] trade skipped — no price data for {}", trade.ticker);
                continue;
            };
            let price_creds = price_to_creds(price_usd);
            // Split disjoint borrows each iteration — portfolios and trade_history are separate fields
            let stock = &mut ud.stock;
            let (portfolios, history) = (&mut stock.portfolios, &mut stock.trade_history);
            let port = &mut portfolios[port_idx];
            match trade.func.as_str() {
                "apply_buy" => {
                    let amount_usd = trade.amount_usd.unwrap_or(0.0);
                    if amount_usd < MIN_TRADE_USD {
                        tracing::warn!("[Professor] BUY {} skipped — amount_usd {:.2} < {}", trade.ticker, amount_usd, MIN_TRADE_USD);
                        continue;
                    }
                    let cost_creds = price_to_creds(amount_usd);
                    if port.cash < cost_creds || cost_creds.round() > cash_limit_creds.round() {
                        tracing::warn!("[Professor] BUY {} skipped — cost_creds={:.0} cash={:.0} limit={:.0}", trade.ticker, cost_creds, port.cash, cash_limit_creds);
                        continue;
                    }
                    let quantity = amount_usd / price_usd;
                    apply_buy(port, history, &trade.ticker, &asset_name, asset_type, quantity, price_creds, cost_creds, PROFESSOR_PORT);
                    tracing::info!("[Professor] BUY {} executed — {:.4}sh @ ${:.2}", trade.ticker, quantity, price_usd);
                    executed.push(ExecutedTrade { action: "BUY".to_string(), ticker: trade.ticker.clone(), amount_usd, price_usd, pnl: None });
                }
                "apply_sell" => {
                    let sell_pct = trade.sell_pct.unwrap_or(0.0).clamp(0.0, 1.0);
                    if sell_pct == 0.0 {
                        tracing::warn!("[Professor] SELL {} skipped — sell_pct is 0", trade.ticker);
                        continue;
                    }
                    let qty = match port.positions.iter().find(|p| p.ticker == trade.ticker && !matches!(&p.asset_type, AssetType::Option(_))) {
                        Some(p) => p.quantity * sell_pct,
                        None => {
                            tracing::warn!("[Professor] SELL {} skipped — position not found", trade.ticker);
                            continue;
                        }
                    };
                    let Some(pnl) = apply_sell(port, history, &trade.ticker, &asset_name, qty, price_creds, PROFESSOR_PORT) else {
                        tracing::warn!("[Professor] SELL {} skipped — apply_sell returned None", trade.ticker);
                        continue;
                    };
                    tracing::info!("[Professor] SELL {} executed — {:.4}sh @ ${:.2}", trade.ticker, qty, price_usd);
                    executed.push(ExecutedTrade { action: "SELL".to_string(), ticker: trade.ticker.clone(), amount_usd: price_usd * qty, price_usd, pnl: Some(pnl) });
                }
                _ => { tracing::warn!("[Professor] unknown trade func: {}", trade.func); }
            }
        }

        if let Some(mem) = ud.professor_memory.as_mut() {
            mem.entries.push_back(MemoryEntry { date: Utc::now(), content: reason.clone() });
            let cutoff = Utc::now() - chrono::Duration::days(7);
            mem.entries.retain(|e| e.date > cutoff);
            while mem.entries.len() > MAX_MEMORY_ENTRIES {
                mem.entries.pop_front();
            }
        }
    }


    let trade_lines = if executed.is_empty() {
        "No trades today — holding conviction.".to_string()
    } else {
        executed.iter().map(|t| {
            if t.action == "BUY" {
                format!("✦ BOUGHT **{}** @ ${:.2} — ${:.2}", t.ticker, t.price_usd, t.amount_usd)
            } else {
                let pnl_str = t.pnl.map(crate::helper::fmt_pnl).unwrap_or_default();
                format!("✦ SOLD **{}** @ ${:.2} — ${:.2}  {}", t.ticker, t.price_usd, t.amount_usd, pnl_str)
            }
        }).collect::<Vec<_>>().join("\n")
    };

    let uwu_line = if uwu_creds > 0 { format!("+{uwu_creds} creds") } else if uwu_creds < 0 { format!("{uwu_creds} creds (crit fail)") } else { "cooldown".to_string() };
    let claim_line = if claim_creds > 0 { format!("+{claim_creds} creds") } else { "not yet (need 3 uwu rolls)".to_string() };

    let (portfolio_after, value_after) = {
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
            (format!("Cash: ${:.2}\n{}\nTotal Value: ${:.2}", creds_to_price(port.cash), pos_lines, total_usd), total_usd)
        }).unwrap_or_default()
    };

    let total_change_pct = if value_before > 0.0 {
        (value_after - value_before) / value_before * 100.0
    } else {
        0.0
    };
    let total_change_str = format!("{:+.2}%", total_change_pct);

    let desc = format!(
        "**Daily Income**\n/uwu: {uwu_line}\n/claim: {claim_line}\n\n\
         **Market Today**\n{pulse}\n\n\
         **Portfolio Changes**\n{trade_lines}\n\n\
         *{reason}*\n\n\
         **Portfolio After**\n{portfolio_after}\n\n\
         **Total Change: {total_change_str}**"
    );

    let embed = serenity::CreateEmbed::new()
        .title(format!("Professor's Daily Report — {}", Utc::now().format("%b %d, %Y")))
        .description(desc)
        .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1490778995980243105/Koro-sensei_goes_gangster.png?ex=69d54ba1&is=69d3fa21&hm=9a3bb34d8d2dfc5f3a478128ab59051c940f4bf68e393db7260f03682c2ed01b")
        .color(data::EMBED_CYAN)
        .footer(default_footer());

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

    let recent_trades: Vec<String> = user_data.stock.trade_history.iter().rev()
        .filter(|t| t.portfolio == PROFESSOR_PORT && !matches!(t.action, data::TradeAction::Sell if t.realized_pnl.is_none()))
        .take(5)
        .map(|t| {
            let qty = fmt_qty(t.quantity);
            let value = creds_to_price(t.total_creds);
            match t.action {
                data::TradeAction::Buy => format!("Bought **{}** shares of **{}** worth **${:.2}**", qty, t.ticker, value),
                data::TradeAction::Sell => {
                    let pnl = t.realized_pnl.unwrap_or(0.0);
                    let cost_basis = t.total_creds - pnl;
                    let pct = if cost_basis > 0.0 { pnl / cost_basis * 100.0 } else { 0.0 };
                    format!("Sold **{}** shares of **{}** worth **${:.2}** ({:+.1}%)", qty, t.ticker, value, pct)
                }
            }
        })
        .collect();

    let trades_section = if recent_trades.is_empty() {
        "_No trades yet._".to_string()
    } else {
        recent_trades.join("\n")
    };

    let desc = format!(
        "**Cash:** ${:.2}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\
         **Positions:**\n{pos_lines}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\
         **Recent Activity:**\n{trades_section}\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\
         **Latest Thoughts:**\n{memory_str}",
        creds_to_price(port.cash)
    );

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Professor's Portfolio")
            .description(desc)
            .color(data::EMBED_CYAN)
            .footer(default_footer()),
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
