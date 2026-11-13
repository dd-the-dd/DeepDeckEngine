use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_basic_land_subtype, lookup_permanent_card_type,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, AddManaEffectNode, LocalPlayerReference, SelectorPredicate,
    recognize_activation_cost_sequence, recognize_add_mana_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LocalActivationConditionNode {
    ControlsPermanent {
        player: LocalPlayerReference,
        predicate: SelectorPredicate,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionalActivatedManaAbilityNode {
    pub costs: ActivationCostSequenceNode,
    pub effect: AddManaEffectNode,
    pub activation_condition: LocalActivationConditionNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndefinitePermanentQueryState {
    BranchDeterminer,
    BranchPredicate,
    ConnectorOrComplete,
    Complete,
}

/// Recognizes an indefinite permanent-existence query such as
/// `an Island or a Swamp`. Articles belong to this query parent rather than
/// to the shared object predicate because they delimit every union branch.
fn recognize_indefinite_permanent_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    let mut state = IndefinitePermanentQueryState::BranchDeterminer;
    let mut branches = Vec::new();
    loop {
        state = match state {
            IndefinitePermanentQueryState::BranchDeterminer => {
                cursor.enter("condition.controls-permanent.branch-determiner");
                let determiner = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("an indefinite object determiner"))?;
                if determiner.token_type != "quantifierWord"
                    || (!determiner.raw.eq_ignore_ascii_case("a")
                        && !determiner.raw.eq_ignore_ascii_case("an"))
                {
                    return Err(cursor.fail_with_span(
                        "an indefinite object determiner",
                        determiner.index,
                        determiner.span_start,
                        determiner.span_end,
                        determiner.raw,
                    ));
                }
                cursor.advance_to(cursor.position() + 1)?;
                IndefinitePermanentQueryState::BranchPredicate
            }
            IndefinitePermanentQueryState::BranchPredicate => {
                cursor.enter("condition.controls-permanent.branch-predicate");
                let token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("a permanent type or subtype"))?;
                let predicate = if let Some(card_type) =
                    lookup_permanent_card_type(&token.raw, AcceptedForms::SingularOnly)
                {
                    SelectorPredicate::CardType(card_type)
                } else if let Some(subtype) =
                    lookup_basic_land_subtype(&token.raw, AcceptedForms::SingularOnly)
                {
                    SelectorPredicate::Subtype(subtype.ir_name().to_string())
                } else {
                    return Err(cursor.fail_with_span(
                        "a permanent type or subtype",
                        token.index,
                        token.span_start,
                        token.span_end,
                        token.raw,
                    ));
                };
                cursor.advance_to(cursor.position() + 1)?;
                branches.push(predicate);
                IndefinitePermanentQueryState::ConnectorOrComplete
            }
            IndefinitePermanentQueryState::ConnectorOrComplete => {
                cursor.enter("condition.controls-permanent.connector-or-complete");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    IndefinitePermanentQueryState::BranchDeterminer
                } else {
                    IndefinitePermanentQueryState::Complete
                }
            }
            IndefinitePermanentQueryState::Complete => {
                cursor.enter("condition.controls-permanent.complete");
                return match branches.len() {
                    0 => Err(cursor.fail("one or more permanent-existence branches")),
                    1 => Ok(branches.pop().expect("one predicate branch")),
                    _ => Ok(SelectorPredicate::Union(branches)),
                };
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlsPermanentConditionState {
    Player,
    Relation,
    Predicate,
    Complete,
}

/// Recognizes a controller-relative permanent-existence condition. The
/// predicate is retained as the same typed object predicate used by targets
/// and set queries, so lowering and runtime matching stay shared.
fn recognize_controls_permanent_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalActivationConditionNode> {
    let mut state = ControlsPermanentConditionState::Player;
    let mut predicate = None;
    loop {
        state = match state {
            ControlsPermanentConditionState::Player => {
                cursor.enter("condition.controls-permanent.player");
                cursor.consume("you")?;
                ControlsPermanentConditionState::Relation
            }
            ControlsPermanentConditionState::Relation => {
                cursor.enter("condition.controls-permanent.relation");
                let relation = cursor.consume_type("controlWord")?;
                cursor.require(
                    relation.raw.eq_ignore_ascii_case("control"),
                    "the controller relation `control`",
                )?;
                ControlsPermanentConditionState::Predicate
            }
            ControlsPermanentConditionState::Predicate => {
                cursor.enter("condition.controls-permanent.predicate");
                predicate = Some(recognize_indefinite_permanent_query(cursor)?);
                ControlsPermanentConditionState::Complete
            }
            ControlsPermanentConditionState::Complete => {
                cursor.enter("condition.controls-permanent.bound");
                return Ok(LocalActivationConditionNode::ControlsPermanent {
                    player: LocalPlayerReference::Controller,
                    predicate: cursor
                        .require_value(predicate, "a populated controlled-permanent predicate")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionalManaActivationState {
    Costs,
    CostSeparator,
    ManaEffect,
    EffectTerminator,
    ActivationKeyword,
    Restriction,
    Conditional,
    Condition,
    ConditionTerminator,
    Complete,
}

#[derive(Default)]
struct ConditionalManaActivationFrame {
    costs: Option<ActivationCostSequenceNode>,
    effect: Option<AddManaEffectNode>,
    activation_condition: Option<LocalActivationConditionNode>,
}

/// Recognizes one mana activation followed by a generic activation-condition
/// suffix. The condition is metadata on the activation, never a follow-up
/// effect, and therefore remains independent from the produced mana.
pub(in crate::oracle::canonical) fn recognize_conditional_activated_mana_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionalActivatedManaAbilityNode> {
    let mut state = ConditionalManaActivationState::Costs;
    let mut frame = ConditionalManaActivationFrame::default();
    loop {
        state = match state {
            ConditionalManaActivationState::Costs => {
                cursor.enter("activation.conditional-mana.costs");
                frame.costs = Some(recognize_activation_cost_sequence(cursor)?);
                ConditionalManaActivationState::CostSeparator
            }
            ConditionalManaActivationState::CostSeparator => {
                cursor.enter("activation.conditional-mana.cost-separator");
                cursor.consume(":")?;
                ConditionalManaActivationState::ManaEffect
            }
            ConditionalManaActivationState::ManaEffect => {
                cursor.enter("activation.conditional-mana.effect");
                frame.effect = Some(recognize_add_mana_effect(cursor)?);
                ConditionalManaActivationState::EffectTerminator
            }
            ConditionalManaActivationState::EffectTerminator => {
                cursor.enter("activation.conditional-mana.effect-terminator");
                cursor.consume(".")?;
                ConditionalManaActivationState::ActivationKeyword
            }
            ConditionalManaActivationState::ActivationKeyword => {
                cursor.enter("activation.condition.keyword");
                cursor.consume("activate")?;
                ConditionalManaActivationState::Restriction
            }
            ConditionalManaActivationState::Restriction => {
                cursor.enter("activation.condition.restriction");
                cursor.consume("only")?;
                ConditionalManaActivationState::Conditional
            }
            ConditionalManaActivationState::Conditional => {
                cursor.enter("activation.condition.if");
                cursor.consume("if")?;
                ConditionalManaActivationState::Condition
            }
            ConditionalManaActivationState::Condition => {
                cursor.enter("activation.condition.body");
                frame.activation_condition = Some(recognize_controls_permanent_condition(cursor)?);
                ConditionalManaActivationState::ConditionTerminator
            }
            ConditionalManaActivationState::ConditionTerminator => {
                cursor.enter("activation.condition.terminator");
                cursor.consume(".")?;
                ConditionalManaActivationState::Complete
            }
            ConditionalManaActivationState::Complete => {
                cursor.enter("activation.conditional-mana.complete");
                cursor.finish()?;
                return Ok(ConditionalActivatedManaAbilityNode {
                    costs: cursor.require_value(frame.costs, "populated activation costs")?,
                    effect: cursor.require_value(frame.effect, "a populated mana effect")?,
                    activation_condition: cursor.require_value(
                        frame.activation_condition,
                        "a populated activation condition",
                    )?,
                });
            }
        };
    }
}
