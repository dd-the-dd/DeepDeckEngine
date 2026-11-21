//! Parser-free model boundary used by the deterministic engine runtime.
//!
//! Oracle compilation belongs to the parser/runtime contract crate. The core
//! runtime can still execute already-compiled rules; dynamic catalog
//! compilation fails explicitly when that optional capability is absent.

use crate::engine::CardDefinition;
use serde_json::Value;

const ORACLE_COMPILATION_DISABLED: &str =
    "Oracle compilation is unavailable in the parser-free engine runtime";

pub fn compile_named_token_definition(name: &str) -> Result<Option<CardDefinition>, String> {
    let normalized = name.trim().to_ascii_lowercase();
    Ok(crate::game_piece_artifact::embedded_artifact()?
        .definitions
        .remove(&normalized))
}

/// The parser-free runtime cannot compile the embedded dungeon Oracle text.
/// Venturing then uses only the outside-game pieces already supplied by the
/// game state, exactly as a caller-provided setup defines them.
pub fn builtin_dungeon_definitions() -> Result<&'static [CardDefinition], String> {
    Ok(&[])
}

pub fn compile_named_card_definition(_name: &str) -> Result<Option<CardDefinition>, String> {
    Err(ORACLE_COMPILATION_DISABLED.to_string())
}

pub fn compile_related_token_definition(_token: &Value) -> Result<Option<CardDefinition>, String> {
    Err(ORACLE_COMPILATION_DISABLED.to_string())
}
