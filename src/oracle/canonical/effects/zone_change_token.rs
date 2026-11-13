use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a targeted battlefield zone change immediately followed by a
/// controller-bound creature or catalog-named token creation. The child automata own
/// targets, anaphora, quantities, characteristics, and full consumption.
pub(in crate::oracle::canonical) fn claims_promoted_zone_change_token_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.first().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy") || token.raw.eq_ignore_ascii_case("exile")
    }) && tokens.windows(4).any(|window| {
        window[0].raw == "."
            && window[2].raw.eq_ignore_ascii_case("controller")
            && window[3].raw.eq_ignore_ascii_case("creates")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_zone_change_token(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("zone-change-create-token", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.zone-change-token.validation");
    let [LocalEffectNode::ZoneChange(zone_change), creation] = sequence.effects.as_slice() else {
        return Err(cursor.fail("a zone change followed by creature-token creation"));
    };
    let (player, quantity, token) = match creation {
        LocalEffectNode::CreateTokens(create_tokens) => (
            &create_tokens.player,
            create_tokens.quantity.clone(),
            json!({
                "colors": create_tokens
                    .token
                    .colors
                    .iter()
                    .map(|color| canonical_color_name(*color))
                    .collect::<Vec<_>>(),
                "types": ["Creature"],
                "subtypes": create_tokens
                    .token
                    .subtypes
                    .iter()
                    .map(|subtype| subtype.ir_name())
                    .collect::<Vec<_>>(),
                "power": create_tokens.token.power,
                "toughness": create_tokens.token.toughness,
                "abilities": [],
            }),
        ),
        LocalEffectNode::CreateNamedTokens(create_tokens) => (
            &create_tokens.player,
            create_tokens.quantity.clone(),
            json!({ "kind": "namedToken", "name": create_tokens.token_name }),
        ),
        _ => return Err(cursor.fail("a zone change followed by token creation")),
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
        *player == LocalPlayerReference::ControllerOf(binding),
        "the prior target's controller as token controller",
    )?;

    let (target_id, candidates, minimum, maximum) = lower_single_target(&zone_change.target);
    let target = chosen_target(target_id);
    let controller_binding_id = "changedPermanentController";
    let zone_change_effect =
        lower_targeted_zone_change_node(&mut cursor, zone_change, target.clone())?;
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
            "effects": [
                {
                    "kind": "bind",
                    "id": controller_binding_id,
                    "value": { "kind": "controllerOf", "object": target.clone() },
                },
                zone_change_effect,
                {
                    "kind": "createTokens",
                    "controller": { "kind": "boundValue", "id": controller_binding_id },
                    "quantity": lower_numeric_atom(quantity),
                    "token": token,
                },
            ],
        }),
        &[
            "Recognize one targeted zone change and one controller-bound token creation",
            "Resolve the token controller through the prior target binding",
            "Snapshot the controller before the permanent leaves the battlefield",
            "Lower binding, zone change, and token creation as independent primitives",
            &trace,
        ],
    ))
}
