//! Objects a cost of this ability moved or changed (CR 601.2h, 602.2b),
//! read as last known information when the cost was paid (CR 608.2h).
//!
//! ```text
//! CostPaidProperty := "the" ("sacrificed" | "tapped") NOUN's PROPERTY
//!                   | "the" PROPERTY "of" "the" "sacrificed" NOUN
//!                   | "the" PROPERTY "of" "the" "card" "you" "exiled"
//! PROPERTY         := "power" | "toughness" | "mana" "value"
//! ```
//!
//! The reference is symbolic while an ability is parsed: the cost that
//! produces it belongs to the same ability (or, for a sacrifice, to the same
//! spell face) and is found after card composition
//! (`contextual_binding.rs`). A missing or ambiguous producer is a binding
//! failure, never a zero.
//!
//! - `the sacrificed NOUN's PROPERTY` and its postposed genitive `the
//!   PROPERTY of the sacrificed NOUN`: the permanent a `sacrificePermanent`
//!   cost sacrificed (`bindPowerAs` / `bindToughnessAs` / `bindManaValueAs`
//!   on that cost; mana value per CR 202.3).
//! - `the tapped NOUN's power`: the permanent a chosen-permanent `tap` cost
//!   tapped (station, CR 702.184a: `the tapped creature's power`). The
//!   runtime tap cost records only power, so a toughness read is refused.
//! - `the PROPERTY of the card you exiled`: the card an `exileSource` cost
//!   exiled, which is the ability's own source card (scavenge, CR 702.97a).
//!   Only its power and toughness are read.
//!   It is rewritten to the source's property once the producing cost is
//!   proven; the card keeps its characteristics in exile (CR 400.7 does not
//!   change what that card is, only that it is a new object).
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericExpression, NumericObjectProperty, registered_possessive_object};

/// The cost action whose object a reference names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostObjectAction {
    Sacrificed,
    Tapped,
    /// The source card an `exileSource` cost exiled.
    ExiledSourceCard,
}

/// A characteristic of an object a cost of this ability paid with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CostPaidObjectProperty {
    pub action: CostObjectAction,
    pub property: NumericObjectProperty,
}

impl CostPaidObjectProperty {
    /// The symbolic stack value the producing cost records, resolved after
    /// card composition against the same ability's costs.
    pub(in crate::oracle::canonical) const fn binding(self) -> &'static str {
        match (self.action, self.property) {
            (CostObjectAction::Sacrificed, NumericObjectProperty::Power) => {
                "sacrificedPermanentPower"
            }
            (CostObjectAction::Sacrificed, NumericObjectProperty::Toughness) => {
                "sacrificedPermanentToughness"
            }
            (CostObjectAction::Sacrificed, NumericObjectProperty::ManaValue) => {
                "sacrificedPermanentManaValue"
            }
            (CostObjectAction::Tapped, NumericObjectProperty::Power) => "tappedPermanentPower",
            (CostObjectAction::Tapped, NumericObjectProperty::Toughness) => {
                "tappedPermanentToughness"
            }
            (CostObjectAction::Tapped, NumericObjectProperty::ManaValue) => {
                "tappedPermanentManaValue"
            }
            (CostObjectAction::ExiledSourceCard, NumericObjectProperty::Power) => {
                "exiledSourceCardPower"
            }
            (CostObjectAction::ExiledSourceCard, NumericObjectProperty::Toughness) => {
                "exiledSourceCardToughness"
            }
            (CostObjectAction::ExiledSourceCard, NumericObjectProperty::ManaValue) => {
                "exiledSourceCardManaValue"
            }
        }
    }
}

fn word(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn property_word(raw: &str) -> Option<NumericObjectProperty> {
    if raw.eq_ignore_ascii_case("power") {
        Some(NumericObjectProperty::Power)
    } else if raw.eq_ignore_ascii_case("toughness") {
        Some(NumericObjectProperty::Toughness)
    } else {
        None
    }
}

/// The property written at `offset` and its width in words (`mana value`
/// is two). Lexical lookahead only.
fn property_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(NumericObjectProperty, usize)> {
    if word(cursor, offset, "mana") && word(cursor, offset + 1, "value") {
        return Some((NumericObjectProperty::ManaValue, 2));
    }
    cursor
        .peek_ahead(offset)
        .and_then(property_word)
        .map(|property| (property, 1))
}

/// Consumes the property `property_at` proves at the cursor.
fn consume_property(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericObjectProperty> {
    let Some((property, width)) = property_at(cursor, 0) else {
        return Err(cursor.fail("`power`, `toughness`, or `mana value`"));
    };
    if width == 2 {
        cursor.consume_phrase(&["mana", "value"])?;
    } else {
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
    }
    Ok(property)
}

/// Lexical lookahead for a cost-paid object property.
pub(in crate::oracle::canonical) fn cost_paid_object_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if !word(cursor, 0, "the") {
        return false;
    }
    if word(cursor, 1, "sacrificed") || word(cursor, 1, "tapped") {
        return true;
    }
    let Some((_, width)) = property_at(cursor, 1) else {
        return false;
    };
    let of = 1 + width;
    word(cursor, of, "of")
        && word(cursor, of + 1, "the")
        && (word(cursor, of + 2, "sacrificed")
            || (word(cursor, of + 2, "card")
                && word(cursor, of + 3, "you")
                && word(cursor, of + 4, "exiled")))
}

/// Reads a cost-paid object property (see the module grammar).
pub(in crate::oracle::canonical) fn recognize_cost_paid_object_property(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.cost-object.reference");
    cursor.consume("the")?;
    if property_at(cursor, 0).is_some() {
        cursor.enter("numeric.cost-object.postposed-property");
        let property = consume_property(cursor)?;
        cursor.consume_phrase(&["of", "the"])?;
        if word(cursor, 0, "sacrificed") {
            // `the mana value of the sacrificed permanent`: the postposed
            // genitive of the same paid-cost object.
            cursor.enter("numeric.cost-object.postposed-sacrificed");
            cursor.consume("sacrificed")?;
            let noun = cursor
                .peek()
                .ok_or_else(|| cursor.fail("a card type or object domain"))?
                .to_owned();
            cursor.require(
                matches!(
                    lookup_object_selector_lexeme(&noun),
                    Some(CharacteristicLexeme::CardType(_) | CharacteristicLexeme::ObjectDomain(_))
                ),
                "a registered card type or object domain",
            )?;
            cursor.consume(&noun)?;
            return Ok(NumericExpression::SacrificedCostObjectProperty(
                CostPaidObjectProperty {
                    action: CostObjectAction::Sacrificed,
                    property,
                },
            ));
        }
        // `the power of the card you exiled` (CR 702.97a).
        cursor.enter("numeric.cost-object.exiled-card");
        cursor.require(
            property != NumericObjectProperty::ManaValue,
            "the exiled card's power or toughness",
        )?;
        cursor.consume_phrase(&["card", "you", "exiled"])?;
        return Ok(NumericExpression::SacrificedCostObjectProperty(
            CostPaidObjectProperty {
                action: CostObjectAction::ExiledSourceCard,
                property,
            },
        ));
    }
    let action = if word(cursor, 0, "tapped") {
        cursor.consume("tapped")?;
        CostObjectAction::Tapped
    } else {
        cursor.consume("sacrificed")?;
        CostObjectAction::Sacrificed
    };
    cursor.enter("numeric.cost-object.possessive-object");
    let object = cursor.consume_type("possessiveTypeWord")?;
    cursor.require(
        registered_possessive_object(&object.raw),
        "a registered possessive card type or object domain",
    )?;
    cursor.enter("numeric.cost-object.property");
    let property = consume_property(cursor)?;
    if action == CostObjectAction::Tapped {
        cursor.enter("numeric.cost-object.tapped-property");
        cursor.require(
            property == NumericObjectProperty::Power,
            "the tapped permanent's power (a tap cost records no other characteristic)",
        )?;
    }
    Ok(NumericExpression::SacrificedCostObjectProperty(
        CostPaidObjectProperty { action, property },
    ))
}
