use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_card_draw_payment_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 11
        && tokens[0].token_type == "triggerWord"
        && tokens[0].raw.eq_ignore_ascii_case("whenever")
        && tokens[1].raw.eq_ignore_ascii_case("an")
        && tokens[2].raw.eq_ignore_ascii_case("opponent")
        && tokens[3].raw.eq_ignore_ascii_case("draws")
        && tokens[4].raw.eq_ignore_ascii_case("a")
        && tokens[5].raw.eq_ignore_ascii_case("card")
        && tokens[6].raw == ","
        && tokens[7].raw.eq_ignore_ascii_case("that")
        && tokens[8].raw.eq_ignore_ascii_case("player")
        && tokens[9].raw.eq_ignore_ascii_case("may")
        && is_verb_form(&tokens[10].raw, VerbLemma::Pay, VerbForm::Base)
}

pub(in crate::oracle::canonical) fn parse_promoted_card_draw_payment(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("card-draw-payment", input.tokens);
    let ability = recognize_card_draw_payment_triggered_ability(&mut cursor)?;
    let LocalTriggerEventNode::CardDrawn {
        player: PlayerPredicate::Opponent,
        exception: None,
    } = ability.event
    else {
        cursor.enter("trigger.card-draw-payment.lower-event");
        return Err(cursor.fail("an unqualified opponent card-draw event"));
    };
    let decline = ability.payment.decline_effect;
    cursor.enter("trigger.card-draw-payment.lower-player");
    cursor.require(
        decline.player == LocalPlayerReference::Controller,
        "the ability controller as the decline-effect beneficiary",
    )?;
    let decline_effect =
        lower_named_token_creation(&mut cursor, &decline.creation, controller(), true)?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "cardDrawn",
                "opponentOfSourceController": true,
            },
            "effects": [{
                "kind": "optionalPayCostPerformEffects",
                "player": { "kind": "triggeringPlayer" },
                "cost": {
                    "kind": "payMana",
                    "manaCost": ability.payment.cost.as_oracle_cost(),
                },
                "effects": [],
                "declineEffects": [decline_effect],
            }],
        }),
        &[
            "Recognize the opponent card-draw event and bind its player",
            "Resolve both payer anaphors against the event binding",
            "Compose optional mana payment and typed named-token creation primitives",
            &trace,
        ],
    ))
}
