use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DrawCardsEffectNode, ManaCostNode, SpellCastEventNode, TriggeringPlayerBinding,
    recognize_draw_cards_effect, recognize_mana_cost, recognize_player_casts_spell_event,
    recognize_triggering_player_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OptionalDrawUnlessPaymentNode {
    pub draw: DrawCardsEffectNode,
    pub payer: TriggeringPlayerBinding,
    pub cost: ManaCostNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerSpellTaxDrawTriggeredAbilityNode {
    pub event: SpellCastEventNode,
    pub effect: OptionalDrawUnlessPaymentNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalDrawUnlessPaymentState {
    Player,
    Modality,
    Draw,
    Unless,
    Payer,
    PaymentAction,
    Cost,
    Complete,
}

/// Recognizes the independent effect shape
/// `you may draw N cards unless that player pays COST`.
/// The draw count, event-player anaphor, and mana cost remain shared children.
pub(in crate::oracle::canonical) fn recognize_optional_draw_unless_payment(
    cursor: &mut OracleTokenCursor<'_>,
    triggering_player: Option<TriggeringPlayerBinding>,
) -> MachineResult<OptionalDrawUnlessPaymentNode> {
    let mut state = OptionalDrawUnlessPaymentState::Player;
    let mut draw = None;
    let mut payer = None;
    let mut cost = None;
    loop {
        state = match state {
            OptionalDrawUnlessPaymentState::Player => {
                cursor.enter("effect.optional-draw-unless-payment.player");
                cursor.consume("you")?;
                OptionalDrawUnlessPaymentState::Modality
            }
            OptionalDrawUnlessPaymentState::Modality => {
                cursor.enter("effect.optional-draw-unless-payment.modality");
                let modality = cursor.consume_type("modalityWord")?;
                cursor.require(
                    modality.raw.eq_ignore_ascii_case("may"),
                    "the `may` modality",
                )?;
                OptionalDrawUnlessPaymentState::Draw
            }
            OptionalDrawUnlessPaymentState::Draw => {
                cursor.enter("effect.optional-draw-unless-payment.draw");
                draw = Some(recognize_draw_cards_effect(cursor)?);
                OptionalDrawUnlessPaymentState::Unless
            }
            OptionalDrawUnlessPaymentState::Unless => {
                cursor.enter("effect.optional-draw-unless-payment.unless");
                cursor.consume("unless")?;
                OptionalDrawUnlessPaymentState::Payer
            }
            OptionalDrawUnlessPaymentState::Payer => {
                cursor.enter("effect.optional-draw-unless-payment.payer");
                payer = Some(recognize_triggering_player_reference(
                    cursor,
                    triggering_player,
                )?);
                OptionalDrawUnlessPaymentState::PaymentAction
            }
            OptionalDrawUnlessPaymentState::PaymentAction => {
                cursor.enter("effect.optional-draw-unless-payment.payment-action");
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(action.raw.eq_ignore_ascii_case("pays"), "the `pays` action")?;
                OptionalDrawUnlessPaymentState::Cost
            }
            OptionalDrawUnlessPaymentState::Cost => {
                cursor.enter("effect.optional-draw-unless-payment.cost");
                cost = Some(recognize_mana_cost(cursor)?);
                OptionalDrawUnlessPaymentState::Complete
            }
            OptionalDrawUnlessPaymentState::Complete => {
                cursor.enter("effect.optional-draw-unless-payment.complete");
                return Ok(OptionalDrawUnlessPaymentNode {
                    draw: cursor.require_value(draw, "an optional draw effect")?,
                    payer: cursor.require_value(payer, "an event-bound payer")?,
                    cost: cursor.require_value(cost, "a mana payment cost")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerSpellTaxDrawState {
    Trigger,
    Event,
    EventBoundary,
    Effect,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_player_spell_tax_draw_triggered_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSpellTaxDrawTriggeredAbilityNode> {
    let mut state = PlayerSpellTaxDrawState::Trigger;
    let mut event = None;
    let mut effect = None;
    loop {
        state = match state {
            PlayerSpellTaxDrawState::Trigger => {
                cursor.enter("trigger.player-spell-tax-draw.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("whenever"),
                    "the repeated `whenever` trigger",
                )?;
                PlayerSpellTaxDrawState::Event
            }
            PlayerSpellTaxDrawState::Event => {
                cursor.enter("trigger.player-spell-tax-draw.event");
                event = Some(recognize_player_casts_spell_event(cursor)?);
                PlayerSpellTaxDrawState::EventBoundary
            }
            PlayerSpellTaxDrawState::EventBoundary => {
                cursor.enter("trigger.player-spell-tax-draw.event-boundary");
                cursor.consume(",")?;
                PlayerSpellTaxDrawState::Effect
            }
            PlayerSpellTaxDrawState::Effect => {
                cursor.enter("trigger.player-spell-tax-draw.effect");
                effect = Some(recognize_optional_draw_unless_payment(
                    cursor,
                    event.as_ref().map(|event| event.triggering_player),
                )?);
                PlayerSpellTaxDrawState::Terminator
            }
            PlayerSpellTaxDrawState::Terminator => {
                cursor.enter("trigger.player-spell-tax-draw.terminator");
                cursor.consume(".")?;
                PlayerSpellTaxDrawState::Complete
            }
            PlayerSpellTaxDrawState::Complete => {
                cursor.enter("trigger.player-spell-tax-draw.complete");
                cursor.finish()?;
                return Ok(PlayerSpellTaxDrawTriggeredAbilityNode {
                    event: cursor.require_value(event, "a player-casts-spell event")?,
                    effect: cursor.require_value(effect, "an optional taxed draw effect")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "player_spell_tax_draw_tests.rs"]
mod tests;
