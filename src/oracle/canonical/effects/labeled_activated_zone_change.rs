use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims an activated ability whose effect sequence is followed by a
/// per-permanent generic-cost reduction. Label spelling, costs, effects, and
/// metadata remain owned by typed child machines after this structural claim.
pub(in crate::oracle::canonical) fn claims_promoted_labeled_activated_zone_change_family(
    input: &AbilityInput<'_>,
) -> bool {
    // Any ability-word label was stripped at the ability boundary (CR 207.2c).
    let tokens = input.tokens;
    tokens.iter().any(|token| token.raw == ":")
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("this")
                && window[1].raw.eq_ignore_ascii_case("ability")
                && window[2].raw.eq_ignore_ascii_case("costs")
        })
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("less")
                && window[1].raw.eq_ignore_ascii_case("to")
                && window[2].raw.eq_ignore_ascii_case("activate")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_labeled_activated_zone_change(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("labeled-activated-zone-change", input.tokens);
    let ability = recognize_labeled_activated_effect_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("activation.labeled-zone-change.validation");
    cursor.require(
        ability
            .label
            .as_ref()
            .is_none_or(|label| !label.name.is_empty()),
        "a nonempty short ability label",
    )?;

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
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    let (decision, effects) = match ability.effects.effects.as_slice() {
        [LocalEffectNode::ZoneChange(zone_change)] => {
            cursor.require(
                ability.effects.connectors.is_empty(),
                "one standalone zone-change effect",
            )?;
            let (target_id, candidates, minimum, maximum) =
                lower_single_target(&zone_change.target);
            (
                target_decision(target_id, candidates, minimum, maximum),
                vec![lower_targeted_zone_change_node(
                    &mut cursor,
                    zone_change,
                    chosen_target(target_id),
                )?],
            )
        }
        [
            LocalEffectNode::ZoneChange(zone_change),
            LocalEffectNode::OptionalLibrarySearch(search),
        ] => {
            cursor.require(
                ability.effects.connectors == [EffectConnector::Then],
                "a zone change followed by one linked optional library search",
            )?;
            cursor.require(
                search.player == LocalPlayerReference::ControllerOf(binding),
                "the prior target's controller as the optional searching player",
            )?;
            lower_zone_change_search_components(&mut cursor, zone_change, search, binding)?
        }
        _ => {
            return Err(cursor.fail(
                "a zone change, optionally followed by its target controller's library search",
            ));
        }
    };
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": costs,
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [decision],
        },
        "effects": effects,
    });
    install_activation_cost_reduction(&mut cursor, &mut rule, &ability.reduction)?;
    if activates_from_hand {
        rule["activationZone"] = Value::String("hand".to_string());
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        with_activation_object_cost_decisions(rule, &ability.costs),
        &[
            "Recognize a short-labeled activated envelope from one token stream",
            "Compose mana and source-discard costs independently",
            "Reuse the shared zone-change and linked optional-search sequence",
            "Retain the activation cost reduction as metadata outside effects",
            &trace,
        ],
    ))
}
