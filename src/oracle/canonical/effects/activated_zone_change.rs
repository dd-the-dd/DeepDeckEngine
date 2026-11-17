use super::super::*;
use crate::oracle::syntax::{AbilityInput, has_root_activation_effect_head};

pub(in crate::oracle::canonical) const ACTIVATED_CARD_ZONE_TARGET_ID: &str = "targetGraveyardCard";

/// Claims the structural family at the activation-cost/action boundary. The
/// committed child machine remains responsible for separators, articles,
/// source zones, punctuation, and full consumption.
pub(in crate::oracle::canonical) fn claims_promoted_activated_card_zone_change_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let supported_cost = tokens.first().is_some_and(|token| {
        token.token_type == "manaSymbol"
            || token.token_type == "tapSymbol"
            || token.raw.eq_ignore_ascii_case("pay")
            || token.raw.eq_ignore_ascii_case("sacrifice")
            || token.raw.eq_ignore_ascii_case("discard")
                && tokens
                    .get(1)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case("this"))
                && tokens
                    .get(2)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case("card"))
    });
    let has_card_from = tokens.windows(2).any(|window| {
        (window[0].raw.eq_ignore_ascii_case("card") || window[0].raw.eq_ignore_ascii_case("cards"))
            && window[1].raw.eq_ignore_ascii_case("from")
    });
    let card_zone_action = (has_root_activation_effect_head(input, &["exile"])
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("target")
                && (window[1].raw.eq_ignore_ascii_case("card")
                    || window[1].raw.eq_ignore_ascii_case("cards"))
        }))
        || (has_root_activation_effect_head(input, &["return", "target"]) && has_card_from);
    supported_cost && card_zone_action
}

pub(in crate::oracle::canonical) fn parse_promoted_activated_card_zone_change(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("activated-card-zone-change", input.tokens);
    let ability = recognize_activated_card_zone_change(&mut cursor)?;
    lower_activated_card_zone_change(&mut cursor, ability)
}

pub(in crate::oracle::canonical) enum ActivatedCardZoneProgramAttempt {
    Parsed(CanonicalRuleDraft),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_promoted_activated_card_zone_change(
    input: &AbilityInput<'_>,
) -> ActivatedCardZoneProgramAttempt {
    let mut cursor = OracleTokenCursor::new("activated-card-zone-change", input.tokens);
    match try_recognize_activated_card_zone_change(&mut cursor) {
        ActivatedCardZoneChangeParseAttempt::Parsed(node) => {
            match lower_activated_card_zone_change(&mut cursor, node) {
                Ok(draft) => ActivatedCardZoneProgramAttempt::Parsed(draft),
                Err(failure) => ActivatedCardZoneProgramAttempt::FailedCommitted(failure),
            }
        }
        ActivatedCardZoneChangeParseAttempt::RejectedCandidate(failure) => {
            ActivatedCardZoneProgramAttempt::RejectedCandidate(failure)
        }
        ActivatedCardZoneChangeParseAttempt::FailedCommitted(failure) => {
            ActivatedCardZoneProgramAttempt::FailedCommitted(failure)
        }
    }
}

fn lower_activated_card_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
    ability: ActivatedCardZoneChangeNode,
) -> MachineResult<CanonicalRuleDraft> {
    let costs = ability
        .costs
        .costs
        .iter()
        .flat_map(lower_local_activation_costs)
        .collect::<Vec<_>>();
    let activates_from_hand = ability
        .costs
        .costs
        .iter()
        .any(|cost| matches!(cost, LocalActivationCostNode::DiscardSelf(_)));
    let candidates = lower_target_card_move_candidates(&ability.effect);
    let mut decision = json!({
        "id": ACTIVATED_CARD_ZONE_TARGET_ID,
        "kind": "chooseTargets",
        "minimum": lower_target_cardinality_bound(ability.effect.cardinality.minimum),
        "maximum": lower_target_cardinality_bound(ability.effect.cardinality.maximum),
        "candidates": candidates,
    });
    if ability.effect.source_zone == TargetCardSourceZone::SingleGraveyard
        && !matches!(ability.effect.cardinality.maximum, NumericAtom::Integer(1))
    {
        decision["selectionConstraint"] = json!({
            "kind": "sameZoneOwner",
            "zone": "graveyard",
        });
    }
    let uses_x = ability.effect.cardinality.uses_x();
    // A numeric X needs a producer on this face: the activation's own {X}.
    cursor.enter("activated-card-zone-change.x-value.source");
    cursor.require(
        !uses_x
            || ability.costs.costs.iter().any(|cost| {
                matches!(cost, LocalActivationCostNode::PayMana(mana) if mana.contains_variable_x())
            }),
        "an {X} activation cost declaring the X used by the target count",
    )?;
    let effect = match ability.effect.action {
        CardZoneChangeAction::Exile
            if ability.effect.cardinality == ExpressedTargetCardinality::EXACTLY_ONE =>
        {
            // The retired single-object path also installed a persistent
            // source relation. The current AST does not prove it unlinked;
            // preserve the explicit gap rather than silently discard that link.
            cursor.enter("activated-card-zone-change.generic-relation-unavailable");
            return Err(cursor.fail("proved moveCards with an explicit durable relation policy"));
        }
        CardZoneChangeAction::Exile => json!({
            "kind": "exileTargetCards",
            "cards": {
                "kind": "chosenTargets",
                "id": ACTIVATED_CARD_ZONE_TARGET_ID,
            },
        }),
        CardZoneChangeAction::Move(_) => {
            lower_target_card_move_effect(&ability.effect, ACTIVATED_CARD_ZONE_TARGET_ID)
                .ok_or_else(|| cursor.fail("a supported targeted card move"))?
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut decisions = Vec::new();
    if uses_x {
        decisions.push(x_value());
    }
    decisions.push(decision);
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": costs,
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": decisions,
        },
        "effects": [effect],
    });
    if activates_from_hand {
        rule["activationZone"] = Value::String("hand".to_string());
    }

    Ok(draft(
        with_activation_object_cost_decisions(rule, &ability.costs),
        &[
            "Recognize an activated card-zone change from one immutable token stream",
            "Compose activation costs through the shared cost-sequence automaton",
            "Reuse target cardinality independently from the graveyard card domain",
            "Keep card-zone targeting separate from battlefield permanent targeting",
            "Lower one or many chosen cards to existing atomic runtime primitives",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "activated_zone_change_tests.rs"]
mod tests;
