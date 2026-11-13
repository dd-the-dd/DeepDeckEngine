use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ChoiceBinding, ControlledCountDirection, ControlledPermanentCountConditionNode,
    LocalObjectReference, LocalPlayerReference, NumericAtom, ObjectKind, PlayerZoneKind,
    SelectorPredicate, recognize_card_type_union_selector, recognize_numeric_atom,
    recognize_object_query_before_separator, recognize_source_reference,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LocalConditionNode {
    WasKicked {
        spell: LocalObjectReference,
    },
    NotYourTurn,
    CostNotPaid {
        payment: ChoiceBinding,
    },
    PermanentLeftBattlefieldThisTurn {
        player: LocalPlayerReference,
    },
    CardsExistInZone {
        player: LocalPlayerReference,
        zone: PlayerZoneKind,
        predicates: Vec<SelectorPredicate>,
    },
    ControlledPermanentCount(ControlledPermanentCountConditionNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentCountState {
    Player,
    Control,
    Threshold,
    ComparatorConnector,
    ComparatorDirection,
    OptionalOther,
    PermanentPredicate,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentPresenceState {
    If,
    Player,
    Control,
    Determiner,
    PermanentPredicate,
    Separator,
    Complete,
}

/// Recognizes `if you control a/an <permanent predicate>,` as the same
/// controlled-permanent count primitive used by numeric conditions. Control
/// establishes the battlefield-permanent domain; the child selector only
/// owns reusable characteristics such as `commander` or `legendary creature`.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_presence_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalConditionNode> {
    let mut state = ControlledPermanentPresenceState::If;
    let mut predicate = None;
    loop {
        state = match state {
            ControlledPermanentPresenceState::If => {
                cursor.enter("condition.controlled-permanent-presence.if");
                cursor.consume("if")?;
                ControlledPermanentPresenceState::Player
            }
            ControlledPermanentPresenceState::Player => {
                cursor.enter("condition.controlled-permanent-presence.player");
                cursor.consume("you")?;
                ControlledPermanentPresenceState::Control
            }
            ControlledPermanentPresenceState::Control => {
                cursor.enter("condition.controlled-permanent-presence.control");
                cursor.consume("control")?;
                ControlledPermanentPresenceState::Determiner
            }
            ControlledPermanentPresenceState::Determiner => {
                cursor.enter("condition.controlled-permanent-presence.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    determiner.raw.eq_ignore_ascii_case("a")
                        || determiner.raw.eq_ignore_ascii_case("an"),
                    "`a` or `an` before a controlled permanent predicate",
                )?;
                ControlledPermanentPresenceState::PermanentPredicate
            }
            ControlledPermanentPresenceState::PermanentPredicate => {
                cursor.enter("condition.controlled-permanent-presence.permanent-predicate");
                let query = recognize_object_query_before_separator(cursor)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped permanent selector after `you control`",
                )?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent),
                    "a predicate valid for a battlefield permanent",
                )?;
                predicate = Some(query.predicate);
                ControlledPermanentPresenceState::Separator
            }
            ControlledPermanentPresenceState::Separator => {
                cursor.enter("condition.controlled-permanent-presence.separator");
                cursor.consume(",")?;
                ControlledPermanentPresenceState::Complete
            }
            ControlledPermanentPresenceState::Complete => {
                cursor.enter("condition.controlled-permanent-presence.complete");
                return Ok(LocalConditionNode::ControlledPermanentCount(
                    ControlledPermanentCountConditionNode {
                        player: LocalPlayerReference::Controller,
                        threshold: 1,
                        direction: ControlledCountDirection::OrMore,
                        predicate: cursor
                            .require_value(predicate, "the controlled permanent predicate")?,
                        exclude_source: false,
                    },
                ));
            }
        };
    }
}

/// Recognizes the reusable count query `you control N or more/fewer [other]
/// <permanents>`. The surrounding condition owns `if`/`unless` and punctuation.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_count_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledPermanentCountConditionNode> {
    let mut state = ControlledPermanentCountState::Player;
    let mut threshold = None;
    let mut direction = None;
    let mut predicate = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            ControlledPermanentCountState::Player => {
                cursor.enter("condition.controlled-permanent-count.player");
                cursor.consume("you")?;
                ControlledPermanentCountState::Control
            }
            ControlledPermanentCountState::Control => {
                cursor.enter("condition.controlled-permanent-count.control");
                cursor.consume("control")?;
                ControlledPermanentCountState::Threshold
            }
            ControlledPermanentCountState::Threshold => {
                cursor.enter("condition.controlled-permanent-count.threshold");
                let value = match recognize_numeric_atom(cursor)? {
                    NumericAtom::Integer(value) => value,
                    NumericAtom::VariableX => {
                        return Err(cursor.fail("a fixed nonnegative permanent-count threshold"));
                    }
                };
                cursor.require(value >= 0, "a fixed nonnegative permanent-count threshold")?;
                threshold = Some(value);
                ControlledPermanentCountState::ComparatorConnector
            }
            ControlledPermanentCountState::ComparatorConnector => {
                cursor.enter("condition.controlled-permanent-count.comparator-connector");
                cursor.consume("or")?;
                ControlledPermanentCountState::ComparatorDirection
            }
            ControlledPermanentCountState::ComparatorDirection => {
                cursor.enter("condition.controlled-permanent-count.comparator-direction");
                direction = Some(match cursor.peek() {
                    Some(raw) if raw.eq_ignore_ascii_case("more") => {
                        cursor.consume("more")?;
                        ControlledCountDirection::OrMore
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("fewer") => {
                        cursor.consume("fewer")?;
                        ControlledCountDirection::OrFewer
                    }
                    _ => return Err(cursor.fail("`more` or `fewer`")),
                });
                ControlledPermanentCountState::OptionalOther
            }
            ControlledPermanentCountState::OptionalOther => {
                cursor.enter("condition.controlled-permanent-count.optional-other");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
                {
                    cursor.consume("other")?;
                    exclude_source = true;
                }
                ControlledPermanentCountState::PermanentPredicate
            }
            ControlledPermanentCountState::PermanentPredicate => {
                cursor.enter("condition.controlled-permanent-count.permanent-predicate");
                let query = recognize_object_query_before_separator(cursor)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped permanent selector after `you control`",
                )?;
                cursor.require(
                    query.predicate.has_domain(ObjectKind::Permanent),
                    "a selector that establishes the permanent domain",
                )?;
                predicate = Some(query.predicate);
                ControlledPermanentCountState::Complete
            }
            ControlledPermanentCountState::Complete => {
                cursor.enter("condition.controlled-permanent-count.complete");
                return Ok(ControlledPermanentCountConditionNode {
                    player: LocalPlayerReference::Controller,
                    threshold: cursor.require_value(threshold, "the count threshold")?,
                    direction: cursor.require_value(direction, "the comparison direction")?,
                    predicate: cursor
                        .require_value(predicate, "the counted permanent predicate")?,
                    exclude_source,
                });
            }
        };
    }
}

/// Recognizes `if you control N or more/fewer <permanents>,` and retains the
/// numeric comparison and selector as independent typed concepts.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_count_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalConditionNode> {
    cursor.enter("condition.controlled-permanent-count.if");
    cursor.consume("if")?;
    let condition = recognize_controlled_permanent_count_query(cursor)?;
    cursor.enter("condition.controlled-permanent-count.separator");
    cursor.consume(",")?;
    Ok(LocalConditionNode::ControlledPermanentCount(condition))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardsExistInZoneConditionState {
    If,
    Existential,
    Copula,
    Determiner,
    Predicate,
    ConnectorOrZone,
    ZoneRelation,
    ZonePossessor,
    Zone,
    Separator,
    Complete,
}

/// Recognizes an existential conjunction over typed card predicates in one
/// player zone, for example `if there is an instant card and a sorcery card
/// in your graveyard,`. Each object phrase is delegated to the shared card
/// selector, while this parent owns Boolean `and` and the zone scope.
pub(in crate::oracle::canonical) fn recognize_cards_exist_in_controller_graveyard_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalConditionNode> {
    let mut state = CardsExistInZoneConditionState::If;
    let mut predicates = Vec::new();
    loop {
        state = match state {
            CardsExistInZoneConditionState::If => {
                cursor.enter("condition.zone-card-presence.if");
                cursor.consume("if")?;
                CardsExistInZoneConditionState::Existential
            }
            CardsExistInZoneConditionState::Existential => {
                cursor.enter("condition.zone-card-presence.existential");
                cursor.consume("there")?;
                CardsExistInZoneConditionState::Copula
            }
            CardsExistInZoneConditionState::Copula => {
                cursor.enter("condition.zone-card-presence.copula");
                cursor.consume("is")?;
                CardsExistInZoneConditionState::Determiner
            }
            CardsExistInZoneConditionState::Determiner => {
                cursor.enter("condition.zone-card-presence.determiner");
                let determiner = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`a` or `an` before a card predicate"))?
                    .to_string();
                cursor.require(
                    determiner.eq_ignore_ascii_case("a") || determiner.eq_ignore_ascii_case("an"),
                    "`a` or `an` before a card predicate",
                )?;
                cursor.consume(&determiner)?;
                CardsExistInZoneConditionState::Predicate
            }
            CardsExistInZoneConditionState::Predicate => {
                cursor.enter("condition.zone-card-presence.predicate");
                let predicate = recognize_card_type_union_selector(cursor)?;
                cursor.require(
                    predicate.supports_domain(ObjectKind::Card)
                        && predicate.has_domain(ObjectKind::Card),
                    "a selector in the card domain",
                )?;
                predicates.push(predicate);
                CardsExistInZoneConditionState::ConnectorOrZone
            }
            CardsExistInZoneConditionState::ConnectorOrZone => {
                cursor.enter("condition.zone-card-presence.connector-or-zone");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    CardsExistInZoneConditionState::Determiner
                } else {
                    CardsExistInZoneConditionState::ZoneRelation
                }
            }
            CardsExistInZoneConditionState::ZoneRelation => {
                cursor.enter("condition.zone-card-presence.zone-relation");
                cursor.consume("in")?;
                CardsExistInZoneConditionState::ZonePossessor
            }
            CardsExistInZoneConditionState::ZonePossessor => {
                cursor.enter("condition.zone-card-presence.zone-possessor");
                cursor.consume("your")?;
                CardsExistInZoneConditionState::Zone
            }
            CardsExistInZoneConditionState::Zone => {
                cursor.enter("condition.zone-card-presence.zone");
                cursor.consume("graveyard")?;
                CardsExistInZoneConditionState::Separator
            }
            CardsExistInZoneConditionState::Separator => {
                cursor.enter("condition.zone-card-presence.separator");
                cursor.consume(",")?;
                CardsExistInZoneConditionState::Complete
            }
            CardsExistInZoneConditionState::Complete => {
                cursor.enter("condition.zone-card-presence.complete");
                cursor.require(!predicates.is_empty(), "one or more card predicates")?;
                return Ok(LocalConditionNode::CardsExistInZone {
                    player: LocalPlayerReference::Controller,
                    zone: PlayerZoneKind::Graveyard,
                    predicates,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeftBattlefieldConditionState {
    If,
    Quantifier,
    Object,
    Event,
    ZoneDeterminer,
    Zone,
    Relation,
    Player,
    Control,
    DurationDeterminer,
    Duration,
    Complete,
}

/// Recognizes the event-history condition shared by revolt-style text without
/// assigning any meaning to the decorative ability-word label.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_left_battlefield_this_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalConditionNode> {
    let mut state = LeftBattlefieldConditionState::If;
    loop {
        state = match state {
            LeftBattlefieldConditionState::If => {
                cursor.enter("condition.permanent-left-battlefield.if");
                cursor.consume("if")?;
                LeftBattlefieldConditionState::Quantifier
            }
            LeftBattlefieldConditionState::Quantifier => {
                cursor.enter("condition.permanent-left-battlefield.quantifier");
                cursor.consume("a")?;
                LeftBattlefieldConditionState::Object
            }
            LeftBattlefieldConditionState::Object => {
                cursor.enter("condition.permanent-left-battlefield.object");
                cursor.consume("permanent")?;
                LeftBattlefieldConditionState::Event
            }
            LeftBattlefieldConditionState::Event => {
                cursor.enter("condition.permanent-left-battlefield.event");
                cursor.consume("left")?;
                LeftBattlefieldConditionState::ZoneDeterminer
            }
            LeftBattlefieldConditionState::ZoneDeterminer => {
                cursor.enter("condition.permanent-left-battlefield.zone-determiner");
                cursor.consume("the")?;
                LeftBattlefieldConditionState::Zone
            }
            LeftBattlefieldConditionState::Zone => {
                cursor.enter("condition.permanent-left-battlefield.zone");
                cursor.consume("battlefield")?;
                LeftBattlefieldConditionState::Relation
            }
            LeftBattlefieldConditionState::Relation => {
                cursor.enter("condition.permanent-left-battlefield.relation");
                cursor.consume("under")?;
                LeftBattlefieldConditionState::Player
            }
            LeftBattlefieldConditionState::Player => {
                cursor.enter("condition.permanent-left-battlefield.player");
                cursor.consume("your")?;
                LeftBattlefieldConditionState::Control
            }
            LeftBattlefieldConditionState::Control => {
                cursor.enter("condition.permanent-left-battlefield.control");
                cursor.consume("control")?;
                LeftBattlefieldConditionState::DurationDeterminer
            }
            LeftBattlefieldConditionState::DurationDeterminer => {
                cursor.enter("condition.permanent-left-battlefield.duration-determiner");
                cursor.consume("this")?;
                LeftBattlefieldConditionState::Duration
            }
            LeftBattlefieldConditionState::Duration => {
                cursor.enter("condition.permanent-left-battlefield.duration");
                cursor.consume("turn")?;
                LeftBattlefieldConditionState::Complete
            }
            LeftBattlefieldConditionState::Complete => {
                cursor.enter("condition.permanent-left-battlefield.complete");
                return Ok(LocalConditionNode::PermanentLeftBattlefieldThisTurn {
                    player: LocalPlayerReference::Controller,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CostNotPaidConditionState {
    If,
    Player,
    PaymentOmitted,
    Separator,
    Complete,
}

/// Recognizes `if you don't,` while resolving the omitted payment through the
/// typed binding supplied by its parent sequence.
pub(in crate::oracle::canonical) fn recognize_cost_not_paid_condition(
    cursor: &mut OracleTokenCursor<'_>,
    payment: ChoiceBinding,
) -> MachineResult<LocalConditionNode> {
    let mut state = CostNotPaidConditionState::If;
    loop {
        state = match state {
            CostNotPaidConditionState::If => {
                cursor.enter("condition.cost-not-paid.if");
                cursor.consume("if")?;
                CostNotPaidConditionState::Player
            }
            CostNotPaidConditionState::Player => {
                cursor.enter("condition.cost-not-paid.player");
                cursor.consume("you")?;
                CostNotPaidConditionState::PaymentOmitted
            }
            CostNotPaidConditionState::PaymentOmitted => {
                cursor.enter("condition.cost-not-paid.payment-omitted");
                cursor.consume("don't")?;
                CostNotPaidConditionState::Separator
            }
            CostNotPaidConditionState::Separator => {
                cursor.enter("condition.cost-not-paid.separator");
                cursor.consume(",")?;
                CostNotPaidConditionState::Complete
            }
            CostNotPaidConditionState::Complete => {
                cursor.enter("condition.cost-not-paid.complete");
                return Ok(LocalConditionNode::CostNotPaid { payment });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotYourTurnConditionState {
    If,
    Subject,
    Negation,
    Possessor,
    Turn,
    Separator,
    Complete,
}

/// Recognizes the reusable turn condition `if it's not your turn,`.
pub(in crate::oracle::canonical) fn recognize_not_your_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalConditionNode> {
    let mut state = NotYourTurnConditionState::If;
    loop {
        state = match state {
            NotYourTurnConditionState::If => {
                cursor.enter("condition.not-your-turn.if");
                cursor.consume("if")?;
                NotYourTurnConditionState::Subject
            }
            NotYourTurnConditionState::Subject => {
                cursor.enter("condition.not-your-turn.subject");
                let subject = cursor.consume_type("referenceWord")?;
                cursor.require(
                    subject.raw.eq_ignore_ascii_case("it's")
                        || subject.raw.eq_ignore_ascii_case("it’s"),
                    "the current-turn reference `it's`",
                )?;
                NotYourTurnConditionState::Negation
            }
            NotYourTurnConditionState::Negation => {
                cursor.enter("condition.not-your-turn.negation");
                cursor.consume("not")?;
                NotYourTurnConditionState::Possessor
            }
            NotYourTurnConditionState::Possessor => {
                cursor.enter("condition.not-your-turn.possessor");
                cursor.consume("your")?;
                NotYourTurnConditionState::Turn
            }
            NotYourTurnConditionState::Turn => {
                cursor.enter("condition.not-your-turn.turn");
                cursor.consume("turn")?;
                NotYourTurnConditionState::Separator
            }
            NotYourTurnConditionState::Separator => {
                cursor.enter("condition.not-your-turn.separator");
                cursor.consume(",")?;
                NotYourTurnConditionState::Complete
            }
            NotYourTurnConditionState::Complete => {
                cursor.enter("condition.not-your-turn.complete");
                return Ok(LocalConditionNode::NotYourTurn);
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KickedConditionState {
    If,
    Spell,
    Predicate,
    Separator,
    Complete,
}

/// Recognizes the reusable condition envelope `if this spell was kicked,`.
///
/// The spell reference is delegated to the shared reference automaton, so the
/// condition retains the same binding semantics as an effect source.
pub(in crate::oracle::canonical) fn recognize_kicked_condition(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalConditionNode> {
    let mut state = KickedConditionState::If;
    let mut spell = None;
    loop {
        state = match state {
            KickedConditionState::If => {
                cursor.enter("condition.kicked.if");
                cursor.consume("if")?;
                KickedConditionState::Spell
            }
            KickedConditionState::Spell => {
                cursor.enter("condition.kicked.spell");
                spell = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                KickedConditionState::Predicate
            }
            KickedConditionState::Predicate => {
                cursor.enter("condition.kicked.predicate");
                cursor.consume_phrase(&["was", "kicked"])?;
                KickedConditionState::Separator
            }
            KickedConditionState::Separator => {
                cursor.enter("condition.kicked.separator");
                cursor.consume(",")?;
                KickedConditionState::Complete
            }
            KickedConditionState::Complete => {
                cursor.enter("condition.kicked.complete");
                return Ok(LocalConditionNode::WasKicked {
                    spell: cursor.require_value(spell, "a spell bound to the kicked condition")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "condition_controlled_presence_tests.rs"]
mod controlled_presence_tests;
