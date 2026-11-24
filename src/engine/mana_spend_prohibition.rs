//! A barred use of produced mana (CR 106.6): `This mana can't be spent to
//! cast a nonartifact spell` (Powerstone, CR 111.10l). Unlike a permitted
//! use (`castSpell`), the mana may pay for anything except casting a spell
//! the predicate matches: activated abilities, other costs, and the spells
//! outside the predicate.

use super::*;

pub(super) const CAST_SPELL_PROHIBITED: &str = "castSpellProhibited";

/// Whether this restriction is a barred cast use.
pub(super) fn is_cast_spell_prohibition(restriction: &Value) -> bool {
    value_kind(restriction) == Some(CAST_SPELL_PROHIBITED)
}

/// Whether mana under this barred use may pay while `spending_card` is
/// being cast (`None` when no spell is being cast).
pub(super) fn prohibition_permits(
    restriction: &Value,
    spending_card: Option<&CardDefinition>,
) -> bool {
    !spending_card.is_some_and(|card| matches_card_filter(card, &restriction["where"]))
}
