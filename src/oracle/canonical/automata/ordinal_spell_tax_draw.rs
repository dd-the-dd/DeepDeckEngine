use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DrawCardsEffectNode, NumericAtom, NumericExpression, ObjectKind, ObjectPredicate,
    PlayerPredicate, SpellCastEventCaster, SpellCastOrdinalBasis, SpellCastOrdinalSupport,
    TriggeringPlayerBinding, recognize_draw_cards_effect, recognize_mana_cost,
    recognize_source_property_variable_definition, recognize_spell_cast_event_atom,
    recognize_triggering_player_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OrdinalSpellCastEventNode {
    pub caster: PlayerPredicate,
    pub triggering_player: TriggeringPlayerBinding,
    pub ordinal: i64,
    pub filter: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DynamicManaPaymentNode {
    pub payer: TriggeringPlayerBinding,
    pub amount: NumericExpression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OrdinalSpellTaxDrawTriggeredAbilityNode {
    pub event: OrdinalSpellCastEventNode,
    pub draw: DrawCardsEffectNode,
    pub payment: DynamicManaPaymentNode,
}

/// Existing filtered-tax view. Its event words are parsed once by the common
/// reader; this projection preserves the legacy consumer's explicit contract.
pub(in crate::oracle::canonical) fn recognize_ordinal_filtered_spell_cast_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OrdinalSpellCastEventNode> {
    let event =
        recognize_spell_cast_event_atom(cursor, SpellCastOrdinalSupport::MatchingSpellOrdinals)?;
    cursor.enter("event.spell-cast.tax-filtered-shape");
    cursor.require(
        event.caster == SpellCastEventCaster::Opponent,
        "the opponent player role for the existing ordinal tax consumer",
    )?;
    let ordinal = event
        .ordinal
        .ok_or_else(|| cursor.fail("a spell-cast ordinal for this tax consumer"))?;
    cursor.require(
        match ordinal.basis {
            SpellCastOrdinalBasis::AllSpellsByCasterThisTurn => {
                event.query.predicate == ObjectPredicate::ObjectKind(ObjectKind::Spell)
            }
            SpellCastOrdinalBasis::MatchingSpellsByCasterThisTurn => {
                event.query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Spell)
            }
        },
        "a spell-cast ordinal whose basis preserves this tax consumer's exact predicate",
    )?;
    Ok(OrdinalSpellCastEventNode {
        caster: PlayerPredicate::Opponent,
        triggering_player: event.triggering_player,
        ordinal: ordinal.value,
        filter: event.query.predicate,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DynamicPaymentState {
    Unless,
    Payer,
    PaymentAction,
    VariableCost,
    DefinitionSeparator,
    Definition,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_dynamic_mana_payment(
    cursor: &mut OracleTokenCursor<'_>,
    triggering_player: Option<TriggeringPlayerBinding>,
) -> MachineResult<DynamicManaPaymentNode> {
    let mut state = DynamicPaymentState::Unless;
    let mut payer = None;
    let mut amount = None;
    loop {
        state = match state {
            DynamicPaymentState::Unless => {
                cursor.enter("effect.dynamic-mana-payment.unless");
                cursor.consume("unless")?;
                DynamicPaymentState::Payer
            }
            DynamicPaymentState::Payer => {
                cursor.enter("effect.dynamic-mana-payment.payer");
                payer = Some(recognize_triggering_player_reference(
                    cursor,
                    triggering_player,
                )?);
                DynamicPaymentState::PaymentAction
            }
            DynamicPaymentState::PaymentAction => {
                cursor.enter("effect.dynamic-mana-payment.action");
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(action.raw.eq_ignore_ascii_case("pays"), "the `pays` action")?;
                DynamicPaymentState::VariableCost
            }
            DynamicPaymentState::VariableCost => {
                cursor.enter("effect.dynamic-mana-payment.variable-cost");
                let cost = recognize_mana_cost(cursor)?;
                cursor.require(
                    cost.contains_variable_x() && cost.as_oracle_cost().eq_ignore_ascii_case("{X}"),
                    "the single variable generic mana cost `{X}`",
                )?;
                DynamicPaymentState::DefinitionSeparator
            }
            DynamicPaymentState::DefinitionSeparator => {
                cursor.enter("numeric.source-property.definition-separator");
                cursor.consume(",")?;
                DynamicPaymentState::Definition
            }
            DynamicPaymentState::Definition => {
                cursor.enter("effect.dynamic-mana-payment.definition");
                amount = Some(recognize_source_property_variable_definition(
                    cursor,
                    NumericAtom::VariableX,
                )?);
                DynamicPaymentState::Complete
            }
            DynamicPaymentState::Complete => {
                cursor.enter("numeric.source-property.complete");
                return Ok(DynamicManaPaymentNode {
                    payer: cursor.require_value(payer, "an event-bound payer")?,
                    amount: cursor.require_value(amount, "a source numeric property")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrdinalSpellTaxDrawState {
    Trigger,
    Event,
    EventBoundary,
    Draw,
    Payment,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_ordinal_spell_tax_draw_triggered_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OrdinalSpellTaxDrawTriggeredAbilityNode> {
    let mut state = OrdinalSpellTaxDrawState::Trigger;
    let mut event = None;
    let mut draw = None;
    let mut payment = None;
    loop {
        state = match state {
            OrdinalSpellTaxDrawState::Trigger => {
                cursor.enter("trigger.ordinal-spell-tax-draw.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("whenever"),
                    "the repeated `whenever` trigger",
                )?;
                OrdinalSpellTaxDrawState::Event
            }
            OrdinalSpellTaxDrawState::Event => {
                cursor.enter("trigger.ordinal-spell-tax-draw.event");
                event = Some(recognize_ordinal_filtered_spell_cast_event(cursor)?);
                OrdinalSpellTaxDrawState::EventBoundary
            }
            OrdinalSpellTaxDrawState::EventBoundary => {
                cursor.enter("trigger.ordinal-spell-tax-draw.event-boundary");
                cursor.consume(",")?;
                OrdinalSpellTaxDrawState::Draw
            }
            OrdinalSpellTaxDrawState::Draw => {
                cursor.enter("trigger.ordinal-spell-tax-draw.draw");
                draw = Some(recognize_draw_cards_effect(cursor)?);
                OrdinalSpellTaxDrawState::Payment
            }
            OrdinalSpellTaxDrawState::Payment => {
                cursor.enter("trigger.ordinal-spell-tax-draw.payment");
                payment = Some(recognize_dynamic_mana_payment(
                    cursor,
                    event.as_ref().map(|event| event.triggering_player),
                )?);
                OrdinalSpellTaxDrawState::Terminator
            }
            OrdinalSpellTaxDrawState::Terminator => {
                cursor.enter("trigger.ordinal-spell-tax-draw.terminator");
                cursor.consume(".")?;
                OrdinalSpellTaxDrawState::Complete
            }
            OrdinalSpellTaxDrawState::Complete => {
                cursor.enter("trigger.ordinal-spell-tax-draw.complete");
                cursor.finish()?;
                return Ok(OrdinalSpellTaxDrawTriggeredAbilityNode {
                    event: cursor.require_value(event, "an ordinal filtered spell-cast event")?,
                    draw: cursor.require_value(draw, "a draw effect")?,
                    payment: cursor.require_value(payment, "a dynamic mana payment")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "ordinal_spell_tax_draw_tests.rs"]
mod tests;
