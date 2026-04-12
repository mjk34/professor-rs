//! Lightweight HTTP API for uwuwebu (Next.js frontend).
//! Read-only endpoints — no mutations, no persistence writes.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::data::{Portfolio, UserData};
use crate::serenity;

/// Shared state passed to all axum handlers.
pub type WebState = Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>;

pub fn router(users: WebState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/user/{discord_id}", get(get_user))
        .route("/user/{discord_id}/portfolio", get(get_portfolio))
        .with_state(users)
}

async fn health() -> &'static str {
    "OK"
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ProfileDto {
    creds: i32,
    level: i32,
    xp: i32,
    xp_next: i32,
    rolls: i32,
    daily_count: i32,
    tickets: i32,
    luck: String,
}

#[derive(Serialize)]
struct PortfolioSummaryDto {
    portfolios: Vec<PortfolioEntry>,
}

#[derive(Serialize)]
struct PortfolioEntry {
    name: String,
    cash: f64,
    position_count: usize,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn get_user(
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let uid = serenity::UserId::new(discord_id);
    let arc = users.get(&uid).map(|e| Arc::clone(e.value())).ok_or(StatusCode::NOT_FOUND)?;
    let u = arc.read().await;

    Ok(Json(ProfileDto {
        creds: u.get_creds(),
        level: u.get_level(),
        xp: u.get_xp(),
        xp_next: u.get_next_level(),
        rolls: u.get_rolls(),
        daily_count: u.get_daily_count(),
        tickets: u.get_tickets(),
        luck: u.get_luck(),
    }))
}

async fn get_portfolio(
    State(users): State<WebState>,
    Path(discord_id): Path<u64>,
) -> Result<impl IntoResponse, StatusCode> {
    let uid = serenity::UserId::new(discord_id);
    let arc = users.get(&uid).map(|e| Arc::clone(e.value())).ok_or(StatusCode::NOT_FOUND)?;
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
