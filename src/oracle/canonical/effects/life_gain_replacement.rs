//! Lowers a typed life-gain replacement (CR 614.1a) into one
//! battlefield-active `replaceLifeGain` modifier: the affected player
//! relation and an amount over `eventQuantity`.
use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum LifeGainReplacementParseAttempt {
    Parsed(CanonicalRuleDraft),
    NotClaimed,
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_life_gain_replacement(
    input: &AbilityInput<'_>,
) -> LifeGainReplacementParseAttempt {
    let mut cursor = OracleTokenCursor::new("life-gain-replacement", input.tokens);
    let node = match try_recognize_life_gain_replacement(&mut cursor) {
        LifeGainReplacementRecognition::Parsed(node) => node,
        LifeGainReplacementRecognition::NotClaimed => {
            return LifeGainReplacementParseAttempt::NotClaimed;
        }
        LifeGainReplacementRecognition::Failed(failure) => {
            return LifeGainReplacementParseAttempt::Failed(failure);
        }
    };
    cursor.enter("replacement.life-gain.source-domain");
    if let Err(failure) = cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for a battlefield-active life-gain replacement",
    ) {
        return LifeGainReplacementParseAttempt::Failed(failure);
    }
    cursor.enter("replacement.life-gain.lowering");
    let Some(quantity) =
        lower_numeric_expression_for_event(&node.amount, EventNumericBinding::ReplacementQuantity)
    else {
        return LifeGainReplacementParseAttempt::Failed(
            cursor.fail("an amount over the replaced event's quantity"),
        );
    };
    let player = match node.player {
        LifeGainPlayer::You => controller(),
        LifeGainPlayer::Opponent => json!({ "kind": "opponentsOf", "player": controller() }),
        LifeGainPlayer::AnyPlayer => json!({ "kind": "eachPlayer" }),
    };
    LifeGainReplacementParseAttempt::Parsed(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "replaceLifeGain",
                "event": { "player": player },
                "quantity": quantity,
            }],
        }),
        &[
            "Read the affected player of the life-gain event",
            "Read the replacing gain's amount over the event quantity",
            "Lower to one battlefield-active life-gain replacement (CR 614.1a)",
        ],
    ))
}

#[cfg(test)]
#[path = "life_gain_replacement_tests.rs"]
mod tests;
