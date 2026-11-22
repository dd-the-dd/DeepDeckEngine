use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims the promoted inter-sentence zone-change/search grammar by its
/// structural envelope. Child automata still own target, reference, filter,
/// destination, and result-anaphora recognition.
pub(in crate::oracle::canonical) fn claims_promoted_zone_change_search_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(boundary) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    tokens.first().is_some_and(|token| {
        is_verb_form(&token.raw, VerbLemma::Exile, VerbForm::Base)
            || token.raw.eq_ignore_ascii_case("destroy")
    }) && tokens.get(boundary + 1..boundary + 7).is_some_and(|head| {
        head[0].raw.eq_ignore_ascii_case("its")
            && head[1].raw.eq_ignore_ascii_case("controller")
            && head[2].raw.eq_ignore_ascii_case("may")
            && head[3].raw.eq_ignore_ascii_case("search")
            && head[4].raw.eq_ignore_ascii_case("their")
            && head[5].raw.eq_ignore_ascii_case("library")
    })
}

pub(in crate::oracle::canonical) fn lower_zone_change_search_components(
    cursor: &mut OracleTokenCursor<'_>,
    zone_change: &ZoneChangeEffectNode,
    search: &LibrarySearchEffectNode,
    binding: TargetBinding,
) -> MachineResult<(Value, Vec<Value>)> {
    let (target_id, candidates, minimum, maximum) = lower_single_target(&zone_change.target);
    let target = chosen_target(target_id);
    let controller_binding_id = "targetController";
    let player = match search.player {
        LocalPlayerReference::ControllerOf(reference) if reference == binding => {
            json!({ "kind": "boundValue", "id": controller_binding_id })
        }
        _ => unreachable!("validated linked target-controller reference"),
    };
    let SearchDestination::Battlefield { .. } = search.destination else {
        unreachable!("linked optional searches are validated as battlefield moves")
    };
    let zone_change_effect = lower_targeted_zone_change_node(cursor, zone_change, target.clone())?;
    let search_effects = lower_library_search_effects(cursor, search, player.clone())?;
    Ok((
        target_decision(target_id, candidates, minimum, maximum),
        vec![
            json!({
                "kind": "bind",
                "id": controller_binding_id,
                "value": { "kind": "controllerOf", "object": target.clone() },
            }),
            zone_change_effect,
            json!({
                "kind": "optionalEffects",
                "player": player,
                "effects": search_effects,
            }),
        ],
    ))
}

fn lower_zone_change_search_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    zone_change: &ZoneChangeEffectNode,
    search: &LibrarySearchEffectNode,
    binding: TargetBinding,
) -> MachineResult<CanonicalRuleDraft> {
    let (decision, effects) =
        lower_zone_change_search_components(cursor, zone_change, search, binding)?;
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
                "decisions": [decision],
            },
            "effects": effects,
        }),
        &[
            "Recognize zone change and optional library search as typed local effects",
            "Resolve the searching player through the prior target binding",
            "Snapshot the target controller before the battlefield zone change",
            "Lower search, move, shuffle, and search record to primitive operations",
            &trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_zone_change_search(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("zone-change-library-search", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.zone-change-search.validation");
    let [
        LocalEffectNode::ZoneChange(zone_change),
        LocalEffectNode::OptionalLibrarySearch(search),
    ] = sequence.effects.as_slice()
    else {
        return Err(cursor.fail("a zone change followed by one optional library search"));
    };
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "one inter-sentence effect sequence",
    )?;
    cursor.require(
        search.player == LocalPlayerReference::ControllerOf(binding),
        "the controller of the prior target as the searching player",
    )?;
    cursor.require(
        matches!(search.destination, SearchDestination::Battlefield { .. }),
        "a battlefield destination for the linked optional search",
    )?;
    lower_zone_change_search_sequence(&mut cursor, zone_change, search, binding)
}
