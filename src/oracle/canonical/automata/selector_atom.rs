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

/// True when the characteristic word at this surface is only recognized in its
/// plural form (`Rogues`, `creatures`): such a word is a nominal head, while
/// its singular form inside a coordination can be a prenominal modifier.
pub(super) fn atom_surface_is_plural(first: &str, second: Option<&str>) -> bool {
    let (_, surface) = split_negated_surface(first);
    !surface.is_empty()
        && recognize_positive_atom(surface, second, AcceptedForms::SingularOnly).is_none()
        && recognize_positive_atom(surface, second, AcceptedForms::SingularOrPlural).is_some()
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
    // CR 110.4b: `permanent card` names a card with a permanent card type.
    // The following card noun is read as the object domain by the parent.
    if lookup_card_category(first, AcceptedForms::SingularOnly).is_some()
        && second.is_some_and(|noun| {
            noun.eq_ignore_ascii_case("card") || noun.eq_ignore_ascii_case("cards")
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
