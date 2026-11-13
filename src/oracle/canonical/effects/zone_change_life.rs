use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Token-shape boundary for a targeted battlefield zone change followed by a
/// controller life-total clause. Selector, punctuation, numeric expression,
/// and life grammar remain owned by their shared child automata.
pub(in crate::oracle::canonical) fn claims_promoted_zone_change_life_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let action = tokens.first().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy")
            || token.raw.eq_ignore_ascii_case("exile")
            || token.raw.eq_ignore_ascii_case("return")
    });
    let targeted = tokens
        .get(1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"));
    let controller_life_clause = tokens.windows(3).any(|window| {
        window[0].raw == "."
            && window[1].raw.eq_ignore_ascii_case("you")
            && (window[2].raw.eq_ignore_ascii_case("gain")
                || window[2].raw.eq_ignore_ascii_case("lose"))
    });
    action && targeted && controller_life_clause
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetedZoneChangeLifeNode {
    zone_change: ZoneChangeEffectNode,
    life: LocalEffectNode,
}

/// Commits the typed children returned by EffectSequence into the narrow
/// cross-effect memory needed by this spell envelope. No child reparses text.
fn recognize_targeted_zone_change_life_node(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: EffectSequenceNode,
) -> MachineResult<TargetedZoneChangeLifeNode> {
    cursor.enter("sequence.zone-change-life.validation");
    let [LocalEffectNode::ZoneChange(zone_change), life] = sequence.effects.as_slice() else {
        return Err(cursor.fail("one targeted zone change followed by one life-total effect"));
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "one inter-sentence effect sequence",
    )?;
    let player = match life {
        LocalEffectNode::GainLife(effect) => &effect.player,
        LocalEffectNode::LoseLife(effect) => &effect.player,
        _ => return Err(cursor.fail("a gain-life or lose-life effect")),
    };
    cursor.require(
        *player == LocalPlayerReference::Controller,
        "the ability controller as life-total player",
    )?;
    Ok(TargetedZoneChangeLifeNode {
        zone_change: zone_change.clone(),
        life: life.clone(),
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_zone_change_life(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("zone-change-life-sequence", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    let node = recognize_targeted_zone_change_life_node(&mut cursor, sequence)?;

    let (target_id, candidates, minimum, maximum) = lower_single_target(&node.zone_change.target);
    let target = chosen_target(target_id);
    let zone_change =
        lower_targeted_zone_change_node(&mut cursor, &node.zone_change, target.clone())?;
    let life = lower_targetless_local_effect(&node.life, TargetlessDeclarationSupport::None)
        .map_err(|error| cursor.fail(error.expectation()))?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    target_id,
                    candidates,
                    minimum,
                    maximum,
                )],
            },
            "effects": [zone_change, life],
        }),
        &[
            "Recognize the target selector and zone change through shared child automata",
            "Sequence the sentence boundary as Then",
            "Recognize the controller life change and numeric expression independently",
            "Lower to independent zone-change and life-total primitives",
            &trace,
        ],
    ))
}
