use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_type, lookup_creature_subtype, lookup_permanent_card_type,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, CreatureSubtype, ObjectCardType, ObjectPredicate,
    PowerToughnessNode, recognize_activation_cost_sequence, recognize_literal_power_toughness,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LinkedExileSetNode {
    pub predicate: ObjectPredicate,
    pub source_type: ObjectCardType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EnteringCounterNode {
    pub kind: String,
    pub count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LinkedExileBattlefieldMoveNode {
    pub cards: LinkedExileSetNode,
    pub entering_counter: EnteringCounterNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LinkedSetBinding {
    MovedCards,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AddedCreatureCharacteristicsNode {
    pub objects: LinkedSetBinding,
    pub power_toughness: PowerToughnessNode,
    pub subtype: CreatureSubtype,
    pub retain_existing_types: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationTimingNode {
    Sorcery,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LinkedExileReturnActivationNode {
    pub costs: ActivationCostSequenceNode,
    pub movement: LinkedExileBattlefieldMoveNode,
    pub characteristics: AddedCreatureCharacteristicsNode,
    pub timing: ActivationTimingNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkedSetState {
    Quantifier,
    Predicate,
    Domain,
    ExileRelation,
    SourceReference,
    SourceType,
    Complete,
}

/// Recognizes the reusable set `each TYPE card exiled with this SOURCE-TYPE`.
/// The linked-set origin remains independent from the later zone movement.
fn recognize_linked_exile_set(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LinkedExileSetNode> {
    let mut state = LinkedSetState::Quantifier;
    let mut predicate = None;
    let mut source_type = None;
    loop {
        state = match state {
            LinkedSetState::Quantifier => {
                cursor.enter("set.linked-exile.quantifier");
                cursor.consume("each")?;
                LinkedSetState::Predicate
            }
            LinkedSetState::Predicate => {
                cursor.enter("set.linked-exile.predicate");
                let card_type = cursor
                    .peek()
                    .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
                    .ok_or_else(|| cursor.fail("a registered singular card type"))?;
                cursor.advance_to(cursor.position() + 1)?;
                predicate = Some(ObjectPredicate::CardType(card_type));
                LinkedSetState::Domain
            }
            LinkedSetState::Domain => {
                cursor.enter("set.linked-exile.domain");
                cursor.consume("card")?;
                LinkedSetState::ExileRelation
            }
            LinkedSetState::ExileRelation => {
                cursor.enter("set.linked-exile.relation");
                cursor.consume_phrase(&["exiled", "with"])?;
                LinkedSetState::SourceReference
            }
            LinkedSetState::SourceReference => {
                cursor.enter("set.linked-exile.source-reference");
                cursor.consume("this")?;
                LinkedSetState::SourceType
            }
            LinkedSetState::SourceType => {
                cursor.enter("set.linked-exile.source-type");
                let parsed = cursor
                    .peek()
                    .and_then(|raw| lookup_permanent_card_type(raw, AcceptedForms::SingularOnly))
                    .ok_or_else(|| cursor.fail("a registered permanent source type"))?;
                cursor.advance_to(cursor.position() + 1)?;
                source_type = Some(parsed);
                LinkedSetState::Complete
            }
            LinkedSetState::Complete => {
                cursor.enter("set.linked-exile.complete");
                return Ok(LinkedExileSetNode {
                    predicate: cursor.require_value(predicate, "a linked-card predicate")?,
                    source_type: cursor.require_value(source_type, "a linked-exile source type")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveState {
    Action,
    Cards,
    Destination,
    Controller,
    CounterRelation,
    CounterArticle,
    CounterKind,
    CounterNoun,
    CounterObject,
    Complete,
}

fn recognize_entering_counter_kind(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<String> {
    cursor.enter("zone-change.linked-return.entering-counter-kind");
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("a named counter kind"))?;
    cursor.require(
        cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("counter"))
            && token.token_type != "separator",
        "one lexical counter kind followed by `counter`",
    )?;
    let kind = token.raw.to_ascii_lowercase();
    cursor.advance_to(cursor.position() + 1)?;
    Ok(kind)
}

/// Recognizes movement of a linked set to the controller's battlefield with
/// one entering counter. Characteristics are deliberately owned by a sibling
/// automaton because they describe the moved objects rather than the move.
fn recognize_linked_exile_battlefield_move(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LinkedExileBattlefieldMoveNode> {
    let mut state = MoveState::Action;
    let mut cards = None;
    let mut counter_kind = None;
    loop {
        state = match state {
            MoveState::Action => {
                cursor.enter("zone-change.linked-return.action");
                cursor.consume("put")?;
                MoveState::Cards
            }
            MoveState::Cards => {
                cards = Some(recognize_linked_exile_set(cursor)?);
                MoveState::Destination
            }
            MoveState::Destination => {
                cursor.enter("zone-change.linked-return.destination");
                cursor.consume_phrase(&["onto", "the", "battlefield"])?;
                MoveState::Controller
            }
            MoveState::Controller => {
                cursor.enter("zone-change.linked-return.controller");
                cursor.consume_phrase(&["under", "your", "control"])?;
                MoveState::CounterRelation
            }
            MoveState::CounterRelation => {
                cursor.enter("zone-change.linked-return.entering-counter-relation");
                cursor.consume("with")?;
                MoveState::CounterArticle
            }
            MoveState::CounterArticle => {
                cursor.enter("zone-change.linked-return.entering-counter-article");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("a"))
                {
                    cursor.consume("a")?;
                } else {
                    cursor.consume("an")?;
                }
                MoveState::CounterKind
            }
            MoveState::CounterKind => {
                counter_kind = Some(recognize_entering_counter_kind(cursor)?);
                MoveState::CounterNoun
            }
            MoveState::CounterNoun => {
                cursor.enter("zone-change.linked-return.entering-counter-noun");
                cursor.consume("counter")?;
                MoveState::CounterObject
            }
            MoveState::CounterObject => {
                cursor.enter("zone-change.linked-return.entering-counter-object");
                cursor.consume_phrase(&["on", "it"])?;
                MoveState::Complete
            }
            MoveState::Complete => {
                cursor.enter("zone-change.linked-return.complete");
                return Ok(LinkedExileBattlefieldMoveNode {
                    cards: cursor.require_value(cards, "a linked exiled-card set")?,
                    entering_counter: EnteringCounterNode {
                        kind: cursor.require_value(counter_kind, "an entering counter kind")?,
                        count: 1,
                    },
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharacteristicsState {
    Anaphor,
    Predicate,
    Article,
    Statistics,
    Subtype,
    Retention,
    Complete,
}

/// Recognizes a characteristic amendment over the set moved by the preceding
/// sentence. The explicit set anaphor prevents accidental acceptance as an
/// unrelated global continuous effect.
fn recognize_added_creature_characteristics(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddedCreatureCharacteristicsNode> {
    let mut state = CharacteristicsState::Anaphor;
    let mut statistics = None;
    let mut subtype = None;
    loop {
        state = match state {
            CharacteristicsState::Anaphor => {
                cursor.enter("characteristic.linked-set.anaphor");
                cursor.consume_phrase(&["each", "of", "them"])?;
                CharacteristicsState::Predicate
            }
            CharacteristicsState::Predicate => {
                cursor.enter("characteristic.linked-set.predicate");
                cursor.consume("is")?;
                CharacteristicsState::Article
            }
            CharacteristicsState::Article => {
                cursor.enter("characteristic.linked-set.article");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("a"))
                {
                    cursor.consume("a")?;
                } else {
                    cursor.consume("an")?;
                }
                CharacteristicsState::Statistics
            }
            CharacteristicsState::Statistics => {
                statistics = Some(recognize_literal_power_toughness(cursor)?);
                CharacteristicsState::Subtype
            }
            CharacteristicsState::Subtype => {
                cursor.enter("characteristic.linked-set.creature-subtype");
                let parsed = cursor
                    .peek()
                    .and_then(|raw| lookup_creature_subtype(raw, AcceptedForms::SingularOnly))
                    .ok_or_else(|| cursor.fail("a registered singular creature subtype"))?;
                cursor.advance_to(cursor.position() + 1)?;
                subtype = Some(parsed);
                CharacteristicsState::Retention
            }
            CharacteristicsState::Retention => {
                cursor.enter("characteristic.linked-set.retention");
                cursor.consume_phrase(&["in", "addition", "to", "its", "other", "types"])?;
                CharacteristicsState::Complete
            }
            CharacteristicsState::Complete => {
                cursor.enter("characteristic.linked-set.complete");
                return Ok(AddedCreatureCharacteristicsNode {
                    objects: LinkedSetBinding::MovedCards,
                    power_toughness: cursor
                        .require_value(statistics, "literal base power/toughness")?,
                    subtype: cursor.require_value(subtype, "a creature subtype")?,
                    retain_existing_types: true,
                });
            }
        };
    }
}

fn recognize_activation_timing(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationTimingNode> {
    cursor.enter("activation.timing.action");
    cursor.consume("activate")?;
    cursor.enter("activation.timing.restriction");
    cursor.consume_phrase(&["only", "as", "a"])?;
    cursor.enter("activation.timing.kind");
    cursor.consume("sorcery")?;
    Ok(ActivationTimingNode::Sorcery)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbilityState {
    Costs,
    EffectSeparator,
    Movement,
    MovementTerminator,
    Characteristics,
    CharacteristicsTerminator,
    Timing,
    TimingTerminator,
    Complete,
}

/// Composes costs, linked-set movement, entering modifications, the set
/// anaphor's characteristics, and activation timing without matching source
/// text or creating a card-shaped operation.
pub(in crate::oracle::canonical) fn recognize_linked_exile_return_activation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LinkedExileReturnActivationNode> {
    let mut state = AbilityState::Costs;
    let mut costs = None;
    let mut movement = None;
    let mut characteristics = None;
    let mut timing = None;
    loop {
        state = match state {
            AbilityState::Costs => {
                costs = Some(recognize_activation_cost_sequence(cursor)?);
                AbilityState::EffectSeparator
            }
            AbilityState::EffectSeparator => {
                cursor.enter("activation.linked-return.effect-separator");
                cursor.consume(":")?;
                AbilityState::Movement
            }
            AbilityState::Movement => {
                movement = Some(recognize_linked_exile_battlefield_move(cursor)?);
                AbilityState::MovementTerminator
            }
            AbilityState::MovementTerminator => {
                cursor.enter("activation.linked-return.movement-terminator");
                cursor.consume(".")?;
                AbilityState::Characteristics
            }
            AbilityState::Characteristics => {
                characteristics = Some(recognize_added_creature_characteristics(cursor)?);
                AbilityState::CharacteristicsTerminator
            }
            AbilityState::CharacteristicsTerminator => {
                cursor.enter("activation.linked-return.characteristics-terminator");
                cursor.consume(".")?;
                AbilityState::Timing
            }
            AbilityState::Timing => {
                timing = Some(recognize_activation_timing(cursor)?);
                AbilityState::TimingTerminator
            }
            AbilityState::TimingTerminator => {
                cursor.enter("activation.linked-return.timing-terminator");
                cursor.consume(".")?;
                AbilityState::Complete
            }
            AbilityState::Complete => {
                cursor.enter("activation.linked-return.complete");
                cursor.finish()?;
                return Ok(LinkedExileReturnActivationNode {
                    costs: cursor.require_value(costs, "an activation cost sequence")?,
                    movement: cursor.require_value(movement, "one linked-set movement")?,
                    characteristics: cursor
                        .require_value(characteristics, "one moved-set characteristic amendment")?,
                    timing: cursor.require_value(timing, "an activation timing restriction")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "linked_exile_return_tests.rs"]
mod tests;
