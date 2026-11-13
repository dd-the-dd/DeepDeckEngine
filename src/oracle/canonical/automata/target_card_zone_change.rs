use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ExpressedTargetCardinality, ObjectKind, ObjectPredicate, recognize_card_type_union_selector,
    recognize_expressed_target_cardinality,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneChangeAction {
    Exile,
    Move(CardZoneDestination),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneDestination {
    ControllerHand,
    TargetOwnerLibraryTop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetCardSourceZone {
    AnyGraveyard,
    SingleGraveyard,
    ControllerGraveyard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetCardZoneChangeEffectNode {
    pub action: CardZoneChangeAction,
    pub cardinality: ExpressedTargetCardinality,
    pub source_zone: TargetCardSourceZone,
    pub predicate: ObjectPredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetCardZoneChangeState {
    Action,
    Cardinality,
    TargetMarker,
    TargetObject,
    SourceRelation,
    SourceArticle,
    SourceZone,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraveyardZoneSpelling {
    Singular,
    Plural,
}

#[derive(Default)]
struct TargetCardZoneChangeAttemptFrame {
    action: Option<CardZoneChangeAction>,
    cardinality: Option<ExpressedTargetCardinality>,
    source_zone: Option<TargetCardSourceZone>,
    zone_spelling: Option<GraveyardZoneSpelling>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReturnTargetCardState {
    Action,
    Target,
    Selector,
    SourceRelation,
    SourceOwner,
    SourceZone,
    DestinationRelation,
    DestinationOwner,
    DestinationZone,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PutTargetCardState {
    Action,
    Target,
    Selector,
    SourceRelation,
    SourceArticle,
    SourceZone,
    DestinationRelation,
    DestinationPosition,
    DestinationOf,
    DestinationObject,
    DestinationOwner,
    DestinationZone,
    Complete,
}

/// Recognizes one targeted card move from a nonbattlefield zone. Ability
/// envelopes own triggers, activation costs, punctuation, and full input
/// consumption; this child owns only the independent zone-change effect.
pub(in crate::oracle::canonical) fn recognize_target_card_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCardZoneChangeEffectNode> {
    match cursor.peek() {
        Some(raw) if raw.eq_ignore_ascii_case("exile") => {
            recognize_exile_target_card_zone_change_effect(cursor)
        }
        Some(raw) if raw.eq_ignore_ascii_case("return") => {
            recognize_return_target_card_zone_change_effect(cursor)
        }
        Some(raw) if raw.eq_ignore_ascii_case("put") => {
            recognize_put_target_card_zone_change_effect(cursor)
        }
        _ => Err(cursor.fail("`exile`, `return`, or `put` as a targeted card-zone action")),
    }
}

fn recognize_target_card_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
    {
        cursor.enter("selector.target-card.domain");
        cursor.consume("card")?;
        return Ok(ObjectPredicate::ObjectKind(ObjectKind::Card));
    }
    recognize_card_type_union_selector(cursor)
}

fn recognize_exile_target_card_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCardZoneChangeEffectNode> {
    let mut state = TargetCardZoneChangeState::Action;
    let mut frame = TargetCardZoneChangeAttemptFrame::default();
    loop {
        state = match state {
            TargetCardZoneChangeState::Action => {
                cursor.enter("effect.target-card-zone-change.action");
                let action_token = cursor.consume_type("actionWord")?;
                cursor.require(
                    action_token.raw.eq_ignore_ascii_case("exile"),
                    "the `exile` card-zone action",
                )?;
                frame.action = Some(CardZoneChangeAction::Exile);
                TargetCardZoneChangeState::Cardinality
            }
            TargetCardZoneChangeState::Cardinality => {
                cursor.enter("effect.target-card-zone-change.cardinality");
                frame.cardinality = Some(
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
                    {
                        ExpressedTargetCardinality::EXACTLY_ONE
                    } else {
                        recognize_expressed_target_cardinality(cursor)?
                    },
                );
                TargetCardZoneChangeState::TargetMarker
            }
            TargetCardZoneChangeState::TargetMarker => {
                cursor.enter("effect.target-card-zone-change.target-marker");
                cursor.consume("target")?;
                TargetCardZoneChangeState::TargetObject
            }
            TargetCardZoneChangeState::TargetObject => {
                cursor.enter("effect.target-card-zone-change.target-object");
                let object = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("card") || raw.eq_ignore_ascii_case("cards")
                    })
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| cursor.fail("`card` or `cards` as the target object"))?;
                cursor.consume(&object)?;
                TargetCardZoneChangeState::SourceRelation
            }
            TargetCardZoneChangeState::SourceRelation => {
                cursor.enter("effect.target-card-zone-change.source-relation");
                cursor.consume("from")?;
                TargetCardZoneChangeState::SourceArticle
            }
            TargetCardZoneChangeState::SourceArticle => {
                cursor.enter("effect.target-card-zone-change.source-article");
                match cursor.peek() {
                    Some(raw) if raw.eq_ignore_ascii_case("a") => {
                        cursor.consume("a")?;
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("single"))
                        {
                            cursor.consume("single")?;
                        }
                        frame.source_zone = Some(TargetCardSourceZone::SingleGraveyard);
                        frame.zone_spelling = Some(GraveyardZoneSpelling::Singular);
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("any") => {
                        cursor.consume("any")?;
                        frame.source_zone = Some(TargetCardSourceZone::AnyGraveyard);
                        frame.zone_spelling = Some(GraveyardZoneSpelling::Singular);
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("graveyards") => {
                        frame.source_zone = Some(TargetCardSourceZone::AnyGraveyard);
                        frame.zone_spelling = Some(GraveyardZoneSpelling::Plural);
                    }
                    _ => {
                        return Err(cursor
                            .fail("`a`, `a single`, `any`, or plural `graveyards` after `from`"));
                    }
                }
                TargetCardZoneChangeState::SourceZone
            }
            TargetCardZoneChangeState::SourceZone => {
                cursor.enter("effect.target-card-zone-change.source-zone");
                let zone = cursor
                    .peek_token()
                    .map(|token| (token.token_type.clone(), token.raw.clone()))
                    .ok_or_else(|| cursor.fail("the `graveyard` or `graveyards` source zone"))?;
                let expected_spelling =
                    cursor.require_value(frame.zone_spelling, "a source-zone number")?;
                cursor.require(
                    zone.0 == "zoneWord"
                        && match expected_spelling {
                            GraveyardZoneSpelling::Singular => {
                                zone.1.eq_ignore_ascii_case("graveyard")
                            }
                            GraveyardZoneSpelling::Plural => {
                                zone.1.eq_ignore_ascii_case("graveyards")
                            }
                        },
                    match expected_spelling {
                        GraveyardZoneSpelling::Singular => "the singular `graveyard` source zone",
                        GraveyardZoneSpelling::Plural => "the plural `graveyards` source zone",
                    },
                )?;
                cursor.consume(&zone.1)?;
                TargetCardZoneChangeState::Complete
            }
            TargetCardZoneChangeState::Complete => {
                cursor.enter("effect.target-card-zone-change.complete");
                return Ok(TargetCardZoneChangeEffectNode {
                    action: cursor.require_value(frame.action, "a card-zone action")?,
                    cardinality: cursor
                        .require_value(frame.cardinality, "a target-card cardinality")?,
                    source_zone: cursor.require_value(frame.source_zone, "a source card zone")?,
                    predicate: ObjectPredicate::ObjectKind(ObjectKind::Card),
                });
            }
        };
    }
}

fn recognize_return_target_card_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCardZoneChangeEffectNode> {
    let mut state = ReturnTargetCardState::Action;
    let mut predicate = None;
    loop {
        state = match state {
            ReturnTargetCardState::Action => {
                cursor.enter("effect.target-card-move.return.action");
                cursor.consume("return")?;
                ReturnTargetCardState::Target
            }
            ReturnTargetCardState::Target => {
                cursor.enter("effect.target-card-move.return.target");
                cursor.consume("target")?;
                ReturnTargetCardState::Selector
            }
            ReturnTargetCardState::Selector => {
                cursor.enter("effect.target-card-move.return.selector");
                predicate = Some(recognize_target_card_predicate(cursor)?);
                ReturnTargetCardState::SourceRelation
            }
            ReturnTargetCardState::SourceRelation => {
                cursor.enter("effect.target-card-move.return.source-relation");
                cursor.consume("from")?;
                ReturnTargetCardState::SourceOwner
            }
            ReturnTargetCardState::SourceOwner => {
                cursor.enter("effect.target-card-move.return.source-owner");
                cursor.consume("your")?;
                ReturnTargetCardState::SourceZone
            }
            ReturnTargetCardState::SourceZone => {
                cursor.enter("effect.target-card-move.return.source-zone");
                cursor.consume("graveyard")?;
                ReturnTargetCardState::DestinationRelation
            }
            ReturnTargetCardState::DestinationRelation => {
                cursor.enter("effect.target-card-move.return.destination-relation");
                cursor.consume("to")?;
                ReturnTargetCardState::DestinationOwner
            }
            ReturnTargetCardState::DestinationOwner => {
                cursor.enter("effect.target-card-move.return.destination-owner");
                cursor.consume("your")?;
                ReturnTargetCardState::DestinationZone
            }
            ReturnTargetCardState::DestinationZone => {
                cursor.enter("effect.target-card-move.return.destination-zone");
                cursor.consume("hand")?;
                ReturnTargetCardState::Complete
            }
            ReturnTargetCardState::Complete => {
                cursor.enter("effect.target-card-move.return.complete");
                return Ok(TargetCardZoneChangeEffectNode {
                    action: CardZoneChangeAction::Move(CardZoneDestination::ControllerHand),
                    cardinality: ExpressedTargetCardinality::EXACTLY_ONE,
                    source_zone: TargetCardSourceZone::ControllerGraveyard,
                    predicate: cursor.require_value(predicate, "a populated card selector")?,
                });
            }
        };
    }
}

fn recognize_put_target_card_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCardZoneChangeEffectNode> {
    let mut state = PutTargetCardState::Action;
    let mut predicate = None;
    loop {
        state = match state {
            PutTargetCardState::Action => {
                cursor.enter("effect.target-card-move.put.action");
                cursor.consume("put")?;
                PutTargetCardState::Target
            }
            PutTargetCardState::Target => {
                cursor.enter("effect.target-card-move.put.target");
                cursor.consume("target")?;
                PutTargetCardState::Selector
            }
            PutTargetCardState::Selector => {
                cursor.enter("effect.target-card-move.put.selector");
                predicate = Some(recognize_target_card_predicate(cursor)?);
                PutTargetCardState::SourceRelation
            }
            PutTargetCardState::SourceRelation => {
                cursor.enter("effect.target-card-move.put.source-relation");
                cursor.consume("from")?;
                PutTargetCardState::SourceArticle
            }
            PutTargetCardState::SourceArticle => {
                cursor.enter("effect.target-card-move.put.source-article");
                cursor.consume("a")?;
                PutTargetCardState::SourceZone
            }
            PutTargetCardState::SourceZone => {
                cursor.enter("effect.target-card-move.put.source-zone");
                cursor.consume("graveyard")?;
                PutTargetCardState::DestinationRelation
            }
            PutTargetCardState::DestinationRelation => {
                cursor.enter("effect.target-card-move.put.destination-relation");
                cursor.consume("on")?;
                PutTargetCardState::DestinationPosition
            }
            PutTargetCardState::DestinationPosition => {
                cursor.enter("effect.target-card-move.put.destination-position");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
                {
                    cursor.consume("the")?;
                }
                cursor.consume("top")?;
                PutTargetCardState::DestinationOf
            }
            PutTargetCardState::DestinationOf => {
                cursor.enter("effect.target-card-move.put.destination-of");
                cursor.consume("of")?;
                PutTargetCardState::DestinationObject
            }
            PutTargetCardState::DestinationObject => {
                cursor.enter("effect.target-card-move.put.destination-object");
                cursor.consume("its")?;
                PutTargetCardState::DestinationOwner
            }
            PutTargetCardState::DestinationOwner => {
                cursor.enter("effect.target-card-move.put.destination-owner");
                let owner = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("owner's") || raw.eq_ignore_ascii_case("owner’s")
                    })
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| cursor.fail("`owner's`"))?;
                cursor.consume(&owner)?;
                PutTargetCardState::DestinationZone
            }
            PutTargetCardState::DestinationZone => {
                cursor.enter("effect.target-card-move.put.destination-zone");
                cursor.consume("library")?;
                PutTargetCardState::Complete
            }
            PutTargetCardState::Complete => {
                cursor.enter("effect.target-card-move.put.complete");
                return Ok(TargetCardZoneChangeEffectNode {
                    action: CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop),
                    cardinality: ExpressedTargetCardinality::EXACTLY_ONE,
                    source_zone: TargetCardSourceZone::AnyGraveyard,
                    predicate: cursor.require_value(predicate, "a populated card selector")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "target_card_zone_change_tests.rs"]
mod tests;
