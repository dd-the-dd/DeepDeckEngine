use crate::oracle::{
    characteristic_vocabulary::ObjectDomain,
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CollectionBinding, EventNumericBinding, LocalObjectReference, LocalTriggerEventNode,
    NumericExpression, ObjectPredicate, recognize_combat_damage_to_player_event,
    recognize_object_query, recognize_that_many_event_value,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OptionalBoundMillRecoveryNode {
    pub count: NumericExpression,
    pub binding: CollectionBinding,
    pub filter: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CombatDamageOptionalMillAbilityNode {
    pub source: LocalObjectReference,
    pub event_amount: EventNumericBinding,
    pub recovery: OptionalBoundMillRecoveryNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Trigger,
    Event,
    Separator,
    OptionalSubject,
    OptionalMarker,
    MillAction,
    MillCount,
    MillNoun,
    SentenceBoundary,
    ReflexiveCondition,
    ReflexiveSubject,
    RecoveryOptionalMarker,
    RecoveryAction,
    RecoveryArticle,
    RecoveryFilter,
    BoundCollectionRelation,
    BoundCollectionReference,
    Destination,
    Terminator,
    Complete,
}

#[derive(Default)]
struct Frame {
    source: Option<LocalObjectReference>,
    event_amount: Option<EventNumericBinding>,
    count: Option<NumericExpression>,
    filter: Option<ObjectPredicate>,
}

/// Recognizes an optional event-sized mill and its reflexive optional recovery
/// from the cards produced by that mill. The collection binding is explicit;
/// `them`/`those cards` cannot resolve to another zone or effect.
pub(in crate::oracle::canonical) fn recognize_combat_damage_optional_mill_ability(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<CombatDamageOptionalMillAbilityNode> {
    let mut state = State::Trigger;
    let mut frame = Frame::default();
    loop {
        state = match state {
            State::Trigger => {
                cursor.enter("trigger.combat-damage-mill.trigger");
                cursor.consume("whenever")?;
                State::Event
            }
            State::Event => {
                cursor.enter("trigger.combat-damage-mill.event");
                let event =
                    recognize_combat_damage_to_player_event(cursor, name_links, name_failure)?;
                let LocalTriggerEventNode::CombatDamageToPlayer { source, amount } = event else {
                    return Err(cursor.fail("a combat-damage-to-player event"));
                };
                frame.source = Some(source);
                frame.event_amount = Some(amount);
                State::Separator
            }
            State::Separator => {
                cursor.enter("trigger.combat-damage-mill.separator");
                cursor.consume(",")?;
                State::OptionalSubject
            }
            State::OptionalSubject => {
                cursor.enter("effect.optional-mill.player");
                cursor.consume("you")?;
                State::OptionalMarker
            }
            State::OptionalMarker => {
                cursor.enter("effect.optional-mill.marker");
                cursor.consume("may")?;
                State::MillAction
            }
            State::MillAction => {
                cursor.enter("effect.optional-mill.action");
                cursor.consume("mill")?;
                State::MillCount
            }
            State::MillCount => {
                cursor.enter("effect.optional-mill.count");
                frame.count = Some(recognize_that_many_event_value(cursor, frame.event_amount)?);
                State::MillNoun
            }
            State::MillNoun => {
                cursor.enter("effect.optional-mill.noun");
                cursor.consume("cards")?;
                State::SentenceBoundary
            }
            State::SentenceBoundary => {
                cursor.enter("effect.optional-mill.sentence-boundary");
                cursor.consume(".")?;
                State::ReflexiveCondition
            }
            State::ReflexiveCondition => {
                cursor.enter("condition.optional-action-performed");
                cursor.consume_phrase(&["if", "you", "do", ","])?;
                State::ReflexiveSubject
            }
            State::ReflexiveSubject => {
                cursor.enter("effect.optional-recovery.player");
                cursor.consume("you")?;
                State::RecoveryOptionalMarker
            }
            State::RecoveryOptionalMarker => {
                cursor.enter("effect.optional-recovery.marker");
                cursor.consume("may")?;
                State::RecoveryAction
            }
            State::RecoveryAction => {
                cursor.enter("effect.optional-recovery.action");
                cursor.consume("put")?;
                State::RecoveryArticle
            }
            State::RecoveryArticle => {
                cursor.enter("effect.optional-recovery.article");
                let article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .ok_or_else(|| cursor.fail("`a` or `an` before a card selector"))?
                    .to_string();
                cursor.consume(&article)?;
                State::RecoveryFilter
            }
            State::RecoveryFilter => {
                cursor.enter("effect.optional-recovery.filter");
                let query = recognize_object_query(cursor)?;
                cursor.require(query.controller.is_none(), "an unscoped card selector")?;
                cursor.require(
                    query.predicate.has_domain(ObjectDomain::Card),
                    "a selector with the explicit card domain",
                )?;
                frame.filter = Some(query.predicate);
                State::BoundCollectionRelation
            }
            State::BoundCollectionRelation => {
                cursor.enter("reference.bound-milled-cards.relation");
                cursor.consume_phrase(&["from", "among"])?;
                State::BoundCollectionReference
            }
            State::BoundCollectionReference => {
                cursor.enter("reference.bound-milled-cards.anaphor");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("them"))
                {
                    cursor.consume("them")?;
                } else {
                    cursor.consume_phrase(&["those", "cards"])?;
                }
                State::Destination
            }
            State::Destination => {
                cursor.enter("effect.optional-recovery.destination");
                cursor.consume_phrase(&["into", "your", "hand"])?;
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("effect.optional-recovery.terminator");
                cursor.consume(".")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("trigger.combat-damage-mill.complete");
                cursor.finish()?;
                return Ok(CombatDamageOptionalMillAbilityNode {
                    source: cursor.require_value(frame.source, "the combat-damage source")?,
                    event_amount: cursor
                        .require_value(frame.event_amount, "the event damage amount")?,
                    recovery: OptionalBoundMillRecoveryNode {
                        count: cursor.require_value(frame.count, "the mill count")?,
                        binding: CollectionBinding(0),
                        filter: cursor.require_value(frame.filter, "the recovery card filter")?,
                    },
                });
            }
        };
    }
}
