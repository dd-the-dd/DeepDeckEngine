//! A permitted union of uses of produced mana (CR 106.6) beyond the
//! historical cast-only and same-object shapes: at most one cast use (the
//! spells matching `spells`) and one ability use (abilities of sources
//! matching `abilitySources`, or of any source with `any`).
//!
//! ```text
//! { kind: castSpellOrActivateAbilityOf, spells?: FILTER,
//!   abilitySources?: FILTER | "any" }
//! ```

use super::*;

pub(super) const CAST_SPELL_OR_ACTIVATE_ABILITY_OF: &str = "castSpellOrActivateAbilityOf";

/// Whether this restriction is the general union.
pub(super) fn is_use_union(restriction: &Value) -> bool {
    value_kind(restriction) == Some(CAST_SPELL_OR_ACTIVATE_ABILITY_OF)
}

/// A union names at least one use, each with a supported filter.
pub(super) fn use_union_supported(restriction: &Value) -> bool {
    is_use_union(restriction)
        && (restriction.get("spells").is_some() || restriction.get("abilitySources").is_some())
        && restriction.get("spells").is_none_or(card_filter_supported)
        && restriction
            .get("abilitySources")
            .is_none_or(|sources| sources.as_str() == Some("any") || card_filter_supported(sources))
}

/// Whether mana under the union may pay while `spending_card` is cast, or
/// (with no spell being cast) while an ability of `ability_source` is
/// activated. Any other payment is outside every use.
pub(super) fn use_union_permits(
    restriction: &Value,
    spending_card: Option<&CardDefinition>,
    ability_source: Option<&CardDefinition>,
) -> bool {
    match spending_card {
        Some(card) => restriction
            .get("spells")
            .is_some_and(|spells| matches_card_filter(card, spells)),
        None => ability_source.is_some_and(|source| match restriction.get("abilitySources") {
            Some(sources) if sources.as_str() == Some("any") => true,
            Some(sources) => matches_card_filter(source, sources),
            None => false,
        }),
    }
}
