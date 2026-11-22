use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CreateNamedTokensEffectNode, LocalTriggerEventNode, ManaCostNode, PlayerPredicate,
    TriggeringPlayerBinding, recognize_card_drawn_event, recognize_create_named_tokens_effect,
    recognize_mana_cost, recognize_named_token_creation_reminder,
    recognize_triggering_player_reference,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventBoundPaymentDeclineNode {
    pub payer: TriggeringPlayerBinding,
    pub cost: ManaCostNode,
    pub decline_effect: CreateNamedTokensEffectNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardDrawPaymentTriggeredAbilityNode {
    pub event: LocalTriggerEventNode,
    pub payment: EventBoundPaymentDeclineNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaymentDeclineState {
    Payer,
    Modality,
    PaymentAction,
    Cost,
    PaymentTerminator,
    Conditional,
    DefinitePayer,
    NegativePredicate,
    ConditionalBoundary,
    DeclineEffect,
    DeclineTerminator,
    ReminderOrComplete,
    Complete,
}

/// Recognizes an event-bound optional payment followed by an effect performed
/// when the same player declines. Event binding, mana cost, and effect remain
/// typed child results over the single shared token stream.
pub(in crate::oracle::canonical) fn recognize_event_bound_payment_decline(
    cursor: &mut OracleTokenCursor<'_>,
    triggering_player: Option<TriggeringPlayerBinding>,
) -> MachineResult<EventBoundPaymentDeclineNode> {
    let mut state = PaymentDeclineState::Payer;
    let mut payer = None;
    let mut cost = None;
    let mut decline_effect = None;
    loop {
        state = match state {
            PaymentDeclineState::Payer => {
                cursor.enter("effect.event-bound-payment.payer");
                payer = Some(recognize_triggering_player_reference(
                    cursor,
                    triggering_player,
                )?);
                PaymentDeclineState::Modality
            }
            PaymentDeclineState::Modality => {
                cursor.enter("effect.event-bound-payment.modality");
                let modality = cursor.consume_type("modalityWord")?;
                cursor.require(
                    modality.raw.eq_ignore_ascii_case("may"),
                    "the `may` modality",
                )?;
                PaymentDeclineState::PaymentAction
            }
            PaymentDeclineState::PaymentAction => {
                cursor.enter("effect.event-bound-payment.action");
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Pay, VerbForm::Base),
                    "the `pay` cost action",
                )?;
                PaymentDeclineState::Cost
            }
            PaymentDeclineState::Cost => {
                cursor.enter("effect.event-bound-payment.cost");
                cost = Some(recognize_mana_cost(cursor)?);
                PaymentDeclineState::PaymentTerminator
            }
            PaymentDeclineState::PaymentTerminator => {
                cursor.enter("effect.event-bound-payment.payment-terminator");
                cursor.consume(".")?;
                PaymentDeclineState::Conditional
            }
            PaymentDeclineState::Conditional => {
                cursor.enter("effect.event-bound-payment.conditional");
                cursor.consume("if")?;
                PaymentDeclineState::DefinitePayer
            }
            PaymentDeclineState::DefinitePayer => {
                cursor.enter("reference.triggering-player.definite-determiner");
                cursor.consume("the")?;
                cursor.enter("reference.triggering-player.definite-noun");
                cursor.consume("player")?;
                cursor.enter("reference.triggering-player.definite-binding");
                cursor.require(
                    triggering_player.is_some(),
                    "a prior triggering-player binding for `the player`",
                )?;
                PaymentDeclineState::NegativePredicate
            }
            PaymentDeclineState::NegativePredicate => {
                cursor.enter("effect.event-bound-payment.negative-predicate");
                let predicate = cursor.consume_type("negativePredicateWord")?;
                cursor.require(
                    matches!(
                        predicate.raw.to_ascii_lowercase().as_str(),
                        "doesn't" | "doesn’t"
                    ),
                    "the negative `doesn't` payment predicate",
                )?;
                PaymentDeclineState::ConditionalBoundary
            }
            PaymentDeclineState::ConditionalBoundary => {
                cursor.enter("effect.event-bound-payment.conditional-boundary");
                cursor.consume(",")?;
                PaymentDeclineState::DeclineEffect
            }
            PaymentDeclineState::DeclineEffect => {
                cursor.enter("effect.event-bound-payment.decline-effect");
                decline_effect = Some(recognize_create_named_tokens_effect(cursor, None)?);
                PaymentDeclineState::DeclineTerminator
            }
            PaymentDeclineState::DeclineTerminator => {
                cursor.enter("effect.event-bound-payment.decline-terminator");
                cursor.consume(".")?;
                PaymentDeclineState::ReminderOrComplete
            }
            PaymentDeclineState::ReminderOrComplete => {
                cursor.enter("effect.event-bound-payment.reminder-or-complete");
                if cursor.peek() == Some("(") {
                    let effect = decline_effect
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a completed named-token decline effect"))?;
                    recognize_named_token_creation_reminder(cursor, &effect.creation)?;
                }
                PaymentDeclineState::Complete
            }
            PaymentDeclineState::Complete => {
                cursor.enter("effect.event-bound-payment.complete");
                return Ok(EventBoundPaymentDeclineNode {
                    payer: cursor.require_value(payer, "an event-bound payer")?,
                    cost: cursor.require_value(cost, "a mana payment cost")?,
                    decline_effect: cursor
                        .require_value(decline_effect, "a typed decline effect")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardDrawPaymentState {
    Trigger,
    Event,
    EventBoundary,
    Payment,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_card_draw_payment_triggered_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardDrawPaymentTriggeredAbilityNode> {
    let mut state = CardDrawPaymentState::Trigger;
    let mut event = None;
    let mut payment = None;
    loop {
        state = match state {
            CardDrawPaymentState::Trigger => {
                cursor.enter("trigger.card-draw-payment.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("whenever"),
                    "the repeated `whenever` trigger",
                )?;
                CardDrawPaymentState::Event
            }
            CardDrawPaymentState::Event => {
                cursor.enter("trigger.card-draw-payment.event");
                let parsed = recognize_card_drawn_event(cursor)?;
                cursor.enter("trigger.card-draw-payment.event-shape");
                cursor.require(
                    matches!(
                        parsed,
                        LocalTriggerEventNode::CardDrawn {
                            player: PlayerPredicate::Opponent,
                            exception: None,
                        }
                    ),
                    "an opponent drawing a card",
                )?;
                event = Some(parsed);
                CardDrawPaymentState::EventBoundary
            }
            CardDrawPaymentState::EventBoundary => {
                cursor.enter("trigger.card-draw-payment.event-boundary");
                cursor.consume(",")?;
                CardDrawPaymentState::Payment
            }
            CardDrawPaymentState::Payment => {
                cursor.enter("trigger.card-draw-payment.payment");
                payment = Some(recognize_event_bound_payment_decline(
                    cursor,
                    Some(TriggeringPlayerBinding),
                )?);
                CardDrawPaymentState::Complete
            }
            CardDrawPaymentState::Complete => {
                cursor.enter("trigger.card-draw-payment.complete");
                cursor.finish()?;
                return Ok(CardDrawPaymentTriggeredAbilityNode {
                    event: cursor.require_value(event, "a card-draw event")?,
                    payment: cursor.require_value(payment, "an optional payment effect")?,
                });
            }
        };
    }
}
