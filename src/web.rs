//! Lightweight HTTP API for uwuwebu (Next.js frontend).
//! Read-only endpoints — no mutations, no persistence writes.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

use crate::data::{AssetType, Portfolio, UserData};
use crate::serenity;

/// Shared state passed to all axum handlers.
#[derive(Clone)]
pub struct WebState {
    pub users: Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
    pub http: Arc<serenity::Http>,
}

/// Default page size for paginated endpoints.
const DEFAULT_PAGE_LIMIT: usize = 20;
/// Maximum page size for paginated endpoints (leaderboard, trades).
const MAX_PAGE_LIMIT: usize = 50;
/// Maximum clips returned per request (clips are unbounded in `UserData`).
const MAX_CLIPS: usize = 100;
/// Maximum Discord user IDs per batch lookup.
const MAX_BATCH_IDS: usize = 50;

pub fn router(state: WebState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:3000".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    Router::new()
        .route("/health", get(health))
        .route("/user/{discord_id}", get(get_user))
        .route("/user/{discord_id}/portfolio", get(get_portfolio))
        .route("/user/{discord_id}/positions", get(get_positions))
        .route("/user/{discord_id}/trades", get(get_trades))
        .route("/user/{discord_id}/clips", get(get_clips))
        .route("/leaderboard", get(get_leaderboard))
        .route("/discord/users", get(get_discord_users))
        .layer(cors)
        .with_state(state)
}

async fn health() -> &'static str {
    "OK"
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ProfileDto {
    username: Option<String>,
    avatar_url: Option<String>,
    creds: i32,
    level: i32,
    xp: i32,
    xp_next: i32,
    rolls: i32,
    daily_count: i32,
    tickets: i32,
    bonus_count: i32,
    luck: String,
}

#[derive(Debug, Serialize)]
struct PortfolioSummaryDto {
    portfolios: Vec<PortfolioEntry>,
}

#[derive(Debug, Serialize)]
struct PortfolioEntry {
    name: String,
    cash: f64,
    position_count: usize,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn resolve_user(state: &WebState, discord_id: u64) -> Result<Arc<RwLock<UserData>>, StatusCode> {
    let uid = serenity::UserId::new(discord_id);
    state.users.get(&uid).map(|e| Arc::clone(e.value())).ok_or(StatusCode::NOT_FOUND)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn get_user(
    State(state): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&state, discord_id)?;
    let u = arc.read().await;

    let (username, avatar_url) = match state.http.get_user(serenity::UserId::new(discord_id)).await {
        Ok(user) => (Some(user.name.clone()), Some(user.face())),
        Err(_) => (None, None),
    };

    Ok(Json(ProfileDto {
        username,
        avatar_url,
        creds: u.get_creds(),
        level: u.get_level(),
        xp: u.get_xp(),
        xp_next: u.get_next_level(),
        rolls: u.get_rolls(),
        daily_count: u.get_daily_count(),
        tickets: u.get_tickets(),
        bonus_count: u.get_bonus(),
        luck: u.get_luck(),
    }))
}

async fn get_portfolio(
    State(state): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&state, discord_id)?;
    let u = arc.read().await;

    let portfolios = u.stock.portfolios.iter().map(portfolio_entry).collect();
    Ok(Json(PortfolioSummaryDto { portfolios }))
}

fn portfolio_entry(p: &Portfolio) -> PortfolioEntry {
    PortfolioEntry {
        name: p.name.clone(),
        cash: p.cash,
        position_count: p.positions.len(),
    }
}

// ── Leaderboard ──────────────────────────────────────────────────────────────

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LeaderboardSort {
    #[default]
    Creds,
    Level,
    Xp,
    Tickets,
    #[serde(rename = "daily_count")]
    DailyCount,
}

#[derive(Deserialize)]
struct LeaderboardQuery {
    #[serde(default)]
    sort: LeaderboardSort,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct LeaderboardDto {
    users: Vec<LeaderboardUser>,
}

#[derive(Debug, Serialize)]
struct LeaderboardUser {
    discord_id: String,
    creds: i32,
    level: i32,
    xp: i32,
    tickets: i32,
    daily_count: i32,
    luck: String,
}

async fn get_leaderboard(
    State(state): State<WebState>,
    Query(params): Query<LeaderboardQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT);

    let mut entries = Vec::new();
    for entry in state.users.iter() {
        let id = *entry.key();
        let arc = Arc::clone(entry.value());
        drop(entry);
        let u = arc.read().await;
        entries.push(LeaderboardUser {
            discord_id: id.get().to_string(),
            creds: u.get_creds(),
            level: u.get_level(),
            xp: u.get_xp(),
            tickets: u.get_tickets(),
            daily_count: u.get_daily_count(),
            luck: u.get_luck(),
        });
    }

    match params.sort {
        LeaderboardSort::Creds => entries.sort_unstable_by(|a, b| b.creds.cmp(&a.creds)),
        LeaderboardSort::Level => entries.sort_unstable_by(|a, b| b.level.cmp(&a.level)),
        LeaderboardSort::Xp => entries.sort_unstable_by(|a, b| b.xp.cmp(&a.xp)),
        LeaderboardSort::Tickets => entries.sort_unstable_by(|a, b| b.tickets.cmp(&a.tickets)),
        LeaderboardSort::DailyCount => entries.sort_unstable_by(|a, b| b.daily_count.cmp(&a.daily_count)),
    }

    entries.truncate(limit);
    Json(LeaderboardDto { users: entries })
}

// ── Positions ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct PositionsDto {
    portfolios: Vec<PositionsPortfolio>,
}

#[derive(Debug, Serialize)]
struct PositionsPortfolio {
    name: String,
    cash: f64,
    positions: Vec<PositionDto>,
}

#[derive(Debug, Serialize)]
struct PositionDto {
    ticker: String,
    asset_type: String,
    quantity: f64,
    avg_cost: f64,
}

const fn asset_type_tag(at: &AssetType) -> &'static str {
    match at {
        AssetType::Stock => "stock",
        AssetType::ETF => "etf",
        AssetType::Crypto => "crypto",
        AssetType::Option(_) => "option",
    }
}

async fn get_positions(
    State(state): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&state, discord_id)?;
    let u = arc.read().await;

    let portfolios = u.stock.portfolios.iter().map(|p| {
        PositionsPortfolio {
            name: p.name.clone(),
            cash: p.cash,
            positions: p.positions.iter().map(|pos| PositionDto {
                ticker: pos.ticker.clone(),
                asset_type: asset_type_tag(&pos.asset_type).to_string(),
                quantity: pos.quantity,
                avg_cost: pos.avg_cost,
            }).collect(),
        }
    }).collect();

    Ok(Json(PositionsDto { portfolios }))
}

// ── Trades ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TradesQuery {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct TradesDto {
    trades: Vec<TradeDto>,
}

#[derive(Debug, Serialize)]
struct TradeDto {
    portfolio: String,
    ticker: String,
    asset_name: String,
    action: String,
    quantity: f64,
    price_per_unit: f64,
    total_creds: f64,
    realized_pnl: Option<f64>,
    timestamp: String,
}

async fn get_trades(
    State(state): State<WebState>,
    Path(discord_id): Path<u64>,
    Query(params): Query<TradesQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&state, discord_id)?;
    let u = arc.read().await;
    let limit = params.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT);

    let trades: Vec<TradeDto> = u.stock.trade_history.iter().rev().take(limit).map(|t| {
        TradeDto {
            portfolio: t.portfolio.clone(),
            ticker: t.ticker.clone(),
            asset_name: t.asset_name.clone(),
            action: match t.action {
                crate::data::TradeAction::Buy => "buy",
                crate::data::TradeAction::Sell => "sell",
            }.to_string(),
            quantity: t.quantity,
            price_per_unit: t.price_per_unit,
            total_creds: t.total_creds,
            realized_pnl: t.realized_pnl,
            timestamp: t.timestamp.to_rfc3339(),
        }
    }).collect();

    Ok(Json(TradesDto { trades }))
}

// ── Clips ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ClipsDto {
    clips: Vec<ClipDto>,
}

#[derive(Debug, Serialize)]
struct ClipDto {
    title: String,
    link: String,
    date: String,
    rating: Option<f64>,
}

async fn get_clips(
    State(state): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&state, discord_id)?;
    let u = arc.read().await;

    let clips: Vec<ClipDto> = u.submits.iter().take(MAX_CLIPS).filter_map(|s| {
        s.as_ref().map(|c| ClipDto {
            title: c.title.clone(),
            link: c.link.clone(),
            date: c.date.to_rfc3339(),
            rating: c.rating,
        })
    }).collect();

    Ok(Json(ClipsDto { clips }))
}

// ── Discord User Lookup ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DiscordUsersQuery {
    ids: String,
}

#[derive(Debug, Serialize)]
struct DiscordUsersDto {
    users: Vec<DiscordUserInfo>,
}

#[derive(Debug, Serialize)]
struct DiscordUserInfo {
    discord_id: String,
    username: String,
    avatar_url: String,
}

async fn get_discord_users(
    State(state): State<WebState>,
    Query(params): Query<DiscordUsersQuery>,
) -> impl IntoResponse {
    let ids: Vec<u64> = params.ids.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .take(MAX_BATCH_IDS)
        .collect();

    let mut handles = Vec::with_capacity(ids.len());
    for id in ids {
        let http = Arc::clone(&state.http);
        handles.push(tokio::spawn(async move {
            let uid = serenity::UserId::new(id);
            http.get_user(uid).await.ok().map(|user| DiscordUserInfo {
                discord_id: id.to_string(),
                username: user.name.clone(),
                avatar_url: user.face(),
            })
        }));
    }

    let mut users = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(Some(info)) = handle.await {
            users.push(info);
        }
    }

    Json(DiscordUsersDto { users })
}
