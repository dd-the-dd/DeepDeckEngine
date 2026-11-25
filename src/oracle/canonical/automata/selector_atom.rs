use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, CharacteristicLexeme, ObjectCharacteristic, lookup_basic_land_subtype,
    lookup_card_category, lookup_card_color, lookup_card_type, lookup_color_quantity,
    lookup_creature_subtype_tokens, lookup_noncreature_subtype_forms, lookup_object_characteristic,
    lookup_object_domain, lookup_object_selector_lexeme, lookup_supertype,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectPredicate, characteristic_batch_from_word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PredicateAtomState {
    Atom,
    Complete,
}

/// Recognizes one typed object-characteristic atom while leaving the object
/// domain (`spell`, `permanent`, `card`) to the active parent grammar.
/// Negation is resolved once here for compact Oracle prefixes such as
/// `noncreature`, `non-Wizard`, and `nonblue`.
pub(in crate::oracle::canonical) fn recognize_object_predicate_atom(
    cursor: &mut OracleTokenCursor<'_>,
    forms: AcceptedForms,
) -> MachineResult<ObjectPredicate> {
    let mut state = PredicateAtomState::Atom;
    let mut predicate = None;
    loop {
        state = match state {
            PredicateAtomState::Atom => {
                cursor.enter("selector.predicate-atom.typed");
                let first = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a typed object characteristic"))?;
                let (negated, surface) = split_negated_surface(first);
                cursor.require(!surface.is_empty(), "a characteristic after `non-`")?;
                cursor.require(
                    !bare_attachment_reference(cursor),
                    "a qualifier, not the attached-object reference `equipped|enchanted NOUN` (CR 301.5c, 303.4e)",
                )?;
                cursor.require(
                    !super::participial_relation_starts(cursor),
                    "a characteristic, not the participle of a relation (`blocking it`, `blocked by`)",
                )?;
                let (recognized, width) =
                    recognize_positive_atom(surface, cursor.peek_ahead(1), forms).ok_or_else(
                        || cursor.fail("a registered card type, subtype, color, or supertype"),
                    )?;
                cursor.advance_to(cursor.position() + width)?;
                predicate = Some(if negated {
                    ObjectPredicate::Not(Box::new(recognized))
                } else {
                    recognized
                });
                PredicateAtomState::Complete
            }
            PredicateAtomState::Complete => {
                cursor.enter("selector.predicate-atom.complete");
                return cursor.require_value(predicate, "a populated object predicate atom");
            }
        };
    }
}

pub(super) fn object_predicate_atom_starts(
    cursor: &OracleTokenCursor<'_>,
    forms: AcceptedForms,
) -> bool {
    let Some(first) = cursor.peek() else {
        return false;
    };
    let (_, surface) = split_negated_surface(first);
    !surface.is_empty()
        && !bare_attachment_reference(cursor)
        // `creature blocking it`, `creatures blocked by a Wall`: the status
        // word opens a relation of the noun before it, read by the
        // relative slot, not a characteristic of the noun.
        && !super::participial_relation_starts(cursor)
        && recognize_positive_atom(surface, cursor.peek_ahead(1), forms).is_some()
}

/// `equipped creature` / `enchanted permanent` with no determiner before it
/// and a singular noun after it names the object the source is attached to
/// (CR 301.5c, 303.4e), which the attachment reference owns. After a
/// determiner (`target enchanted permanent`, `an equipped creature`) or
/// before a plural noun (`equipped creatures`) it is the status qualifier.
fn bare_attachment_reference(cursor: &OracleTokenCursor<'_>) -> bool {
    let attachment_status = cursor.peek().is_some_and(|raw| {
        matches!(
            super::ObjectStatus::from_adjective(raw),
            Some(super::ObjectStatus::Enchanted | super::ObjectStatus::Equipped)
        )
    });
    if !attachment_status {
        return false;
    }
    let singular_noun = cursor.peek_ahead(1).is_some_and(|noun| {
        recognize_positive_atom(&noun, None, AcceptedForms::SingularOnly).is_some()
    });
    let determined = cursor
        .position()
        .checked_sub(1)
        .and_then(|previous| cursor.token_slice(previous, previous + 1).first())
        .is_some_and(|token| {
            matches!(
                token.raw.to_ascii_lowercase().as_str(),
                "a" | "an"
                    | "target"
                    | "each"
                    | "every"
                    | "another"
                    | "other"
                    | "all"
                    | "any"
                    | "of"
                    | "no"
                    | "that"
                    | "two"
                    | "three"
                    | "x"
            )
        });
    singular_noun && !determined
}

/// Compatibility entry point for parents that already own the complete
/// selector word, including explicit object-domain nouns. It shares the same
/// central vocabulary as the consuming atom machine above.
pub(super) fn positive_object_selector_atom(raw: &str) -> Option<ObjectPredicate> {
    let lexeme = lookup_object_selector_lexeme(raw).or_else(|| {
        lookup_basic_land_subtype(raw, AcceptedForms::SingularOrPlural)
            .map(CharacteristicLexeme::BasicLandSubtype)
    })?;
    match lexeme {
        CharacteristicLexeme::CardType(card_type) => Some(ObjectPredicate::CardType(card_type)),
        CharacteristicLexeme::Supertype(supertype) => Some(ObjectPredicate::Supertype(supertype)),
        CharacteristicLexeme::ObjectDomain(domain) => Some(ObjectPredicate::ObjectKind(domain)),
        CharacteristicLexeme::ObjectCharacteristic(characteristic) => match characteristic {
            ObjectCharacteristic::Commander => Some(ObjectPredicate::IsCommander),
            ObjectCharacteristic::Token => Some(ObjectPredicate::IsToken),
        },
        CharacteristicLexeme::BasicLandSubtype(subtype) => {
            Some(ObjectPredicate::Subtype(subtype.ir_name().to_string()))
        }
        CharacteristicLexeme::CardCategory(_) => None,
    }
}

/// The grammatical number a noun surface shows. `Unknown` is an invariant
/// noun (`Equipment`, `Merfolk`, `Plains`, `Starfish`) that agrees with
/// either number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GrammaticalNumber {
    Singular,
    Plural,
    Unknown,
}

/// `card(s)`, `spell(s)`, `permanent(s)`: a domain noun. Trailing a
/// coordinated list (`creature, land, or planeswalker cards`), it takes the
/// whole list's number, so it says nothing about one member's number.
pub(super) fn atom_surface_is_domain_noun(first: &str) -> bool {
    lookup_object_domain(first, AcceptedForms::SingularOrPlural).is_some()
}

/// The number of the noun at this surface, or None for a word that is not
/// an object noun (a color, a supertype, a status adjective).
pub(super) fn atom_surface_number(first: &str, second: Option<&str>) -> Option<GrammaticalNumber> {
    let (_, surface) = split_negated_surface(first);
    if surface.is_empty() {
        return None;
    }
    let (predicate, _) = recognize_positive_atom(surface, second, AcceptedForms::SingularOrPlural)?;
    let noun = matches!(
        predicate,
        ObjectPredicate::CardType(_)
            | ObjectPredicate::ObjectKind(_)
            | ObjectPredicate::Subtype(_)
            | ObjectPredicate::IsToken
            | ObjectPredicate::IsCommander
    );
    if !noun {
        return None;
    }
    if recognize_positive_atom(surface, second, AcceptedForms::SingularOnly).is_none() {
        return Some(GrammaticalNumber::Plural);
    }
    Some(match predicate {
        // A creature subtype is singular only when Oracle prints a distinct
        // plural for it; noncreature and land subtypes are invariant in
        // Oracle lists (`Equipment`, `Plains`).
        ObjectPredicate::Subtype(name) => {
            if crate::oracle::characteristic_vocabulary::lookup_creature_subtype(
                &name,
                AcceptedForms::SingularOnly,
            )
            .is_some()
                && crate::oracle::characteristic_vocabulary::creature_subtype_has_distinct_plural(
                    &name,
                )
            {
                GrammaticalNumber::Singular
            } else {
                GrammaticalNumber::Unknown
            }
        }
        _ => GrammaticalNumber::Singular,
    })
}

fn split_negated_surface(raw: &str) -> (bool, &str) {
    let normalized = raw.to_ascii_lowercase();
    if normalized.starts_with("non-") {
        (true, &raw[4..])
    } else if normalized.starts_with("non") {
        (true, &raw[3..])
    } else {
        (false, raw)
    }
}

fn recognize_positive_atom(
    first: &str,
    second: Option<&str>,
    forms: AcceptedForms,
) -> Option<(ObjectPredicate, usize)> {
    if let Some(card_type) = lookup_card_type(first, forms) {
        return Some((ObjectPredicate::CardType(card_type), 1));
    }
    if let Some(status) = super::lookup_status_predicate(first) {
        return Some((status, 1));
    }
    if let Some(color) = lookup_card_color(first) {
        return Some((ObjectPredicate::Color(color), 1));
    }
    if let Some(quantity) = lookup_color_quantity(first) {
        return Some((ObjectPredicate::ColorQuantity(quantity), 1));
    }
    if let Some(subtype) = lookup_basic_land_subtype(first, forms) {
        return Some((ObjectPredicate::Subtype(subtype.ir_name().to_string()), 1));
    }
    if let Some(supertype) = lookup_supertype(first, AcceptedForms::SingularOnly) {
        return Some((ObjectPredicate::Supertype(supertype), 1));
    }
    // CR 110.4b: `permanent card` names a card with a permanent card type,
    // and `permanent spell` the spell of such a card. The following card or
    // spell noun is read as the object domain by the parent.
    if lookup_card_category(first, AcceptedForms::SingularOnly).is_some()
        && second.is_some_and(|noun| {
            ["card", "cards", "spell", "spells"]
                .iter()
                .any(|domain| noun.eq_ignore_ascii_case(domain))
        })
    {
        return Some((ObjectPredicate::PermanentCard, 1));
    }
    if let Some(domain) = lookup_object_domain(first, forms) {
        return Some((ObjectPredicate::ObjectKind(domain), 1));
    }
    if let Some(batch) = characteristic_batch_from_word(first) {
        return Some((ObjectPredicate::CharacteristicBatch(batch), 1));
    }
    if let Some(characteristic) = lookup_object_characteristic(first, forms) {
        return Some((
            match characteristic {
                ObjectCharacteristic::Commander => ObjectPredicate::IsCommander,
                ObjectCharacteristic::Token => ObjectPredicate::IsToken,
            },
            1,
        ));
    }
    if let Some((subtype, width)) = lookup_creature_subtype_tokens(first, second, forms) {
        return Some((
            ObjectPredicate::Subtype(subtype.ir_name().to_string()),
            width,
        ));
    }
    // CR 205.3: artifact, enchantment, land, planeswalker, battle and spell
    // subtypes are object-head nouns of the same kind as creature types.
    lookup_noncreature_subtype_forms(first, forms)
        .map(|subtype| (ObjectPredicate::Subtype(subtype.to_string()), 1))
}

#[cfg(test)]
#[path = "selector_atom_tests.rs"]
mod tests;
