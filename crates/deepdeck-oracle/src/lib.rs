//! Standalone Oracle-text parser boundary.
//!
//! The source modules remain shared with the engine crate during the staged
//! extraction. This package gives parser work an independent compilation and
//! test target now; physical source relocation can happen incrementally.

#[path = "../../../src/card_catalog.rs"]
pub mod card_catalog;

pub mod oracle;

#[cfg(test)]
pub use deepdeck_engine_runtime::engine;
