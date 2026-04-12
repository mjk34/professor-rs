//! Shared formatting, financial math, and embed utilities.

use crate::data::{OptionType, UserData, GOLD_LEVEL_THRESHOLD};
use poise::serenity_prelude as serenity;

pub fn parse_user_mention(user_mention: &str) -> Option<u64> {
    user_mention
        .replace(&['<', '>', '!', '@', '&'][..], "")
        .parse::<u64>()
        .ok()
}

pub fn price_to_creds(usd: f64) -> f64 {
    usd * 100.0
}

pub fn creds_to_price(creds: f64) -> f64 {
    creds / 100.0
}

pub fn fmt_qty(q: f64) -> String {
    if q.fract() == 0.0 {
        format!("{q:.0}")
    } else {
        format!("{q:.4}")
    }
}

pub fn format_large_num(n: f64) -> String {
    if n >= 1e12 {
        format!("${:.2}T", n / 1e12)
    } else if n >= 1e9 {
        format!("${:.2}B", n / 1e9)
    } else if n >= 1e6 {
        format!("${:.2}M", n / 1e6)
    } else {
        format!("${n:.2}")
    }
}

pub fn option_intrinsic(opt_type: OptionType, price_usd: f64, strike: f64) -> f64 {
    match opt_type {
        OptionType::Call => (price_usd - strike).max(0.0),
        OptionType::Put  => (strike - price_usd).max(0.0),
    }
}

pub const fn option_type_str(ot: OptionType) -> &'static str {
    match ot {
        OptionType::Call => "CALL",
        OptionType::Put  => "PUT",
    }
}

pub fn default_footer() -> serenity::CreateEmbedFooter {
    serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo")
}

pub fn fmt_pnl(pnl: f64) -> String {
    if pnl >= 0.0 {
        format!("▲ +${:.2} ({:.0} creds)", creds_to_price(pnl), pnl)
    } else {
        format!("▼ -${:.2} ({:.0} creds)", creds_to_price(pnl.abs()), pnl.abs())
    }
}

pub fn fmt_limit_tag(lp: Option<f64>) -> String {
    lp.map_or_else(|| "@ market".to_string(), |p| format!("@ limit **${p:.2}**"))
}

pub fn fmt_pct_change(value: f64, basis: f64) -> String {
    if basis > 0.0 {
        format!(" ({:+.1}%)", value / basis * 100.0)
    } else {
        String::new()
    }
}

pub fn gold_hysa_rate(fed_rate: f64) -> f64 {
    (fed_rate * 0.92).max(0.5)
}

pub const fn is_gold(user_data: &UserData) -> bool {
    user_data.get_level() >= GOLD_LEVEL_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::OptionType;

    #[test]
    fn price_creds_roundtrip() {
        assert_eq!(price_to_creds(10.0), 1000.0);
        assert_eq!(creds_to_price(1000.0), 10.0);
        assert_eq!(creds_to_price(price_to_creds(42.50)), 42.50);
    }

    #[test]
    fn fmt_qty_integer_vs_fractional() {
        assert_eq!(fmt_qty(5.0), "5");
        assert_eq!(fmt_qty(5.5), "5.5000");
        assert_eq!(fmt_qty(0.0001), "0.0001");
    }

    #[test]
    fn format_large_num_thresholds() {
        assert_eq!(format_large_num(500.0), "$500.00");
        assert_eq!(format_large_num(1_500_000.0), "$1.50M");
        assert_eq!(format_large_num(2_300_000_000.0), "$2.30B");
        assert_eq!(format_large_num(1_100_000_000_000.0), "$1.10T");
    }

    #[test]
    fn option_intrinsic_call() {
        assert_eq!(option_intrinsic(OptionType::Call, 150.0, 100.0), 50.0);
        assert_eq!(option_intrinsic(OptionType::Call, 80.0, 100.0), 0.0); // OTM
    }

    #[test]
    fn option_intrinsic_put() {
        assert_eq!(option_intrinsic(OptionType::Put, 80.0, 100.0), 20.0);
        assert_eq!(option_intrinsic(OptionType::Put, 150.0, 100.0), 0.0); // OTM
    }

    #[test]
    fn fmt_pnl_positive_and_negative() {
        assert_eq!(fmt_pnl(500.0), "▲ +$5.00 (500 creds)");
        assert_eq!(fmt_pnl(-300.0), "▼ -$3.00 (300 creds)");
        assert_eq!(fmt_pnl(0.0), "▲ +$0.00 (0 creds)");
    }

    #[test]
    fn fmt_pct_change_normal_and_zero_basis() {
        assert_eq!(fmt_pct_change(10.0, 100.0), " (+10.0%)");
        assert_eq!(fmt_pct_change(-5.0, 100.0), " (-5.0%)");
        assert_eq!(fmt_pct_change(10.0, 0.0), ""); // zero basis returns empty
    }

    #[test]
    fn gold_hysa_rate_floor_and_normal() {
        assert_eq!(gold_hysa_rate(0.0), 0.5);   // floor
        assert!((gold_hysa_rate(5.0) - 4.6).abs() < 1e-9);
    }

    #[test]
    fn parse_user_mention_formats() {
        assert_eq!(parse_user_mention("<@123456789>"), Some(123_456_789));
        assert_eq!(parse_user_mention("<@!123456789>"), Some(123_456_789));
        assert_eq!(parse_user_mention("123456789"), Some(123_456_789));
        assert_eq!(parse_user_mention("notanumber"), None);
    }
}
