use super::super::*;
use crate::oracle::syntax::AbilityInput;

#[derive(Debug)]
pub(in crate::oracle::canonical) enum MandatoryAdditionalDiscardParseAttempt {
    Parsed(CanonicalRuleDraft),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
    NotClaimed,
}

fn claims_mandatory_discard(input: &AbilityInput<'_>) -> bool {
    let tokens = input.tokens;
    has_mandatory_additional_cost_header(tokens)
        && tokens
            .get(MANDATORY_ADDITIONAL_COST_HEADER.len())
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("discard"))
        && tokens.last().is_some_and(|token| token.raw == ".")
        && tokens.iter().filter(|token| token.raw == ".").count() == 1
        && !tokens.windows(2).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("and") || window[0].raw.eq_ignore_ascii_case("or"))
                && matches!(
                    window[1].token_type.as_str(),
                    "actionWord" | "costActionWord"
                )
        })
}

pub(in crate::oracle::canonical) fn try_parse_promoted_mandatory_additional_discard(
    input: &AbilityInput<'_>,
) -> MandatoryAdditionalDiscardParseAttempt {
    if !claims_mandatory_discard(input) {
        return MandatoryAdditionalDiscardParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("mandatory-additional-discard", input.tokens);
    let node = match try_recognize_mandatory_additional_discard_cost(&mut cursor) {
        MandatoryAdditionalDiscardCostAttempt::Parsed(node) => node,
        MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure) => {
            return MandatoryAdditionalDiscardParseAttempt::Unpromoted(failure);
        }
        MandatoryAdditionalDiscardCostAttempt::Failed(failure) => {
            return MandatoryAdditionalDiscardParseAttempt::Failed(failure);
        }
    };
    let decision_id = "additionalCostCard";
    let costs = (0..node.quantity)
        .map(|index| {
            let id = if node.quantity == 1 {
                decision_id.to_string()
            } else {
                format!("{decision_id}:{index}")
            };
            json!({ "kind": "discardCard", "card": { "kind": "chosenObject", "id": id } })
        })
        .collect::<Vec<_>>();
    MandatoryAdditionalDiscardParseAttempt::Parsed(draft(
        json!({
            "kind": "spellAbility", "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [object_choice(
                    decision_id,
                    json!({ "kind": "cards", "zone": hand(controller()), "where": lower_card_object_predicate(&node.filter) }),
                    json!({ "kind": "exactly", "value": integer(node.quantity) }),
                )],
                "additionalCosts": costs,
            },
            "effects": [],
        }),
        &[
            "Recognize the shared mandatory casting-cost header",
            "Reuse the atomic discard action and typed card selector",
            "Choose distinct cards non-targetingly from the caster's hand",
            "Pay one existing discardCard primitive per selected scalar slot",
            "Retain random, alternatives and contextual quantities as explicit unsupported boundaries",
        ],
    ))
}

#[cfg(test)]
#[path = "mandatory_additional_discard_tests.rs"]
mod tests;
