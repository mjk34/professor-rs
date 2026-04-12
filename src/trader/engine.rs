//! Core trade execution — `apply_buy` and `apply_sell` are pure functions that
//! mutate Portfolio + `TradeRecord` state without any Discord or async concerns.
//! Keeping them isolated here makes them straightforward to unit-test.

use crate::data::{AssetType, Portfolio, Position, TradeAction, TradeRecord, TRADE_HISTORY_LIMIT};
use chrono::Utc;
use std::collections::VecDeque;

#[expect(clippy::too_many_arguments, reason = "apply_buy mirrors the full trade record — all fields are required")]
pub(crate) fn apply_buy(
    port: &mut Portfolio,
    history: &mut VecDeque<TradeRecord>,
    ticker: &str,
    asset_name: &str,
    asset_type: AssetType,
    quantity: f64,
    price_per_unit: f64,
    total_cost_creds: f64,
    portfolio_name: &str,
) {
    port.cash -= total_cost_creds;

    if let Some(existing) = port.positions.iter_mut().find(|p| {
        p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_))
    }) {
        let total_qty = existing.quantity + quantity;
        existing.avg_cost = existing.avg_cost.mul_add(existing.quantity, total_cost_creds) / total_qty;
        existing.quantity = total_qty;
    } else {
        port.positions.push(Position {
            ticker: ticker.to_string(),
            asset_type,
            quantity,
            avg_cost: price_per_unit,
        });
    }

    history.push_back(TradeRecord {
        portfolio: portfolio_name.to_string(),
        ticker: ticker.to_string(),
        asset_name: asset_name.to_string(),
        action: TradeAction::Buy,
        quantity,
        price_per_unit,
        total_creds: total_cost_creds,
        realized_pnl: None,
        timestamp: Utc::now(),
    });
    if history.len() > TRADE_HISTORY_LIMIT {
        history.pop_front();
    }
}

pub(crate) fn apply_sell(
    port: &mut Portfolio,
    history: &mut VecDeque<TradeRecord>,
    ticker: &str,
    asset_name: &str,
    quantity: f64,
    price_per_unit: f64,
    portfolio_name: &str,
) -> Option<f64> {
    let pos_idx = port.positions.iter()
        .position(|p| p.ticker == ticker && !matches!(&p.asset_type, AssetType::Option(_)))?;

    let avg_cost = port.positions[pos_idx].avg_cost;
    let proceeds = price_per_unit * quantity;
    let pnl      = avg_cost.mul_add(-quantity, proceeds);

    port.cash += proceeds;
    port.positions[pos_idx].quantity -= quantity;
    if port.positions[pos_idx].quantity < 1e-9 {
        // Sub-nanoshare residuals treated as fully closed
        port.positions.remove(pos_idx);
    }

    history.push_back(TradeRecord {
        portfolio: portfolio_name.to_string(),
        ticker: ticker.to_string(),
        asset_name: asset_name.to_string(),
        action: TradeAction::Sell,
        quantity,
        price_per_unit,
        total_creds: proceeds,
        realized_pnl: Some(pnl),
        timestamp: Utc::now(),
    });
    if history.len() > TRADE_HISTORY_LIMIT {
        history.pop_front();
    }
    Some(pnl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Portfolio;

    fn make_port() -> (Portfolio, VecDeque<TradeRecord>) {
        let mut port = Portfolio::new("TestPort".to_string());
        port.cash = 100_000.0;
        (port, VecDeque::new())
    }

    #[test]
    fn buy_creates_new_position() {
        let (mut port, mut history) = make_port();
        apply_buy(&mut port, &mut history, "AAPL", "Apple", AssetType::Stock, 10.0, 1500.0, 15_000.0, "TestPort");

        assert_eq!(port.cash, 85_000.0);
        assert_eq!(port.positions.len(), 1);
        assert_eq!(port.positions[0].quantity, 10.0);
        assert_eq!(port.positions[0].avg_cost, 1500.0);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].action, TradeAction::Buy);
    }

    #[test]
    fn buy_averages_cost_on_existing_position() {
        let (mut port, mut history) = make_port();
        // Buy 10 @ 1000 creds/unit
        apply_buy(&mut port, &mut history, "AAPL", "Apple", AssetType::Stock, 10.0, 1000.0, 10_000.0, "TestPort");
        // Buy 10 more @ 2000 creds/unit
        apply_buy(&mut port, &mut history, "AAPL", "Apple", AssetType::Stock, 10.0, 2000.0, 20_000.0, "TestPort");

        assert_eq!(port.positions.len(), 1);
        assert_eq!(port.positions[0].quantity, 20.0);
        assert_eq!(port.positions[0].avg_cost, 1500.0); // (10*1000 + 10*2000) / 20
    }

    #[test]
    fn sell_removes_position_when_fully_closed() {
        let (mut port, mut history) = make_port();
        apply_buy(&mut port, &mut history, "AAPL", "Apple", AssetType::Stock, 10.0, 1000.0, 10_000.0, "TestPort");
        let pnl = apply_sell(&mut port, &mut history, "AAPL", "Apple", 10.0, 1500.0, "TestPort");

        assert!(port.positions.is_empty());
        assert_eq!(pnl, Some(5000.0)); // (1500 - 1000) * 10
        assert_eq!(port.cash, 100_000.0 - 10_000.0 + 15_000.0);
    }

    #[test]
    fn sell_partial_reduces_quantity() {
        let (mut port, mut history) = make_port();
        apply_buy(&mut port, &mut history, "AAPL", "Apple", AssetType::Stock, 10.0, 1000.0, 10_000.0, "TestPort");
        apply_sell(&mut port, &mut history, "AAPL", "Apple", 5.0, 1000.0, "TestPort");

        assert_eq!(port.positions[0].quantity, 5.0);
    }

    #[test]
    fn sell_nonexistent_position_returns_none() {
        let (mut port, mut history) = make_port();
        let result = apply_sell(&mut port, &mut history, "NVDA", "Nvidia", 1.0, 500.0, "TestPort");
        assert!(result.is_none());
    }

    #[test]
    fn sell_pnl_negative_on_loss() {
        let (mut port, mut history) = make_port();
        apply_buy(&mut port, &mut history, "AAPL", "Apple", AssetType::Stock, 10.0, 2000.0, 20_000.0, "TestPort");
        let pnl = apply_sell(&mut port, &mut history, "AAPL", "Apple", 10.0, 1000.0, "TestPort");
        assert_eq!(pnl, Some(-10_000.0)); // sold at loss
    }
}
