use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_effect_sequence_modal_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let literal_cardinality = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("choose"))
        && tokens
            .get(1)
            .and_then(|token| parse_number_word(&token.raw))
            .is_some_and(|count| count > 0)
        && (tokens.get(2).is_some_and(|token| token.raw == "\u{2014}")
            || (tokens
                .get(2)
                .is_some_and(|token| token.raw.eq_ignore_ascii_case("or"))
                && tokens.get(3).is_some_and(|token| {
                    token.raw.eq_ignore_ascii_case("both") || token.raw.eq_ignore_ascii_case("more")
                })
                && tokens.get(4).is_some_and(|token| token.raw == "\u{2014}")));
    if !literal_cardinality {
        return false;
    }
    tokens
        .iter()
        .filter(|token| token.raw == "\u{2022}")
        .count()
        > 0
}

pub(in crate::oracle::canonical) fn parse_promoted_effect_sequence_modal(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor =
        OracleTokenCursor::new("exact-cardinality-effect-sequence-modal", input.tokens);
    let modal = recognize_effect_sequence_modal(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    let lowered = lower_effect_sequence_modal(&mut cursor, &modal)?;
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": lowered.decisions,
            },
            "effects": lowered.effects,
        }),
        &[
            "Recognize exact modal cardinality and bullet boundaries from tokens",
            "Delegate every printed mode to the shared effect-sequence automaton",
            "Scope only mode-local declarations to their selected mode",
            "Preserve printed mode and effect order during lowering",
        ],
    ))
}

/// Shared modal lowering (CR 700.2): one `chooseModes` declaration, each mode's
/// own declarations scoped to its selection, and one conditional per mode.
/// Spells and triggered abilities (modes chosen as the trigger is put on the
/// stack, CR 700.2b) reuse it unchanged.
pub(in crate::oracle::canonical) fn lower_effect_sequence_modal(
    cursor: &mut OracleTokenCursor<'_>,
    modal: &EffectSequenceModalNode,
) -> MachineResult<LoweredEffectSequence> {
    let mode_ids = (1..=modal.modes.len())
        .map(|index| format!("mode{index}"))
        .collect::<Vec<_>>();
    let mut lowered_modes = Vec::with_capacity(modal.modes.len());
    for (index, mode) in modal.modes.iter().enumerate() {
        let mode_id = format!("mode{}", index + 1);
        let selected = selection("chosenModes", &mode_id);
        let mut lowered = lower_local_effect_sequence(cursor, mode)?;
        // Declarations and resolution cost results are local to their mode:
        // two selected modes must not share one `tryPayCost` result id.
        let replacements = lowered
            .decisions
            .iter()
            .filter_map(|decision| decision["id"].as_str())
            .filter(|id| *id != "xValue")
            .chain(
                lowered
                    .effects
                    .iter()
                    .filter(|effect| effect["kind"].as_str() == Some("tryPayCost"))
                    .filter_map(|effect| effect["id"].as_str()),
            )
            .map(|id| (id.to_string(), format!("{mode_id}:{id}")))
            .collect::<Vec<_>>();
        for decision in &mut lowered.decisions {
            let is_shared_x = decision["id"].as_str() == Some("xValue");
            replace_decision_ids(decision, &replacements);
            if is_shared_x {
                if let Some(object) = decision.as_object_mut() {
                    object.remove("condition");
                }
            } else {
                decision["condition"] = if let Some(existing) = decision.get("condition") {
                    and(vec![selected.clone(), existing.clone()])
                } else {
                    selected.clone()
                };
            }
        }
        for effect in &mut lowered.effects {
            replace_decision_ids(effect, &replacements);
        }
        lowered_modes.push((selected, lowered));
    }

    let mut decisions = vec![json!({
        "id": "chosenModes",
        "kind": "chooseModes",
        "minimum": modal.cardinality.minimum,
        "maximum": integer(i64::from(modal.cardinality.maximum)),
        "options": mode_ids,
        "allowRepeated": false,
    })];
    let mut effects = Vec::with_capacity(lowered_modes.len());
    for (selected, lowered) in lowered_modes {
        decisions.extend(lowered.decisions);
        effects.push(json!({
            "kind": "conditional",
            "condition": selected,
            "then": lowered.effects,
        }));
    }
    Ok(LoweredEffectSequence { effects, decisions })
}

fn modal_token_is(token: &crate::oracle::model::RecognizedEntity, expected: &str) -> bool {
    token.raw.eq_ignore_ascii_case(expected)
}

fn claims_targeted_token_mode(mode: &[crate::oracle::model::RecognizedEntity]) -> bool {
    matches!(mode, [target, player, action, quantity, statistics, color, ..]
        if modal_token_is(target, "target")
            && (modal_token_is(player, "player") || modal_token_is(player, "opponent"))
            && (modal_token_is(action, "create") || modal_token_is(action, "creates"))
            && (quantity.token_type == "quantity"
                || modal_token_is(quantity, "a")
                || modal_token_is(quantity, "an"))
            && statistics.token_type == "counterValue"
            && color.token_type == "colorWord")
}

fn claims_targeted_scry_draw_mode(mode: &[crate::oracle::model::RecognizedEntity]) -> bool {
    matches!(mode, [target, player, scries, ..]
        if modal_token_is(target, "target")
            && (modal_token_is(player, "player") || modal_token_is(player, "opponent"))
            && modal_token_is(scries, "scries"))
}

fn claims_graveyard_card_exile_mode(mode: &[crate::oracle::model::RecognizedEntity]) -> bool {
    matches!(mode, [exile, target, cards, from, ..]
        if modal_token_is(exile, "exile")
            && modal_token_is(target, "target")
            && (modal_token_is(cards, "card") || modal_token_is(cards, "cards"))
            && modal_token_is(from, "from"))
        || matches!(mode, [exile, count, target, cards, from, ..]
            if modal_token_is(exile, "exile")
                && count.token_type == "quantity"
                && modal_token_is(target, "target")
                && (modal_token_is(cards, "card") || modal_token_is(cards, "cards"))
                && modal_token_is(from, "from"))
        || matches!(mode, [exile, up, to, count, target, cards, from, ..]
            if modal_token_is(exile, "exile")
                && modal_token_is(up, "up")
                && modal_token_is(to, "to")
                && count.token_type == "quantity"
                && modal_token_is(target, "target")
                && (modal_token_is(cards, "card") || modal_token_is(cards, "cards"))
                && modal_token_is(from, "from"))
}

fn claims_battlefield_exile_mode(mode: &[crate::oracle::model::RecognizedEntity]) -> bool {
    matches!(mode, [exile, target, ..]
        if modal_token_is(exile, "exile") && modal_token_is(target, "target"))
        && !mode.iter().any(|token| modal_token_is(token, "then"))
}

fn claims_damage_mode(
    mode: &[crate::oracle::model::RecognizedEntity],
    absolute_start: usize,
    input: &AbilityInput<'_>,
) -> bool {
    let action_offset = input
        .name_links
        .iter()
        .find(|link| link.token_start == absolute_start)
        .map(|link| link.token_end.saturating_sub(absolute_start))
        .or_else(|| {
            matches!(mode, [this, source, ..]
                if modal_token_is(this, "this")
                    && matches!(source.raw.to_ascii_lowercase().as_str(), "spell" | "source" | "creature"))
            .then_some(2)
        });
    action_offset
        .and_then(|offset| mode.get(offset))
        .is_some_and(|action| modal_token_is(action, "deal") || modal_token_is(action, "deals"))
        && !mode.iter().any(|token| modal_token_is(token, "then"))
}

fn claims_keyword_grant_mode(mode: &[crate::oracle::model::RecognizedEntity]) -> bool {
    let supported_recipient_prefix = mode
        .first()
        .is_some_and(|token| modal_token_is(token, "target") || token.token_type == "typeWord");
    let first_action_is_grant = mode
        .iter()
        .find(|token| token.token_type == "actionWord")
        .is_some_and(|action| modal_token_is(action, "gain") || modal_token_is(action, "gains"));
    supported_recipient_prefix
        && first_action_is_grant
        && !mode.iter().any(|token| modal_token_is(token, "then"))
}

/// Claims the promoted family structurally. Every bullet must have a family
/// prefix owned by one promoted child before the parent commits to the modal
/// automaton. This prevents one recognized Kozilek mode from pulling unrelated
/// modal cards out of the legacy path.
pub(in crate::oracle::canonical) fn claims_promoted_primitive_modal_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let exact_header = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("choose"))
        && tokens.get(1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("one") || token.raw.eq_ignore_ascii_case("two")
        })
        && tokens
            .iter()
            .filter(|token| token.raw == "\u{2022}")
            .count()
            >= 2;
    if !exact_header {
        return false;
    }
    let bullets = tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| (token.raw == "\u{2022}").then_some(index))
        .collect::<Vec<_>>();
    bullets.iter().enumerate().all(|(mode_index, bullet)| {
        let start = bullet + 1;
        let end = bullets.get(mode_index + 1).copied().unwrap_or(tokens.len());
        let mode = &tokens[start..end];
        let starts_targeted_player_child =
            claims_targeted_token_mode(mode) || claims_targeted_scry_draw_mode(mode);
        let starts_zone_change =
            claims_battlefield_exile_mode(mode) || claims_graveyard_card_exile_mode(mode);
        claims_damage_mode(mode, start, input)
            || starts_targeted_player_child
            || starts_zone_change
            || claims_keyword_grant_mode(mode)
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_primitive_modal(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("exact-cardinality-primitive-modal", input.tokens);
    let modal =
        recognize_primitive_modal(&mut cursor, input.name_links, input.name_resolution_failure)?;
    let mode_ids = (1..=modal.modes.len())
        .map(|index| format!("mode{index}"))
        .collect::<Vec<_>>();
    let uses_x = modal.modes.iter().any(ModalPrimitiveActionNode::uses_x);
    let mut decisions = vec![json!({
        "id": "chosenModes",
        "kind": "chooseModes",
        "minimum": modal.cardinality.minimum,
        "maximum": integer(i64::from(modal.cardinality.maximum)),
        "options": mode_ids,
        "allowRepeated": modal.cardinality.allow_repeated,
    })];
    if uses_x {
        decisions.push(x_value());
    }
    let mut effects = Vec::with_capacity(modal.modes.len());

    for (index, mode) in modal.modes.into_iter().enumerate() {
        let mode_id = format!("mode{}", index + 1);
        let selected = selection("chosenModes", &mode_id);
        let mode_effects = match mode {
            ModalPrimitiveActionNode::Damage(damage) => {
                let target_id = format!("{mode_id}:targetDamageable");
                let mut lowered = lower_targeted_damage_effect(&damage, &target_id)
                    .ok_or_else(|| cursor.fail("a targeted damage modal action"))?;
                lowered.decision["condition"] = selected.clone();
                decisions.push(lowered.decision);
                vec![lowered.effect]
            }
            ModalPrimitiveActionNode::TemporaryKeywordGrant(grant) => {
                let target_id = format!("{mode_id}:targetPermanent");
                let mut lowered = lower_temporary_keyword_grant(grant, &target_id)
                    .ok_or_else(|| cursor.fail("a permanent temporary-keyword grant"))?;
                if let Some(mut decision) = lowered.decision.take() {
                    decision["condition"] = selected.clone();
                    decisions.push(decision);
                }
                lowered.effects
            }
            ModalPrimitiveActionNode::TargetedTokenCreation(creation) => {
                let target_id = format!("{mode_id}:targetPlayer");
                let (_, candidates, minimum, maximum) = lower_single_target(&creation.target);
                let mut decision = target_decision(&target_id, candidates, minimum, maximum);
                decision["condition"] = selected.clone();
                decisions.push(decision);
                let ability = ActivatedManaAbilityNode {
                    costs: creation.ability.costs,
                    effect: creation.ability.mana,
                    followup: None,
                };
                let abilities = [
                    match lower_recognized_add_led_activation(&mut cursor, ability) {
                        EffectProgramParseAttempt::Parsed(draft) => draft.rule,
                        EffectProgramParseAttempt::Unpromoted(failure)
                        | EffectProgramParseAttempt::Failed(failure)
                        | EffectProgramParseAttempt::FailedSupport(failure) => return Err(failure),
                        EffectProgramParseAttempt::NotApplicable => {
                            unreachable!(
                                "an embedded token activation already has a structural context"
                            )
                        }
                    },
                ];
                vec![json!({
                    "kind": "createTokens",
                    "controller": chosen_target(&target_id),
                    "quantity": lower_numeric_atom(creation.quantity),
                    "token": {
                        "colors": creation
                            .token
                            .colors
                            .iter()
                            .map(|color| canonical_color_name(*color))
                            .collect::<Vec<_>>(),
                        "types": ["Creature"],
                        "subtypes": creation
                            .token
                            .subtypes
                            .iter()
                            .map(|subtype| subtype.ir_name())
                            .collect::<Vec<_>>(),
                        "power": creation.token.power,
                        "toughness": creation.token.toughness,
                        "abilities": abilities,
                    },
                })]
            }
            ModalPrimitiveActionNode::TargetPlayerScryThenDraw(sequence) => {
                let target_id = format!("{mode_id}:targetPlayer");
                let (_, candidates, minimum, maximum) = lower_single_target(&sequence.target);
                let mut decision = target_decision(&target_id, candidates, minimum, maximum);
                decision["condition"] = selected.clone();
                decisions.push(decision);
                let expected_binding = TargetBinding {
                    effect_index: 0,
                    slot_index: 0,
                };
                cursor.require(
                    sequence.target_binding == expected_binding,
                    "the retained target-player binding for scry and draw",
                )?;
                let player = chosen_target(&target_id);
                let mut lowered = lower_scry_effects_for_player(
                    &sequence.scry_count,
                    player.clone(),
                    Some(&mode_id),
                )?;
                lowered.push(json!({
                    "kind": "drawCards",
                    "player": player,
                    "count": lower_numeric_expression(&sequence.draw_count)
                        .ok_or_else(|| cursor.fail("a target-independent draw count"))?,
                }));
                lowered
            }
            ModalPrimitiveActionNode::ZoneChange(zone_change) => {
                let target_id = format!("{mode_id}:targetPermanent");
                let (_, candidates, minimum, maximum) = lower_single_target(&zone_change.target);
                let mut decision = target_decision(&target_id, candidates, minimum, maximum);
                decision["condition"] = selected.clone();
                decisions.push(decision);
                vec![lower_targeted_zone_change_node(
                    &mut cursor,
                    &zone_change,
                    chosen_target(&target_id),
                )?]
            }
            ModalPrimitiveActionNode::TargetCardZoneChange(zone_change) => {
                let target_id = format!("{mode_id}:targetCards");
                let mut decision = json!({
                    "id": target_id,
                    "kind": "chooseTargets",
                    "minimum": lower_target_cardinality_bound(zone_change.cardinality.minimum),
                    "maximum": lower_target_cardinality_bound(zone_change.cardinality.maximum),
                    "candidates": {
                        "kind": "cards",
                        "zone": { "kind": "anyGraveyard" },
                        "where": Value::Null,
                    },
                    "condition": selected.clone(),
                });
                if zone_change.source_zone == TargetCardSourceZone::SingleGraveyard
                    && zone_change.cardinality != ExpressedTargetCardinality::EXACTLY_ONE
                {
                    decision["selectionConstraint"] = json!({
                        "kind": "sameZoneOwner",
                        "zone": "graveyard",
                    });
                }
                decisions.push(decision);
                vec![json!({
                    "kind": match zone_change.action {
                        CardZoneChangeAction::Exile => "exileTargetCards",
                        CardZoneChangeAction::Move(_) => {
                            return Err(cursor.fail("an exile action in a graveyard-exile mode"));
                        }
                    },
                    "cards": { "kind": "chosenTargets", "id": target_id },
                })]
            }
        };
        effects.push(json!({
            "kind": "conditional",
            "condition": selected,
            "then": mode_effects,
        }));
    }
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
                "decisions": decisions,
            },
            "effects": effects,
        }),
        &[
            "Recognize exact-cardinality modality and bullet boundaries in the modal parent",
            "Delegate direct damage to source, numeric, and target-selector children",
            "Delegate both targeted and controller-scoped grants to one keyword-grant child",
            "Delegate targeted token creation, targeted scry/draw, and zone changes to typed children",
            "Lower one chooseModes decision and independent conditional primitive effects",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "modal_spell_tests.rs"]
mod tests;
