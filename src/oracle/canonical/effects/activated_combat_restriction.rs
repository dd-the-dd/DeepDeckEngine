use super::super::*;
use crate::oracle::syntax::{AbilityInput, has_root_activation_effect_head};

pub(in crate::oracle::canonical) enum ActivatedCombatRestrictionParseAttempt {
    Parsed(CanonicalRuleDraft),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
    NotClaimed,
}

/// This scanner is only a dispatch hint. It does not parse a cost, selector,
/// duration, or effect. Their shared children are invoked once by the attempt.
fn is_combat_restriction_candidate(input: &AbilityInput<'_>) -> bool {
    let Some(separator) = input.tokens.iter().position(|token| token.raw == ":") else {
        return false;
    };
    let body = &input.tokens[separator + 1..];
    let Some(head) = body.first() else {
        return false;
    };
    let target_head = head.raw.eq_ignore_ascii_case("target")
        || head.raw.eq_ignore_ascii_case("another")
        || head.raw.eq_ignore_ascii_case("up")
        || head.token_type == "quantity";
    if !target_head || !has_root_activation_effect_head(input, &[head.raw.as_str()]) {
        return false;
    }
    body.iter()
        .take_while(|token| token.raw != ".")
        .enumerate()
        .any(|(index, token)| {
            (token.raw.eq_ignore_ascii_case("can't") || token.raw.eq_ignore_ascii_case("can’t"))
                && (body.get(index + 1).is_some_and(|action| {
                    action.raw.eq_ignore_ascii_case("attack")
                        || action.raw.eq_ignore_ascii_case("block")
                }) || (body
                    .get(index + 1)
                    .is_some_and(|action| action.raw.eq_ignore_ascii_case("be"))
                    && body
                        .get(index + 2)
                        .is_some_and(|action| action.raw.eq_ignore_ascii_case("blocked"))))
        })
}

/// Parses an activated combat-restriction program exactly once. Uncovered
/// costs, suffixes, and lowering shapes retain a structured migration boundary;
/// committed syntax errors and lowering invariants remain hard failures.
pub(in crate::oracle::canonical) fn try_parse_promoted_activated_combat_restriction(
    input: &AbilityInput<'_>,
) -> ActivatedCombatRestrictionParseAttempt {
    if !is_combat_restriction_candidate(input) {
        return ActivatedCombatRestrictionParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("activated-combat-restriction", input.tokens);
    let ability = match try_recognize_activated_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &EffectRecognitionContext::default(),
    ) {
        ActivatedEffectSequenceParseAttempt::Parsed(ability) => ability,
        ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure) => {
            return ActivatedCombatRestrictionParseAttempt::Unpromoted(failure);
        }
        ActivatedEffectSequenceParseAttempt::FailedCommitted(failure) => {
            return ActivatedCombatRestrictionParseAttempt::Failed(failure);
        }
    };
    if !ability
        .effects
        .effects
        .iter()
        .any(|effect| matches!(effect, LocalEffectNode::TemporaryKeywordGrant(_)))
    {
        return ActivatedCombatRestrictionParseAttempt::NotClaimed;
    }
    let lowered = match try_lower_local_effect_sequence(&mut cursor, &ability.effects) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return ActivatedCombatRestrictionParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure) => {
            return ActivatedCombatRestrictionParseAttempt::Failed(failure);
        }
        EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return ActivatedCombatRestrictionParseAttempt::Failed(failure);
        }
    };
    let costs = ability
        .costs
        .costs
        .iter()
        .map(lower_local_activation_cost)
        .collect::<Vec<_>>();
    let mut decisions = lower_activation_cost_decisions(&ability.costs);
    decisions.extend(lowered.decisions);

    ActivatedCombatRestrictionParseAttempt::Parsed(draft(
        with_activation_object_cost_source_context(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                },
                "effects": lowered.effects,
            }),
            &ability.costs,
        ),
        &[
            "Compose the shared activation-cost and effect-sequence machines once",
            "Recognize a typed target combat restriction and duration",
            "Lower target declaration and grantKeyword independently",
        ],
    ))
}

#[cfg(test)]
#[path = "activated_combat_restriction_tests.rs"]
mod tests;
