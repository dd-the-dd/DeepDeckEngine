//! Cross-crate Oracle IR to engine executability contracts.
//!
//! This package joins the public parser and parser-free runtime boundaries. It
//! must not compile a private second copy of either implementation.

pub use deepdeck_engine_runtime::engine;
pub use deepdeck_oracle::{card_catalog, oracle};
