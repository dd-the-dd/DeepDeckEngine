use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the targeted-player compound-exile envelope. The boundary is
/// deliberately structural: the local automaton remains responsible for the
/// player selector, controlled-permanent phrase, connector, possessive zone,
/// punctuation, and complete consumption.
pub(in crate::oracle::canonical) fn claims_promoted_player_compound_exile_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.first().is_some_and(|token| {
        token.token_type == "targetMarker" && token.raw.eq_ignore_ascii_case("target")
    }) && tokens.get(1).is_some_and(|token| {
        matches!(
            token.token_type.as_str(),
            "playerQualifierWord" | "typeWord"
        ) && (token.raw.eq_ignore_ascii_case("player")
            || token.raw.eq_ignore_ascii_case("opponent"))
    }) && tokens.get(2).is_some_and(|token| {
        token.token_type == "actionWord" && token.raw.eq_ignore_ascii_case("exiles")
    }) && tokens.windows(2).any(|pair| {
        pair[0].raw.eq_ignore_ascii_case("they") && pair[1].raw.eq_ignore_ascii_case("control")
    }) && tokens
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("their"))
}

pub(in crate::oracle::canonical) fn parse_promoted_player_compound_exile(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("player-compound-exile", input.tokens);
    let compound = recognize_player_compound_exile(&mut cursor)?;
    cursor.enter("compound-exile.validation");
    cursor.require(
        compound.connector == CompoundObjectConnector::And,
        "an `and` compound-object connector",
    )?;
    cursor.require(
        compound.permanent.player == compound.subject_binding,
        "the controlled permanent bound to the targeted player",
    )?;
    cursor.require(
        compound.zone.player == compound.subject_binding,
        "the possessive zone bound to the targeted player",
    )?;
    cursor.require(
        compound.zone.zone == PlayerZoneKind::Graveyard,
        "the targeted player's graveyard",
    )?;

    let (target_id, target_candidates, target_minimum, target_maximum) =
        lower_single_target(&compound.subject);
    let player = chosen_target(target_id);
    let permanent_filter = lower_permanent_object_predicate(&compound.permanent.predicate);
    let choice_id = "controlledPermanentToExile";
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
                    target_candidates,
                    target_minimum,
                    target_maximum,
                )],
            },
            "effects": [
                {
                    "kind": "choosePermanents",
                    "id": choice_id,
                    "player": player.clone(),
                    "minimum": minimum(vec![
                        integer(1),
                        json!({
                            "kind": "countPermanents",
                            "player": player.clone(),
                            "where": permanent_filter.clone(),
                        }),
                    ]),
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "controller": player.clone(),
                        "where": permanent_filter,
                    },
                },
                {
                    "kind": "exileTogether",
                    "objects": {
                        "kind": "union",
                        "sets": [
                            decision_result(choice_id),
                            {
                                "kind": "cardsInZone",
                                "zone": graveyard(player),
                            },
                        ],
                    },
                },
            ],
        }),
        &[
            "Recognize one targeted-player subject",
            "Bind `they` and `their` to that target",
            "Parse `and` as a compound-object connector",
            "Lower the controlled-permanent choice and graveyard as one simultaneous exile set",
            &trace,
        ],
    ))
}

/// Claims the promoted inter-sentence exile/anaphora grammar. The boundary
/// only identifies the family; local automata still own every transition and
/// preserve committed failures for malformed target, reference, or numeric
/// expressions.
pub(in crate::oracle::canonical) fn claims_promoted_exile_anaphora_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(boundary) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("exile"))
        && tokens.get(boundary + 1..boundary + 5).is_some_and(|head| {
            head[0].raw.eq_ignore_ascii_case("its")
                && head[1].raw.eq_ignore_ascii_case("controller")
                && is_verb_form(&head[2].raw, VerbLemma::Gain, VerbForm::ThirdPersonSingular)
                && head[3].raw.eq_ignore_ascii_case("life")
        })
}

fn lower_exile_anaphora_sequence(
    zone_change: &ZoneChangeEffectNode,
    transitions: &[MachineTransition],
) -> CanonicalRuleDraft {
    let (target_id, candidates, minimum, maximum) = lower_single_target(&zone_change.target);
    let target = chosen_target(target_id);
    let controller_binding_id = "exiledPermanentController";
    let power_binding_id = "exiledPermanentPower";
    // The caller validated the controller and power of the prior target.
    let player = json!({ "kind": "boundValue", "id": controller_binding_id });
    let amount = json!({ "kind": "boundValue", "id": power_binding_id });
    let trace = transitions
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    draft(
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
                {
                    "kind": "bind",
                    "id": power_binding_id,
                    "value": {
                        "kind": "maximum",
                        "operands": [
                            { "kind": "powerOf", "object": target.clone() },
                            integer(0),
                        ],
                    },
                },
                {
                    "kind": "exilePermanent",
                    "permanent": target,
                },
                {
                    "kind": "gainLife",
                    "player": player,
                    "amount": amount,
                },
            ],
        }),
        &[
            "Recognize exile and life gain as independent local effects",
            "Resolve both `its` references through the prior target binding",
            "Snapshot controller and nonnegative power before the zone change",
            "Lower to bind, exilePermanent, and gainLife primitives",
            &trace,
        ],
    )
}

pub(in crate::oracle::canonical) fn parse_promoted_exile_anaphora(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("exile-anaphora-sequence", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.exile-anaphora.validation");
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    // `its controller gains life` is read by the shared player-action child
    // as an object-related subject; the established life reader keeps the
    // equivalent local controller reference.
    let (zone_change, controller_of_target, amount) = match sequence.effects.as_slice() {
        [
            LocalEffectNode::ZoneChange(zone_change),
            LocalEffectNode::GainLife(gain),
        ] => (
            zone_change,
            gain.player == LocalPlayerReference::ControllerOf(binding),
            &gain.amount,
        ),
        [
            LocalEffectNode::ZoneChange(zone_change),
            LocalEffectNode::PlayerAction(PlayerActionNode {
                player,
                action:
                    SingularPlayerAction::Life(LifeActionNode {
                        action: TargetPlayerLifeAction::Gain,
                        amount,
                    }),
            }),
        ] => (
            zone_change,
            *player
                == PlayerEffectSubject::ObjectRelation {
                    relation: ObjectPlayerRelation::Controller,
                    object: PermanentEffectReference::Target(binding),
                },
            amount,
        ),
        _ => {
            return Err(cursor.fail("an exile effect followed by one linked life-gain effect"));
        }
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "one inter-sentence effect sequence",
    )?;
    cursor.require(
        zone_change.action == ZoneChangeAction::Exile,
        "an exile zone change",
    )?;
    cursor.require(controller_of_target, "the controller of the prior target")?;
    cursor.require(
        amount.is_target_property(binding, SelectorNumericProperty::Power),
        "the power of the prior target",
    )?;
    Ok(lower_exile_anaphora_sequence(
        zone_change,
        cursor.transitions(),
    ))
}
