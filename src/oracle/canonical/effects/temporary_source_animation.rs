use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_temporary_source_animation_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(separator) = tokens.iter().position(|token| token.raw == ":") else {
        return false;
    };
    let effect_start = separator + 1;
    let source_start = if tokens
        .get(effect_start)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("until"))
    {
        tokens
            .iter()
            .enumerate()
            .skip(effect_start)
            .find_map(|(index, token)| (token.raw == ",").then_some(index + 1))
            .unwrap_or(effect_start)
    } else {
        effect_start
    };
    tokens
        .first()
        .is_some_and(|token| token.token_type == "manaSymbol")
        && source_reference_starts_at(
            source_start,
            tokens.get(source_start).map(|token| token.raw.as_str()),
            input.name_links,
            input.name_resolution_failure,
        )
        && tokens
            .iter()
            .skip(source_start)
            .any(|token| token.raw.eq_ignore_ascii_case("becomes"))
        && tokens
            .iter()
            .skip(source_start)
            .any(|token| token.raw.eq_ignore_ascii_case("creature"))
        && tokens
            .iter()
            .skip(source_start)
            .any(|token| token.raw.eq_ignore_ascii_case("still"))
}

fn lower_animation_duration(duration: Option<&EffectDurationNode>) -> Value {
    match duration {
        None => json!({ "kind": "permanent" }),
        Some(EffectDurationNode::UntilEndOfCurrentTurn) => {
            json!({ "kind": "untilEndOfCurrentTurn" })
        }
        Some(EffectDurationNode::UntilNextTurn { player }) => json!({
            "kind": "untilNextTurn",
            "player": match player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_) => {
                    unreachable!("source animation duration cannot require a target binding")
                }
            },
        }),
        Some(
            EffectDurationNode::UntilEndOfNextTurn { .. }
            | EffectDurationNode::WhileSourceControlled,
        ) => {
            unreachable!("source animation reads only the continuous-effect durations")
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_temporary_source_animation(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("temporary-source-animation-activation", input.tokens);
    let ability = recognize_activated_effect_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;

    cursor.enter("activation.temporary-animation.effect-shape");
    let [LocalEffectNode::AnimateSource(animation)] = ability.effects.effects.as_slice() else {
        return Err(cursor.fail("one temporary source-animation effect"));
    };
    cursor.require(
        ability.effects.connectors.is_empty(),
        "one atomic recognized animation effect",
    )?;
    cursor.require(
        animation.source == LocalObjectReference::SourceSelf,
        "the current source as animated object",
    )?;
    cursor.require(
        !animation.retained_card_types.is_empty(),
        "one or more explicitly retained card types",
    )?;

    let costs = ability
        .costs
        .costs
        .iter()
        .flat_map(lower_local_activation_costs)
        .collect::<Vec<_>>();
    let duration = lower_animation_duration(animation.duration.as_ref());
    let mut effects = vec![json!({
        "kind": "becomeCreature",
        "object": self_ref(),
        "addTypes": animation
            .card_types
            .iter()
            .map(|card_type| card_type.ir_name())
            .collect::<Vec<_>>(),
        "addSubtypes": animation
            .subtypes
            .iter()
            .map(|subtype| subtype.ir_name())
            .collect::<Vec<_>>(),
        "addColors": animation
            .colors
            .colors
            .iter()
            .map(|color| color.ir_symbol())
            .collect::<Vec<_>>(),
        "basePower": integer(animation.power_toughness.power),
        "baseToughness": integer(animation.power_toughness.toughness),
        "retainExistingTypes": true,
        "duration": duration.clone(),
    })];
    effects.extend(animation.keywords.iter().map(|keyword| {
        json!({
            "kind": "grantKeyword",
            "object": self_ref(),
            "keyword": keyword.canonical_name(),
            "duration": duration.clone(),
        })
    }));
    if animation.all_creature_types {
        effects.push(json!({
            "kind": "modifySubtypeSet",
            "object": self_ref(),
            "subtypeClass": "creature",
            "mode": "all",
            "duration": duration.clone(),
        }));
    }

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        with_activation_object_cost_decisions(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": effects,
            }),
            &ability.costs,
        ),
        &[
            "Compose an activation cost sequence with one atomic animation node",
            "Recognize optional duration, power/toughness, colors, subtypes, types, and characteristics as typed children",
            "Preserve the source's explicitly retained card type",
            "Lower animation, keyword grants, and subtype-set changes as independent primitive effects",
            &trace,
        ],
    ))
}
