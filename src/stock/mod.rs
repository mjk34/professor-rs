//! Stock trading module — search, buy, sell, and trade modals.

mod modals;
mod orders;
mod search;

#[expect(unused_imports, reason = "buy/sell are registered via main.rs when uncommented; kept for re-export path stability")]
#[doc(inline)] pub use orders::{buy, sell};
#[doc(inline)] pub use search::search;
