//! Standalone deterministic simulation runtime.
//!
//! During the staged extraction these modules still live at their historical
//! paths. Compiling this package excludes HTTP, sessions, analytics, and the
//! application server while retaining the engine's private unit tests.

#[path = "../../../src/card_catalog.rs"]
pub mod card_catalog;

#[cfg(feature = "oracle-compilation")]
pub use deepdeck_oracle::oracle;

#[path = "../../../src/engine/mod.rs"]
pub mod engine;

pub mod game_piece_artifact;
#[path = "../../../src/game_rules.rs"]
pub mod game_rules;

#[cfg(feature = "oracle-compilation")]
#[path = "../../../src/model.rs"]
pub mod model;

#[cfg(not(feature = "oracle-compilation"))]
#[path = "../../../src/model_runtime.rs"]
pub mod model;
