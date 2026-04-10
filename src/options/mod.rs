//! Options trading module — quote, long positions, and short positions.

mod engine;
mod long;
mod quote;
mod short;

// Re-export commands for main.rs registration
#[doc(inline)] pub use long::{options_buy, options_sell};
#[doc(inline)] pub use quote::options_quote;
#[doc(inline)] pub use short::{options_cover, options_write};

// Re-export engine functions used externally (trader/portfolio.rs)
#[expect(unused_imports, reason = "option_premium_creds used by trader/portfolio.rs; others exported for completeness")]
#[doc(inline)] pub use engine::{find_option_idx, naked_margin_usd, option_premium_creds, parse_expiry};
