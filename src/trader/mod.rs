//! Portfolio, watchlist, trades, and core trade execution engine.

mod engine;
mod portfolio;
mod trades;
mod watchlist;

// Re-export engine functions so professor.rs and stock/ can use the same path
#[doc(inline)] pub(crate) use engine::{apply_buy, apply_sell};
#[doc(inline)] pub(crate) use portfolio::portfolio;
#[doc(inline)] pub(crate) use trades::trades;
#[doc(inline)] pub(crate) use watchlist::watchlist;
