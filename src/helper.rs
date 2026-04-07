//!---------------------------------------------------------------------!
//! This file contains a collection of internal functions to help       !
//! reduce repetitive code                                              !
//!                                                                     !
//! Utilities:                                                          !
//!     [ ] - parse_user_mention                                        !
//!     [ ] - price_to_creds / creds_to_price                          !
//!     [ ] - fmt_qty / format_large_num                               !
//!     [ ] - option_intrinsic / parse_option_type / option_type_str   !
//!     [ ] - default_footer                                            !
//!---------------------------------------------------------------------!

use crate::data::OptionType;
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
        format!("{:.0}", q)
    } else {
        format!("{:.4}", q)
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
        format!("${:.2}", n)
    }
}

pub fn option_intrinsic(opt_type: &OptionType, price_usd: f64, strike: f64) -> f64 {
    match opt_type {
        OptionType::Call => (price_usd - strike).max(0.0),
        OptionType::Put  => (strike - price_usd).max(0.0),
    }
}

pub fn parse_option_type(s: &str) -> Option<OptionType> {
    match s.to_lowercase().as_str() {
        "call" | "c" => Some(OptionType::Call),
        "put"  | "p" => Some(OptionType::Put),
        _ => None,
    }
}

pub fn option_type_str(ot: &OptionType) -> &'static str {
    match ot {
        OptionType::Call => "CALL",
        OptionType::Put  => "PUT",
    }
}

pub fn default_footer() -> serenity::CreateEmbedFooter {
    serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo")
}
