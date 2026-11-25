//! Objects a cost of this ability moved or changed (CR 601.2h, 602.2b),
//! read as last known information when the cost was paid (CR 608.2h).
//!
//! ```text
//! CostPaidProperty := "the" ("sacrificed" | "tapped") NOUN's PROPERTY
//!                   | "the" CARDVERB NOUN's PROPERTY
//!                   | "the" PROPERTY "of" "the" "sacrificed" NOUN
//!                   | "the" PROPERTY "of" "the" "card" "you" "exiled"
//! CARDVERB         := "discarded" | "exiled" | "revealed"
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
//! - `the discarded|exiled|revealed card's PROPERTY`: the one card a chosen
//!   `discardCard`, `exileObject` or `revealCard` cost of the same ability
//!   (or of the same spell face's additional costs) paid with, by its
//!   printed characteristics recorded when the cost was paid
//!   (`src/engine/cost_paid_card_properties.rs`). The binding environment
//!   reads the participle against an earlier action of the program first
//!   (`recognize_participle_object_property`); only without such an action
//!   is the reference this symbolic cost read.
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
    /// The one card a chosen `discardCard` cost discarded.
    DiscardedCard,
    /// The one card a chosen `exileObject` cost exiled from a card zone.
    ExiledCard,
    /// The one card a chosen `revealCard` cost revealed.
    RevealedCard,
}

impl CostObjectAction {
    /// The cost action an object-producing participle names (`the
    /// discarded card`), when a cost can produce that object. `the exiled`
    /// names a chosen card a cost exiled; the source card an `exileSource`
    /// cost exiled is written `the card you exiled`.
    pub(in crate::oracle::canonical) fn from_participle(
        verb: super::ActionResultVerb,
    ) -> Option<Self> {
        match verb {
            super::ActionResultVerb::Sacrificed => Some(Self::Sacrificed),
            super::ActionResultVerb::Tapped => Some(Self::Tapped),
            super::ActionResultVerb::Discarded => Some(Self::DiscardedCard),
            super::ActionResultVerb::Exiled => Some(Self::ExiledCard),
            super::ActionResultVerb::Revealed => Some(Self::RevealedCard),
            super::ActionResultVerb::Destroyed
            | super::ActionResultVerb::Milled
            | super::ActionResultVerb::Returned => None,
        }
    }

    /// A card a cost paid with, read only by a card nominal or a card type.
    const fn names_card(self) -> bool {
        matches!(
            self,
            Self::DiscardedCard | Self::ExiledCard | Self::RevealedCard
        )
    }
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
            (CostObjectAction::DiscardedCard, NumericObjectProperty::Power) => "discardedCardPower",
            (CostObjectAction::DiscardedCard, NumericObjectProperty::Toughness) => {
                "discardedCardToughness"
            }
            (CostObjectAction::DiscardedCard, NumericObjectProperty::ManaValue) => {
                "discardedCardManaValue"
            }
            (CostObjectAction::ExiledCard, NumericObjectProperty::Power) => "exiledCostCardPower",
            (CostObjectAction::ExiledCard, NumericObjectProperty::Toughness) => {
                "exiledCostCardToughness"
            }
            (CostObjectAction::ExiledCard, NumericObjectProperty::ManaValue) => {
                "exiledCostCardManaValue"
            }
            (CostObjectAction::RevealedCard, NumericObjectProperty::Power) => {
                "revealedCostCardPower"
            }
            (CostObjectAction::RevealedCard, NumericObjectProperty::Toughness) => {
                "revealedCostCardToughness"
            }
            (CostObjectAction::RevealedCard, NumericObjectProperty::ManaValue) => {
                "revealedCostCardManaValue"
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

/// A possessive nominal that may name a card a cost paid with: `card` or a
/// card type. A permanent or spell nominal names no card in a hand or a
/// graveyard.
fn card_cost_nominal(nominal: super::TargetNominal) -> bool {
    match nominal {
        super::TargetNominal::Domain(kind) => kind == super::ObjectKind::Card,
        super::TargetNominal::CardType(_) => true,
    }
}

/// The cost-paid property of `verb` written as a participle the binding
/// environment found no producing action for (`the mana value of the
/// exiled card`): the symbolic read card composition binds to the same
/// ability's producing cost (`contextual_binding.rs`), whose absence there
/// is a binding failure. `None` when no cost produces that participle's
/// object, or when the cost records no such characteristic (a tap cost
/// records only the tapped permanent's power).
pub(in crate::oracle::canonical) fn cost_paid_participle_property(
    verb: super::ActionResultVerb,
    property: super::SelectorNumericProperty,
) -> Option<NumericExpression> {
    let action = CostObjectAction::from_participle(verb)?;
    let property = match property {
        super::SelectorNumericProperty::Power => NumericObjectProperty::Power,
        super::SelectorNumericProperty::Toughness => NumericObjectProperty::Toughness,
        super::SelectorNumericProperty::ManaValue => NumericObjectProperty::ManaValue,
    };
    if action == CostObjectAction::Tapped && property != NumericObjectProperty::Power {
        return None;
    }
    Some(NumericExpression::SacrificedCostObjectProperty(
        CostPaidObjectProperty { action, property },
    ))
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
    let action = cursor
        .peek()
        .and_then(super::ActionResultVerb::from_word)
        .and_then(CostObjectAction::from_participle)
        .ok_or_else(|| {
            cursor.fail("`sacrificed`, `tapped`, `discarded`, `exiled` or `revealed`")
        })?;
    let participle = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&participle)?;
    cursor.enter("numeric.cost-object.possessive-object");
    let object = cursor.consume_type("possessiveTypeWord")?;
    cursor.require(
        registered_possessive_object(&object.raw),
        "a registered possessive card type or object domain",
    )?;
    if action.names_card() {
        // `the discarded permanent's power` names no card a cost chose: a
        // card cost's object is read by the card nominal or a card type.
        cursor.enter("numeric.cost-object.card-nominal");
        cursor.require(
            super::possessive_nominal(&object.raw).is_some_and(card_cost_nominal),
            "a card nominal (`card's`) or a card type for a card a cost paid with",
        )?;
    }
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
