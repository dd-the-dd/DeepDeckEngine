use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DamageEffectNode, DamageRecipientNode, LocalConditionNode, LocalEffectNode,
    LocalObjectReference, NumericExpression, PlayerPredicate, SelectorPredicate, TargetListNode,
    TargetSelector, recognize_kicked_condition, recognize_numeric_atom,
};
use crate::oracle::{
    characteristic_vocabulary::PlayerRole,
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionalEffectNode {
    pub condition: LocalConditionNode,
    pub then_effects: Vec<LocalEffectNode>,
    pub else_effects: Vec<LocalEffectNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplacementState {
    Condition,
    SourceAnaphor,
    Action,
    Amount,
    Resource,
    TargetAnaphorOrInstead,
    Instead,
    Terminator,
    Complete,
}

#[derive(Default)]
struct ReplacementMemory {
    condition: Option<LocalConditionNode>,
    source: Option<LocalObjectReference>,
    amount: Option<NumericExpression>,
    target: Option<TargetListNode>,
}

/// Recognizes a later sentence that conditionally replaces a prior damage
/// effect while preserving its source and target bindings.
pub(in crate::oracle::canonical) fn recognize_conditional_damage_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: &DamageEffectNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ConditionalEffectNode> {
    let mut state = ReplacementState::Condition;
    let mut memory = ReplacementMemory::default();
    loop {
        state = match state {
            ReplacementState::Condition => {
                cursor.enter("replacement.damage.condition");
                memory.condition = Some(recognize_kicked_condition(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                ReplacementState::SourceAnaphor
            }
            ReplacementState::SourceAnaphor => {
                cursor.enter("replacement.damage.source-anaphor");
                cursor.consume("it")?;
                memory.source = Some(antecedent.source.clone());
                ReplacementState::Action
            }
            ReplacementState::Action => {
                cursor.enter("replacement.damage.action");
                cursor.consume("deals")?;
                ReplacementState::Amount
            }
            ReplacementState::Amount => {
                cursor.enter("replacement.damage.amount");
                memory.amount = Some(NumericExpression::Atom(recognize_numeric_atom(cursor)?));
                ReplacementState::Resource
            }
            ReplacementState::Resource => {
                cursor.enter("replacement.damage.resource");
                cursor.consume("damage")?;
                ReplacementState::TargetAnaphorOrInstead
            }
            ReplacementState::TargetAnaphorOrInstead => {
                cursor.enter("replacement.damage.target-anaphor-or-instead");
                let DamageRecipientNode::Targets(antecedent_target) = &antecedent.recipient else {
                    return Err(cursor.fail("a prior targeted damage recipient"));
                };
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("to"))
                {
                    cursor.consume_phrase(&["to", "that"])?;
                    recognize_target_anaphor(cursor, antecedent_target)?;
                }
                memory.target = Some(antecedent_target.clone());
                ReplacementState::Instead
            }
            ReplacementState::Instead => {
                cursor.enter("replacement.damage.instead");
                cursor.consume("instead")?;
                ReplacementState::Terminator
            }
            ReplacementState::Terminator => {
                cursor.enter("replacement.damage.terminator");
                cursor.consume(".")?;
                ReplacementState::Complete
            }
            ReplacementState::Complete => {
                cursor.enter("replacement.damage.complete");
                let replacement = LocalEffectNode::DealDamage(DamageEffectNode {
                    source: cursor.require_value(memory.source, "an anaphoric damage source")?,
                    amount: cursor.require_value(memory.amount, "a replacement damage amount")?,
                    recipient: DamageRecipientNode::Targets(
                        cursor.require_value(memory.target, "an anaphoric damage target")?,
                    ),
                });
                return Ok(ConditionalEffectNode {
                    condition: cursor.require_value(
                        memory.condition,
                        "a condition for the replacement effect",
                    )?,
                    then_effects: vec![replacement],
                    else_effects: vec![LocalEffectNode::DealDamage(antecedent.clone())],
                });
            }
        };
    }
}

fn recognize_target_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: &TargetListNode,
) -> MachineResult<()> {
    cursor.enter("reference.target-anaphor.noun");
    let slot = antecedent
        .slots
        .first()
        .ok_or_else(|| cursor.fail("an antecedent target slot"))?;
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a noun compatible with the antecedent target"))?;
    let compatible = if noun.eq_ignore_ascii_case("target") {
        true
    } else {
        match &slot.selector {
            TargetSelector::Any | TargetSelector::Union(_) => false,
            TargetSelector::Player(player) => noun.eq_ignore_ascii_case(
                match player {
                    PlayerPredicate::Any => PlayerRole::Any,
                    PlayerPredicate::Opponent => PlayerRole::Opponent,
                }
                .oracle_singular(),
            ),
            TargetSelector::Object(predicate)
            | TargetSelector::ControlledObject { predicate, .. } => object_noun(predicate, noun),
            TargetSelector::StackItem(_) => false,
        }
    };
    if !compatible {
        return Err(cursor.fail("a noun compatible with the antecedent target"));
    }
    let noun = noun.to_string();
    cursor.consume(&noun)
}

fn object_noun(predicate: &SelectorPredicate, noun: &str) -> bool {
    match predicate {
        SelectorPredicate::CardType(card_type) => {
            noun.eq_ignore_ascii_case(card_type.oracle_singular())
        }
        SelectorPredicate::ObjectKind(object_kind) => {
            noun.eq_ignore_ascii_case(object_kind.oracle_singular())
        }
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::Color(_)
        | SelectorPredicate::ColorQuantity(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Status(_)
        | SelectorPredicate::PermanentCard
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::Not(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::ChosenCreatureType
        | SelectorPredicate::NumericComparison { .. } => false,
        SelectorPredicate::Intersection(operands) | SelectorPredicate::Union(operands) => {
            operands.iter().any(|operand| object_noun(operand, noun))
        }
    }
}
