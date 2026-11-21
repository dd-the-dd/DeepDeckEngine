//! Cross-crate Oracle IR to engine executability contracts.
//!
//! This package joins the public parser and parser-free runtime boundaries. It
//! must not compile a private second copy of either implementation.

pub use deepdeck_engine_runtime::engine;
pub use deepdeck_oracle::{card_catalog, oracle};

/// The card adapter joins these public parser/runtime contracts here, without
/// enabling the runtime package's optional Oracle compilation feature.
#[path = "../../../src/model.rs"]
pub mod model;
