//! Definite descriptions of one object (CR 608.2c, 608.2h, 607.2a): the
//! one reader of `the exiled card`, `the sacrificed creature`, `the creature
//! exiled this way` and `the creature that died` as an object operand.
//!
//! ```text
//! Description := "the" Participle Nominal                 the sacrificed creature
//!              | "the" Nominal Participle "this" "way"    the creature exiled this way
//!              | "the" Nominal "that" "died"              the creature that died
//! Participle  := "sacrificed" | "exiled" | "discarded" | "revealed"
//!              | "destroyed" | "milled" | "tapped" | "returned"
//! Nominal     := a registered card type or object domain, not possessive
//! ```
//!
//! A description never declares an object: it restates one a producer in
//! scope made, and the reader returns that object with the kind of its
//! producer. The producers, in the lexical order the program reads them:
//!
//! - the one object an earlier action of the same program VERBed (an
//!   action result published by the binding environment), when its
//!   selection guarantees the nominal;
//! - the object a cost of this ability VERBed (`Sacrifice a creature:
//!   ... the sacrificed creature`), whose paying cost records it as last
//!   known information; card composition proves exactly one producing cost
//!   (`contextual_binding.rs`, `bindObjectAs`) or fails with a binding
//!   diagnostic;
//! - the card a linked ability of the source exiled (`the exiled card`
//!   outside an exile of this program, CR 607.2a); card composition pairs
//!   the reader with exactly one exiling ability of the face or fails with a
//!   dependency diagnostic (`card_composition.rs`);
//! - the event's object that left the battlefield (`the creature that
//!   died`), read as last known information (CR 603.10a).
//!
//! `the chosen card` is not a description of this reader: a choice of a
//! card name is not an object (CR 201.3), and a chosen object has its own
//! choice reader. A participle no producer in scope can make is a committed
//! dependency diagnostic, never a grammar fallback to another reading.
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActionResultVerb, BindingEnvironment, CostPaidConditionObject, GrammaticalNumber, ObjectKind,
    PermanentEffectReference, RelationAnchor, SelectorNumericProperty, TargetNominal,
    TypedObservedEventObject,
};

/// The object a definite description names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DescribedObject {
    /// An object the binding environment names: the result of an earlier
    /// action of the program, or the event's object that left the
    /// battlefield.
    Bound(PermanentEffectReference),
    /// The object a cost of this ability (or of the same spell face) paid
    /// with, as its paying cost recorded it (CR 601.2h, 602.2b, 608.2h).
    CostPaid(CostPaidConditionObject),
    /// The card a linked exile of the source exiled (CR 607.2a), paired by
    /// card composition with exactly one exiling ability of the face.
    LinkedExile,
}

/// The kind of producer a description restates (`docs`: the reader never
/// guesses between several of them; the lexical order above decides).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DescriptionProducer {
    /// An earlier action of the same program.
    ActionResult,
    /// A cost of this ability or of the same spell face.
    CostPaid,
    /// A linked exiling ability of the same face.
    LinkedExile,
    /// The triggering event.
    EventObject,
}

/// A resolved definite description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DefiniteObjectDescription {
    pub object: DescribedObject,
    pub producer: DescriptionProducer,
    pub nominal: TargetNominal,
}

/// The written form of a description, read lexically before resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DescriptionHead {
    /// `the VERBed NOUN`.
    Participle {
        verb: ActionResultVerb,
        nominal: TargetNominal,
    },
    /// `the NOUN VERBed this way`: the object an earlier instruction or a
    /// cost of the same ability VERBed.
    ThisWay {
        verb: ActionResultVerb,
        nominal: TargetNominal,
    },
    /// `the NOUN that died`: the event's object.
    Died { nominal: TargetNominal },
}

/// The diagnostic of a description no producer in scope makes.
pub(in crate::oracle::canonical) const DESCRIPTION_WITHOUT_PRODUCER: &str = "a producer in scope for the definite description: an earlier action of this program that VERBed exactly one object of this nominal, a sacrifice or discard cost of this ability, or a linked exile of the source";

/// The diagnostic of `the NOUN VERBed this way` without that instruction.
pub(in crate::oracle::canonical) const THIS_WAY_WITHOUT_PRODUCER: &str = "an earlier instruction or cost of this ability that VERBed exactly one object of this nominal (`this way`)";

/// The diagnostic of `the NOUN that died` outside a death trigger.
pub(in crate::oracle::canonical) const DIED_WITHOUT_EVENT: &str =
    "a triggering event whose object of this nominal left the battlefield (`that died`)";

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The registered nominal of one plain (non-possessive) noun token at
/// `offset`: a card type or an object domain (`card`, `creature`).
fn plain_nominal_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<TargetNominal> {
    let token = cursor
        .token_slice(cursor.position() + offset, cursor.position() + offset + 1)
        .first()
        .filter(|token| token.token_type != "possessiveTypeWord")?;
    match lookup_object_selector_lexeme(&token.raw) {
        Some(CharacteristicLexeme::CardType(value)) => Some(TargetNominal::CardType(value)),
        Some(CharacteristicLexeme::ObjectDomain(value)) => Some(TargetNominal::Domain(value)),
        _ => None,
    }
}

/// The description written `offset` tokens after the cursor, with its
/// token width. Lexical lookahead only; it proves no producer.
fn description_head_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(DescriptionHead, usize)> {
    if !word_at(cursor, offset, "the") {
        return None;
    }
    if let Some(verb) = cursor
        .peek_ahead(offset + 1)
        .and_then(ActionResultVerb::from_word)
    {
        let nominal = plain_nominal_at(cursor, offset + 2)?;
        return Some((DescriptionHead::Participle { verb, nominal }, 3));
    }
    let nominal = plain_nominal_at(cursor, offset + 1)?;
    if let Some(verb) = cursor
        .peek_ahead(offset + 2)
        .and_then(ActionResultVerb::from_word)
        && word_at(cursor, offset + 3, "this")
        && word_at(cursor, offset + 4, "way")
    {
        return Some((DescriptionHead::ThisWay { verb, nominal }, 5));
    }
    (word_at(cursor, offset + 2, "that") && word_at(cursor, offset + 3, "died"))
        .then_some((DescriptionHead::Died { nominal }, 4))
}

/// Lexical lookahead for a definite description at the cursor.
pub(in crate::oracle::canonical) fn definite_object_description_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    description_head_at(cursor, 0).is_some()
}

/// Lexical lookahead for a definite description `offset` tokens ahead,
/// excluding the prepositive participle of a sacrifice or a tap (`the
/// sacrificed creature`), which the cost-paid property reader keeps.
pub(in crate::oracle::canonical) fn postposed_description_starts_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> bool {
    match description_head_at(cursor, offset) {
        Some((DescriptionHead::Participle { verb, .. }, _)) => !matches!(
            verb,
            ActionResultVerb::Sacrificed | ActionResultVerb::Tapped
        ),
        Some(_) => true,
        None => false,
    }
}

/// Whether the cursor reads an action of an effect program (where an
/// earlier action and a cost of the ability are in scope), not a static
/// ability or an event pattern.
fn in_effect_program(environment: &BindingEnvironment) -> bool {
    environment.effect_index.is_some()
}

/// The earlier action of the program that VERBed exactly one object of the
/// nominal, as the environment publishes it.
fn action_result(
    environment: &BindingEnvironment,
    verb: ActionResultVerb,
    nominal: TargetNominal,
) -> Option<PermanentEffectReference> {
    environment
        .result_object
        .as_ref()
        .filter(|result| {
            result.verb == Some(verb)
                && result.number == GrammaticalNumber::Singular
                && super::selector_names_nominals(&result.selector, &[nominal])
        })
        .map(|result| result.permanent_reference())
}

/// The cost-paid object a participle names when a cost records that object
/// (`bindObjectAs`): a sacrifice pays with a permanent of the named kind, a
/// discard with a card. Any other participle has no cost object record.
fn cost_paid_object(
    verb: ActionResultVerb,
    nominal: TargetNominal,
) -> Result<CostPaidConditionObject, &'static str> {
    match (verb, nominal) {
        (ActionResultVerb::Sacrificed, TargetNominal::CardType(card_type))
            if card_type.is_permanent_type() =>
        {
            Ok(CostPaidConditionObject::SacrificedPermanent)
        }
        (ActionResultVerb::Sacrificed, TargetNominal::Domain(ObjectKind::Permanent)) => {
            Ok(CostPaidConditionObject::SacrificedPermanent)
        }
        (ActionResultVerb::Discarded, TargetNominal::Domain(ObjectKind::Card)) => {
            Ok(CostPaidConditionObject::DiscardedCard)
        }
        (ActionResultVerb::Sacrificed | ActionResultVerb::Discarded, _) => Err(
            "a nominal the paying cost's object has (a permanent kind for a sacrifice, `card` for a discard)",
        ),
        _ => Err(DESCRIPTION_WITHOUT_PRODUCER),
    }
}

/// Resolves a written description against the environment (see the module
/// documentation for the producers and their order).
fn resolve_description(
    environment: &BindingEnvironment,
    head: DescriptionHead,
) -> Result<DefiniteObjectDescription, &'static str> {
    let described = |object, producer, nominal| DefiniteObjectDescription {
        object,
        producer,
        nominal,
    };
    match head {
        DescriptionHead::Participle { verb, nominal }
        | DescriptionHead::ThisWay { verb, nominal } => {
            if let Some(reference) = action_result(environment, verb, nominal) {
                return Ok(described(
                    DescribedObject::Bound(reference),
                    DescriptionProducer::ActionResult,
                    nominal,
                ));
            }
            let this_way = matches!(head, DescriptionHead::ThisWay { .. });
            if verb == ActionResultVerb::Exiled {
                if let Some(expected) = super::participle_card_target_conflict(environment, verb) {
                    return Err(expected);
                }
                // `the card exiled this way` restates an instruction of this
                // ability; a linked exile of another ability is not `this
                // way`.
                if this_way {
                    return Err(THIS_WAY_WITHOUT_PRODUCER);
                }
                return Ok(described(
                    DescribedObject::LinkedExile,
                    DescriptionProducer::LinkedExile,
                    nominal,
                ));
            }
            // A cost exists only for an effect program of an activated
            // ability or a spell; card composition proves it.
            if !in_effect_program(environment) {
                return Err(if this_way {
                    THIS_WAY_WITHOUT_PRODUCER
                } else {
                    DESCRIPTION_WITHOUT_PRODUCER
                });
            }
            cost_paid_object(verb, nominal).map(|object| {
                described(
                    DescribedObject::CostPaid(object),
                    DescriptionProducer::CostPaid,
                    nominal,
                )
            })
        }
        DescriptionHead::Died { nominal } => environment
            .frame
            .event_object
            .as_ref()
            .filter(|slot| {
                slot.departed && super::predicate_names_nominal(&slot.predicate, nominal)
            })
            .map(|slot| {
                described(
                    DescribedObject::Bound(slot.reference),
                    DescriptionProducer::EventObject,
                    nominal,
                )
            })
            .ok_or(DIED_WITHOUT_EVENT),
    }
}

/// Reads a definite description at the cursor and resolves it against
/// `environment`. A description without a producer in scope is a committed
/// diagnostic at its head.
pub(in crate::oracle::canonical) fn recognize_definite_object_description(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
) -> MachineResult<DefiniteObjectDescription> {
    cursor.enter("reference.definite-description.head");
    let (head, width) = cursor.require_value(
        description_head_at(cursor, 0),
        "`the VERBed NOUN`, `the NOUN VERBed this way` or `the NOUN that died`",
    )?;
    cursor.enter("reference.definite-description.producer");
    let description =
        resolve_description(environment, head).map_err(|expected| cursor.fail(expected))?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(description)
}

/// The anchor of a relation of the shared query written as a definite
/// description (`that share a creature type with the sacrificed creature`,
/// `with the same name as the exiled card`). `None` when no description
/// starts at the cursor: the relation's own anchor reader continues.
///
/// The event's departed object is not an anchor: a relation reads its
/// anchor where it is now, and the card a dead creature became is another
/// object (CR 400.7).
pub(in crate::oracle::canonical) fn recognize_described_relation_anchor(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<RelationAnchor>> {
    if !definite_object_description_starts(cursor) {
        return Ok(None);
    }
    let environment = super::installed_binding_environment(cursor);
    let description = recognize_definite_object_description(cursor, &environment)?;
    cursor.enter("selector.relation.anchor.description");
    cursor.require(
        description.producer != DescriptionProducer::EventObject,
        "an anchor that is still the object the description names; the event's object left the battlefield (CR 400.7)",
    )?;
    // An earlier action's result is the bound object every relation
    // already reads; only the recorded and linked objects are new anchors.
    if let DescribedObject::Bound(reference) = description.object {
        cursor.require(
            crate::oracle::canonical::effects::bound_permanent_reference_value(reference, &[])
                .is_some(),
            "a bound anchor object the relation can name at resolution",
        )?;
        return Ok(Some(RelationAnchor::Object(reference)));
    }
    Ok(Some(RelationAnchor::Described(description.object)))
}

/// The numeric property of a described object (`the power of the creature
/// that died`, `the mana value of the card revealed this way`), for the
/// postposed genitive reader. An earlier action's result or the event's
/// object is read live while it is the same object, else as last known
/// information (CR 400.7, 608.2h); a cost-paid card or permanent is read
/// from what its paying cost recorded. A linked exile has no numeric read
/// here and fails explicitly.
pub(in crate::oracle::canonical) fn recognize_described_object_property(
    cursor: &mut OracleTokenCursor<'_>,
    environment: &BindingEnvironment,
    property: SelectorNumericProperty,
) -> MachineResult<super::NumericExpression> {
    let (head, width) = cursor.require_value(
        description_head_at(cursor, 0),
        "`the VERBed NOUN`, `the NOUN VERBed this way` or `the NOUN that died`",
    )?;
    cursor.enter("numeric.definite-description.producer");
    // The cost-paid numeric reads are the cost's recorded characteristics,
    // which exist for every cost participle (a tapped creature's power, an
    // exiled or revealed card's mana value), not only for the costs that
    // record their object.
    let fallback = match head {
        DescriptionHead::Participle { verb, nominal }
        | DescriptionHead::ThisWay { verb, nominal }
            if action_result(environment, verb, nominal).is_none()
                && in_effect_program(environment) =>
        {
            if let Some(expected) = super::participle_card_target_conflict(environment, verb) {
                return Err(cursor.fail(expected));
            }
            let card_nominal = matches!(
                nominal,
                TargetNominal::Domain(ObjectKind::Card) | TargetNominal::CardType(_)
            );
            let permanent_cost = matches!(
                verb,
                ActionResultVerb::Sacrificed | ActionResultVerb::Tapped
            );
            Some(
                (card_nominal || permanent_cost)
                    .then(|| super::cost_paid_participle_property(verb, property))
                    .flatten()
                    .ok_or_else(|| cursor.fail(DESCRIPTION_WITHOUT_PRODUCER))?,
            )
        }
        _ => None,
    };
    if let Some(expression) = fallback {
        cursor.advance_to(cursor.position() + width)?;
        return Ok(expression);
    }
    let description = recognize_definite_object_description(cursor, environment)?;
    cursor.enter("numeric.definite-description.object");
    let DescribedObject::Bound(reference) = description.object else {
        return Err(cursor.fail(
            "a described object whose characteristics this numeric read records (an earlier action's result or the event's object)",
        ));
    };
    if let PermanentEffectReference::Observed(observed) = reference {
        cursor.require(
            matches!(
                observed,
                TypedObservedEventObject::EventPermanent
                    | TypedObservedEventObject::EnteringPermanent
                    | TypedObservedEventObject::MovedOntoBattlefield
                    | TypedObservedEventObject::CombatCounterpart
            ),
            "an event object whose characteristics the runtime records",
        )?;
    }
    super::bound_object_numeric_property(reference, property, description.nominal.as_predicate())
        .map_err(|expected| cursor.fail(expected))
}

#[cfg(test)]
#[path = "definite_object_description_tests.rs"]
mod tests;
