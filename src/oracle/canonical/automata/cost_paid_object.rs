//! Objects a cost of this ability moved or changed (CR 601.2h, 602.2b),
//! read as last known information when the cost was paid (CR 608.2h).
//!
//! ```text
//! CostPaidProperty := "the" ("sacrificed" | "tapped") NOUN's PROPERTY
//!                   | "the" PROPERTY "of" "the" "card" "you" "exiled"
//! PROPERTY         := "power" | "toughness"
//! ```
//!
//! The reference is symbolic while an ability is parsed: the cost that
//! produces it belongs to the same ability (or, for a sacrifice, to the same
//! spell face) and is found after card composition
//! (`contextual_binding.rs`). A missing or ambiguous producer is a binding
//! failure, never a zero.
//!
//! - `the sacrificed NOUN's PROPERTY`: the permanent a `sacrificePermanent`
//!   cost sacrificed (`bindPowerAs` / `bindToughnessAs` on that cost).
//! - `the tapped NOUN's power`: the permanent a chosen-permanent `tap` cost
//!   tapped (station, CR 702.184a: `the tapped creature's power`). The
//!   runtime tap cost records only power, so a toughness read is refused.
//! - `the PROPERTY of the card you exiled`: the card an `exileSource` cost
//!   exiled, which is the ability's own source card (scavenge, CR 702.97a).
//!   It is rewritten to the source's property once the producing cost is
//!   proven; the card keeps its characteristics in exile (CR 400.7 does not
//!   change what that card is, only that it is a new object).
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
            (CostObjectAction::Tapped, NumericObjectProperty::Power) => "tappedPermanentPower",
            (CostObjectAction::Tapped, NumericObjectProperty::Toughness) => {
                "tappedPermanentToughness"
            }
            (CostObjectAction::ExiledSourceCard, NumericObjectProperty::Power) => {
                "exiledSourceCardPower"
            }
            (CostObjectAction::ExiledSourceCard, NumericObjectProperty::Toughness) => {
                "exiledSourceCardToughness"
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

/// Lexical lookahead for a cost-paid object property.
pub(in crate::oracle::canonical) fn cost_paid_object_property_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word(cursor, 0, "the")
        && (word(cursor, 1, "sacrificed")
            || word(cursor, 1, "tapped")
            || (cursor.peek_ahead(1).and_then(property_word).is_some()
                && word(cursor, 2, "of")
                && word(cursor, 3, "the")
                && word(cursor, 4, "card")
                && word(cursor, 5, "you")
                && word(cursor, 6, "exiled")))
}

/// Reads a cost-paid object property (see the module grammar).
pub(in crate::oracle::canonical) fn recognize_cost_paid_object_property(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.cost-object.reference");
    cursor.consume("the")?;
    if let Some(property) = cursor.peek().and_then(property_word) {
        // `the power of the card you exiled` (CR 702.97a).
        cursor.enter("numeric.cost-object.exiled-card");
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
        cursor.consume_phrase(&["of", "the", "card", "you", "exiled"])?;
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
    let token = cursor.consume_type("propertyWord")?;
    let property =
        property_word(&token.raw).ok_or_else(|| cursor.fail("`power` or `toughness`"))?;
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
