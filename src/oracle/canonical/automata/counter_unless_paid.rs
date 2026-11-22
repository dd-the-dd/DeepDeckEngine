use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterEffectNode, LocalEffectNode, ManaCostNode, recognize_counter_effect, recognize_mana_cost,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterUnlessPaidEffectNode {
    pub counter: CounterEffectNode,
    pub payment: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterUnlessPaidState {
    Counter,
    Unless,
    PossessiveReference,
    Payer,
    PaymentAction,
    Payment,
    Terminator,
    Complete,
}

/// Composes the shared counter-target child with a controller mana-payment
/// exception. The exception is retained independently from the counter effect
/// so the same payment grammar can vary without changing target recognition.
pub(in crate::oracle::canonical) fn recognize_counter_unless_paid_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterUnlessPaidEffectNode> {
    let mut state = CounterUnlessPaidState::Counter;
    let mut counter = None;
    let mut payment = None;
    loop {
        state = match state {
            CounterUnlessPaidState::Counter => {
                cursor.enter("effect.counter-unless-paid.counter");
                let LocalEffectNode::CounterSpell(effect) = recognize_counter_effect(cursor)?
                else {
                    return Err(cursor.fail("a spell counter effect"));
                };
                counter = Some(effect);
                CounterUnlessPaidState::Unless
            }
            CounterUnlessPaidState::Unless => {
                cursor.enter("effect.counter-unless-paid.unless");
                cursor.consume("unless")?;
                CounterUnlessPaidState::PossessiveReference
            }
            CounterUnlessPaidState::PossessiveReference => {
                cursor.enter("effect.counter-unless-paid.possessive-reference");
                cursor.consume("its")?;
                CounterUnlessPaidState::Payer
            }
            CounterUnlessPaidState::Payer => {
                cursor.enter("effect.counter-unless-paid.payer");
                let payer = cursor.consume_type("playerRoleWord")?;
                cursor.require(
                    payer.raw.eq_ignore_ascii_case("controller"),
                    "the target spell's controller",
                )?;
                CounterUnlessPaidState::PaymentAction
            }
            CounterUnlessPaidState::PaymentAction => {
                cursor.enter("effect.counter-unless-paid.payment-action");
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Pay, VerbForm::ThirdPersonSingular),
                    "`pays`",
                )?;
                CounterUnlessPaidState::Payment
            }
            CounterUnlessPaidState::Payment => {
                cursor.enter("effect.counter-unless-paid.payment");
                payment = Some(recognize_mana_cost(cursor)?);
                CounterUnlessPaidState::Terminator
            }
            CounterUnlessPaidState::Terminator => {
                cursor.enter("effect.counter-unless-paid.terminator");
                cursor.consume(".")?;
                CounterUnlessPaidState::Complete
            }
            CounterUnlessPaidState::Complete => {
                cursor.enter("effect.counter-unless-paid.complete");
                cursor.finish()?;
                return Ok(CounterUnlessPaidEffectNode {
                    counter: cursor.require_value(counter, "a populated counter effect")?,
                    payment: cursor.require_value(payment, "a populated mana payment")?,
                });
            }
        };
    }
}
