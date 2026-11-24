//! Partner abilities (CR 702.124a) modify Commander deck construction and
//! function before the game begins. During the game they have no behavior:
//! the engine accepts the typed keyword as a characteristic of the object
//! and attaches no trigger, permission, cost or modifier to it.
use super::*;

/// `partner`, `partner—[text]`, `choose a Background`, `Doctor's
/// companion` and `Spell commander`, each explicitly scoped to deck
/// construction by the compiler.
pub(super) fn deck_construction_keyword_supported(ability: &Value) -> bool {
    ability["rulesScope"].as_str() == Some("deckConstruction")
        && match value_kind(ability) {
            Some("partner") => ability
                .get("variant")
                .is_none_or(|variant| variant.as_str().is_some_and(|name| !name.trim().is_empty())),
            Some("chooseABackground" | "doctorsCompanion" | "spellCommander") => {
                ability.get("variant").is_none()
            }
            _ => false,
        }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "deck_construction_keywords_tests.rs"]
mod tests;
