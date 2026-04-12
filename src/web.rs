//! Lightweight HTTP API for uwuwebu (Next.js frontend).
//! Read-only endpoints — no mutations, no persistence writes.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::data::{AssetType, Portfolio, UserData};
use crate::serenity;

/// Shared state passed to all axum handlers.
pub type WebState = Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>;

/// Default page size for paginated endpoints.
const DEFAULT_PAGE_LIMIT: usize = 20;
/// Maximum page size for paginated endpoints (leaderboard, trades).
const MAX_PAGE_LIMIT: usize = 50;
/// Maximum clips returned per request (clips are unbounded in UserData).
const MAX_CLIPS: usize = 100;

pub fn router(users: WebState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/user/{discord_id}", get(get_user))
        .route("/user/{discord_id}/portfolio", get(get_portfolio))
        .route("/user/{discord_id}/positions", get(get_positions))
        .route("/user/{discord_id}/trades", get(get_trades))
        .route("/user/{discord_id}/clips", get(get_clips))
        .route("/leaderboard", get(get_leaderboard))
        .with_state(users)
}

async fn health() -> &'static str {
    "OK"
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ProfileDto {
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

fn resolve_user(users: &WebState, discord_id: u64) -> Result<Arc<RwLock<UserData>>, StatusCode> {
    let uid = serenity::UserId::new(discord_id);
    users.get(&uid).map(|e| Arc::clone(e.value())).ok_or(StatusCode::NOT_FOUND)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn get_user(
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&users, discord_id)?;
    let u = arc.read().await;

    Ok(Json(ProfileDto {
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
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&users, discord_id)?;
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
    luck: String,
}

async fn get_leaderboard(
    State(users): State<WebState>,
    Query(params): Query<LeaderboardQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT);

    let mut entries = Vec::new();
    for entry in users.iter() {
        let id = *entry.key();
        let arc = Arc::clone(entry.value());
        drop(entry);
        let u = arc.read().await;
        entries.push(LeaderboardUser {
            discord_id: id.get().to_string(),
            creds: u.get_creds(),
            level: u.get_level(),
            xp: u.get_xp(),
            luck: u.get_luck(),
        });
    }

    match params.sort {
        LeaderboardSort::Level => entries.sort_unstable_by(|a, b| b.level.cmp(&a.level)),
        LeaderboardSort::Xp => entries.sort_unstable_by(|a, b| b.xp.cmp(&a.xp)),
        LeaderboardSort::Creds => entries.sort_unstable_by(|a, b| b.creds.cmp(&a.creds)),
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
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&users, discord_id)?;
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
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
    Query(params): Query<TradesQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&users, discord_id)?;
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
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let arc = resolve_user(&users, discord_id)?;
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
