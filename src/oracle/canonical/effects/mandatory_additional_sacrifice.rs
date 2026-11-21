use super::super::*;
use crate::oracle::syntax::AbilityInput;

const ENVELOPE: &[&str] = MANDATORY_ADDITIONAL_COST_HEADER;

#[derive(Debug)]
pub(in crate::oracle::canonical) enum MandatoryAdditionalSacrificeParseAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

/// Token-only parent boundary. The child automata remain authoritative for the
/// sacrifice action and permanent predicate.
fn claims_promoted_mandatory_additional_sacrifice_family(input: &AbilityInput<'_>) -> bool {
    let tokens = input.tokens;
    ENVELOPE.iter().enumerate().all(|(index, expected)| {
        tokens
            .get(index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    }) && tokens
        .get(ENVELOPE.len())
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("sacrifice"))
        && tokens.get(ENVELOPE.len() + 1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an")
        })
        && !tokens.windows(2).any(|window| {
            let is_cost_connector = window[0].raw.eq_ignore_ascii_case("and")
                || window[0].raw.eq_ignore_ascii_case("or");
            let starts_another_action = matches!(
                window[1].token_type.as_str(),
                "actionWord" | "costActionWord"
            );
            let starts_another_object =
                window[1].raw.eq_ignore_ascii_case("a") || window[1].raw.eq_ignore_ascii_case("an");
            is_cost_connector && (starts_another_action || starts_another_object)
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
        && tokens.iter().filter(|token| token.raw == ".").count() == 1
}

pub(in crate::oracle::canonical) fn try_parse_promoted_mandatory_additional_sacrifice(
    input: &AbilityInput<'_>,
) -> MandatoryAdditionalSacrificeParseAttempt {
    if !claims_promoted_mandatory_additional_sacrifice_family(input) {
        return MandatoryAdditionalSacrificeParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("mandatory-additional-sacrifice", input.tokens);
    let node = match try_recognize_mandatory_additional_sacrifice_cost(&mut cursor) {
        MandatoryAdditionalSacrificeCostAttempt::Parsed(node) => node,
        MandatoryAdditionalSacrificeCostAttempt::Unpromoted(failure) => {
            return MandatoryAdditionalSacrificeParseAttempt::Unpromoted(failure);
        }
        MandatoryAdditionalSacrificeCostAttempt::Failed(failure) => {
            return MandatoryAdditionalSacrificeParseAttempt::Failed(failure);
        }
    };
    let decision_id = "additionalCostPermanent";
    let candidates = json!({
        "kind": "permanents",
        "controller": controller(),
        "where": lower_permanent_object_predicate(&node.object.predicate),
    });
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    MandatoryAdditionalSacrificeParseAttempt::Parsed(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [object_choice(
                    decision_id,
                    candidates,
                    json!({ "kind": "exactly", "value": integer(1) }),
                )],
                "additionalCosts": [{
                    "kind": "sacrificePermanent",
                    "permanent": { "kind": "chosenObject", "id": decision_id },
                }],
            },
            "effects": [],
        }),
        &[
            "Recognize the mandatory additional-cost envelope from one token stream",
            "Delegate `sacrifice` to the shared atomic action",
            "Delegate the sacrificed permanent predicate to ObjectQuery",
            "Declare a non-target object choice before casting",
            "Pay one primitive sacrificePermanent cost",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "mandatory_additional_sacrifice_tests.rs"]
mod tests;
