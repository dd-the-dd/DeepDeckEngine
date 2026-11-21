use crate::oracle::{counter_name::CounterName, syntax::AbilityInput};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, GrantedKeyword, NumericAtom, PowerToughnessNode, TargetCardinality,
    TargetListNode, TargetSelector, recognize_activation_cost_sequence, recognize_counter_name,
    recognize_keyword_list, recognize_literal_power_toughness, recognize_numeric_atom,
    recognize_target_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EarthbendReminderNode {
    pub target: TargetListNode,
    pub power_toughness: PowerToughnessNode,
    pub keyword: GrantedKeyword,
    pub counter: CounterName,
    pub counter_count: NumericAtom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EarthbendActivationNode {
    pub costs: ActivationCostSequenceNode,
    pub quantity: NumericAtom,
    pub reminder: Option<EarthbendReminderNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EarthbendActionNode {
    pub quantity: NumericAtom,
}

/// Recognizes the reusable keyword-action core. Ability envelopes own costs,
/// timing, punctuation, and any value binding required by `X`.
pub(in crate::oracle::canonical) fn recognize_earthbend_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EarthbendActionNode> {
    cursor.enter("earthbend.action.keyword");
    cursor.consume("earthbend")?;
    cursor.enter("earthbend.action.quantity");
    let quantity = recognize_numeric_atom(cursor)?;
    Ok(EarthbendActionNode { quantity })
}

fn recognize_earthbend_target(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<TargetListNode> {
    cursor.enter("earthbend.reminder.target");
    let target = recognize_target_list(cursor)?;
    cursor.enter("earthbend.reminder.target-shape");
    let [slot] = target.slots.as_slice() else {
        return Err(cursor.fail("one controlled land target"));
    };
    cursor.require(
        slot.cardinality == TargetCardinality::EXACTLY_ONE,
        "exactly one controlled land target",
    )?;
    let TargetSelector::ControlledObject {
        predicate,
        controller,
    } = &slot.selector
    else {
        return Err(cursor.fail("a controlled land target"));
    };
    cursor.require(
        *controller == super::ObjectControllerScope::AbilityController
            && matches!(
                predicate,
                super::ObjectPredicate::CardType(super::ObjectCardType::Land)
            ),
        "a land target controlled by the ability controller",
    )?;
    Ok(target)
}

fn recognize_earthbend_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EarthbendReminderNode> {
    cursor.enter("earthbend.reminder.open");
    cursor.consume("(")?;

    let target = recognize_earthbend_target(cursor)?;
    cursor.enter("earthbend.reminder.animation.action");
    cursor.consume("becomes")?;
    cursor.enter("earthbend.reminder.animation.article");
    cursor.consume("a")?;
    cursor.enter("earthbend.reminder.animation.statistics");
    let power_toughness = recognize_literal_power_toughness(cursor)?;
    cursor.enter("earthbend.reminder.animation.type");
    cursor.consume("creature")?;
    cursor.enter("earthbend.reminder.animation.keyword-relation");
    cursor.consume("with")?;
    cursor.enter("earthbend.reminder.animation.keyword");
    let keyword_list = recognize_keyword_list(cursor)?;
    let [keyword] = keyword_list.as_slice() else {
        return Err(cursor.fail("one parameterless animation keyword"));
    };
    cursor.enter("earthbend.reminder.animation.retained-type");
    cursor.consume_phrase(&["that's", "still", "a", "land", "."])?;

    cursor.enter("earthbend.reminder.counters.action");
    cursor.consume("put")?;
    cursor.enter("earthbend.reminder.counters.count");
    let counter_count = recognize_numeric_atom(cursor)?;
    cursor.enter("earthbend.reminder.counters.kind");
    let counter = recognize_counter_name(cursor)?;
    cursor.enter("earthbend.reminder.counters.noun");
    cursor.consume(if counter_count == NumericAtom::Integer(1) {
        "counter"
    } else {
        "counters"
    })?;
    cursor.enter("earthbend.reminder.counters.recipient");
    cursor.consume_phrase(&["on", "it", "."])?;

    cursor.enter("earthbend.reminder.return.trigger");
    cursor.consume_phrase(&["when", "it", "dies", "or", "is", "exiled", ","])?;
    cursor.enter("earthbend.reminder.return.action");
    cursor.consume_phrase(&["return", "it", "to", "the", "battlefield", "tapped", "."])?;
    cursor.enter("earthbend.reminder.close");
    cursor.consume(")")?;

    Ok(EarthbendReminderNode {
        target,
        power_toughness,
        keyword: *keyword,
        counter,
        counter_count,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Costs,
    Separator,
    Keyword,
    Terminator,
    Timing,
    ReminderOrComplete,
    Complete,
}

#[derive(Default)]
struct Frame {
    costs: Option<ActivationCostSequenceNode>,
    quantity: Option<NumericAtom>,
    reminder: Option<EarthbendReminderNode>,
}

/// Recognizes an activated Earthbend action and validates its rules reminder
/// from one immutable token stream. The keyword identity is syntax only; the
/// lowering layer emits independent effects.
pub(in crate::oracle::canonical) fn recognize_earthbend_activation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EarthbendActivationNode> {
    let mut state = State::Costs;
    let mut frame = Frame::default();
    loop {
        state = match state {
            State::Costs => {
                cursor.enter("earthbend.activation.costs");
                frame.costs = Some(recognize_activation_cost_sequence(cursor)?);
                State::Separator
            }
            State::Separator => {
                cursor.enter("earthbend.activation.separator");
                cursor.consume(":")?;
                State::Keyword
            }
            State::Keyword => {
                let action = recognize_earthbend_action(cursor)?;
                let quantity = action.quantity;
                if quantity == NumericAtom::VariableX {
                    cursor.enter("earthbend.action.variable-binding");
                    let has_cost_binding = frame.costs.as_ref().is_some_and(|costs| {
                        costs.costs.iter().any(|cost| {
                            matches!(
                                cost,
                                super::LocalActivationCostNode::PayMana(mana)
                                    if mana.contains_variable_x()
                            )
                        })
                    });
                    cursor.require(
                        has_cost_binding,
                        "an {X} mana cost binding the Earthbend X value",
                    )?;
                }
                frame.quantity = Some(quantity);
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("earthbend.action.terminator");
                cursor.consume(".")?;
                State::Timing
            }
            State::Timing => {
                cursor.enter("activation.timing.sorcery");
                cursor.consume_phrase(&["activate", "only", "as", "a", "sorcery", "."])?;
                State::ReminderOrComplete
            }
            State::ReminderOrComplete => {
                cursor.enter("earthbend.activation.reminder-or-complete");
                if cursor.peek().is_some_and(|raw| raw == "(") {
                    let reminder = recognize_earthbend_reminder(cursor)?;
                    let quantity =
                        cursor.require_value(frame.quantity.clone(), "the Earthbend quantity")?;
                    cursor.enter("earthbend.reminder.quantity-match");
                    cursor.require(
                        reminder.counter_count == quantity,
                        "a reminder counter quantity equal to the Earthbend quantity",
                    )?;
                    cursor.require(
                        reminder.power_toughness
                            == (PowerToughnessNode {
                                power: 0,
                                toughness: 0,
                            }),
                        "the canonical 0/0 Earthbend base characteristics",
                    )?;
                    cursor.require(
                        reminder.keyword == GrantedKeyword::Haste,
                        "the canonical Earthbend haste grant",
                    )?;
                    cursor.require(
                        reminder.counter == "+1/+1",
                        "the canonical Earthbend +1/+1 counter kind",
                    )?;
                    frame.reminder = Some(reminder);
                }
                State::Complete
            }
            State::Complete => {
                cursor.enter("earthbend.activation.complete");
                cursor.finish()?;
                return Ok(EarthbendActivationNode {
                    costs: cursor.require_value(frame.costs, "populated activation costs")?,
                    quantity: cursor
                        .require_value(frame.quantity, "a populated Earthbend quantity")?,
                    reminder: frame.reminder,
                });
            }
        };
    }
}

/// Structural family claim: cost syntax and the colon remain owned by the
/// child; only the action keyword after the separator commits this family.
pub(in crate::oracle::canonical) fn claims_earthbend_activation(input: &AbilityInput<'_>) -> bool {
    input
        .tokens
        .iter()
        .position(|token| token.raw == ":")
        .and_then(|separator| input.tokens.get(separator + 1))
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("earthbend"))
}

#[cfg(test)]
#[path = "earthbend_tests.rs"]
mod tests;
