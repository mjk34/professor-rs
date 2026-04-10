//! Pure option pricing functions — no Discord concerns, fully unit-testable.

use crate::data::{AssetType, OptionSide, OptionType, Position};
use crate::helper::price_to_creds;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};

/// Number of underlying shares represented by one options contract (industry standard).
pub const SHARES_PER_CONTRACT: f64 = 100.0;
/// Intrinsic time-value premium added per day-to-expiry when pricing options.
pub const TIME_VALUE_PER_DTE: f64 = 0.05;
/// Margin requirement as a fraction of notional value for a naked call position.
pub const CALL_MARGIN_RATIO: f64 = 0.20;
/// Margin requirement as a fraction of notional value for a naked put position.
pub const PUT_MARGIN_RATIO: f64 = 0.10;

pub const ERR_INVALID_OPTION_TYPE: &str = "Option type must be `call` or `put`.";
pub const ERR_INVALID_EXPIRY: &str = "Invalid expiry date. Use YYYY-MM-DD format.";
pub const ERR_EXPIRY_PAST: &str = "Expiry date is in the past.";
pub const ERR_MIN_CONTRACTS: &str = "Contracts must be at least 1.";

pub fn option_premium_creds(intrinsic_usd: f64, expiry: &DateTime<Utc>, contracts: u32) -> f64 {
    let dte = (*expiry - Utc::now()).num_days().max(0) as f64;
    let per_contract_usd = (intrinsic_usd + dte * TIME_VALUE_PER_DTE).max(0.01);
    price_to_creds(per_contract_usd * f64::from(contracts) * SHARES_PER_CONTRACT)
}

pub fn naked_margin_usd(opt_type: &OptionType, price_usd: f64, strike: f64, contracts: u32, premium_usd: f64) -> f64 {
    let notional = SHARES_PER_CONTRACT * f64::from(contracts);
    let otm_usd = match opt_type {
        OptionType::Call => (strike - price_usd).max(0.0),
        OptionType::Put  => (price_usd - strike).max(0.0),
    } * notional;
    let min_basis = match opt_type {
        OptionType::Call => price_usd,
        OptionType::Put  => strike,
    };
    f64::max(
        (CALL_MARGIN_RATIO * price_usd).mul_add(notional, premium_usd) - otm_usd,
        (PUT_MARGIN_RATIO * min_basis).mul_add(notional, premium_usd),
    )
}

pub fn find_option_idx(
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
        #[expect(clippy::float_cmp, reason = "strike prices are stored/compared as exact values we set")]
        if let AssetType::Option(c) = &p.asset_type {
            c.strike == strike && c.expiry == expiry && c.option_type == *opt_type && c.side == *side
        } else {
            false
        }
    })
}

pub fn parse_expiry(date_str: &str) -> Option<DateTime<Utc>> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59))
        .map(|dt| Utc.from_utc_datetime(&dt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn parse_expiry_valid() {
        let dt = parse_expiry("2030-12-31");
        assert!(dt.is_some());
    }

    #[test]
    fn parse_expiry_invalid_format() {
        assert!(parse_expiry("31-12-2030").is_none());
        assert!(parse_expiry("not-a-date").is_none());
    }

    #[test]
    fn option_premium_creds_minimum() {
        // 0 intrinsic, expired — should still give minimum premium
        let past = Utc::now() - Duration::days(1);
        let result = option_premium_creds(0.0, &past, 1);
        assert!(result > 0.0);
    }

    #[test]
    fn option_premium_scales_with_contracts() {
        let expiry = Utc::now() + Duration::days(30);
        let one   = option_premium_creds(5.0, &expiry, 1);
        let three = option_premium_creds(5.0, &expiry, 3);
        assert!((three - one * 3.0).abs() < 1.0);
    }
}
