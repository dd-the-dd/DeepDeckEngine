use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a choose-one modal family whose bullets use explicit target-color
/// guards. Child automata own mode actions, targets, colors, punctuation, and
/// full consumption after this structural boundary commits.
pub(in crate::oracle::canonical) fn claims_promoted_color_guarded_modal_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 14
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("choose"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("one"))
        && tokens.iter().filter(|token| token.raw == "•").count() >= 2
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("if")
                && window[1].raw.eq_ignore_ascii_case("it's")
                && window[2].token_type == "colorWord"
        })
}

fn lower_modal_counter_target(
    cursor: &OracleTokenCursor<'_>,
    effect: &CounterEffectNode,
    target_id: &str,
) -> MachineResult<(Value, Value, i64, i64)> {
    let slot = effect
        .target
        .slots
        .first()
        .ok_or_else(|| cursor.fail("one modal counter target"))?;
    cursor.require(
        effect.target.slots.len() == 1,
        "exactly one modal counter target",
    )?;
    let mut candidates = json!({ "kind": "spells" });
    match &slot.selector {
        TargetSelector::Object(predicate) => {
            if let Some(where_predicate) = lower_spell_object_predicate(predicate) {
                candidates["where"] = where_predicate;
            }
        }
        TargetSelector::ControlledObject {
            predicate,
            controller: controller_scope,
        } => {
            if let Some(where_predicate) = lower_spell_object_predicate(predicate) {
                candidates["where"] = where_predicate;
            }
            candidates["controller"] = lower_object_controller_scope(*controller_scope);
        }
        TargetSelector::StackItem(predicate) => {
            cursor.require(
                predicate == &StackItemPredicate::Spell,
                "a spell target for the counter mode",
            )?;
        }
        TargetSelector::Any | TargetSelector::Union(_) | TargetSelector::Player(_) => {
            return Err(cursor.fail("a spell target for the counter mode"));
        }
    }
    let target = chosen_target(target_id);
    Ok((
        candidates,
        json!({ "kind": "counterStackObject", "object": target }),
        i64::from(slot.cardinality.minimum),
        i64::from(slot.cardinality.maximum),
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_color_guarded_modal(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("color-guarded-choose-one-modal", input.tokens);
    let modal = recognize_choose_one_color_guarded_modal(&mut cursor)?;
    let mut mode_ids = Vec::with_capacity(modal.modes.len());
    let mut decisions = Vec::with_capacity(1 + modal.modes.len());
    let mut effects = Vec::with_capacity(modal.modes.len());

    for (index, mode) in modal.modes.iter().enumerate() {
        let mode_id = format!("mode{}", index + 1);
        let selected = selection("chosenModes", &mode_id);
        let (target_id, candidates, minimum, maximum, operation) = match &mode.action {
            ColorGuardedModalActionNode::CounterSpell(effect) => {
                let target_id = format!("{mode_id}:targetStackObject");
                let (candidates, operation, minimum, maximum) =
                    lower_modal_counter_target(&cursor, effect, &target_id)?;
                (target_id, candidates, minimum, maximum, operation)
            }
            ColorGuardedModalActionNode::DestroyPermanent(effect) => {
                let (base_id, candidates, minimum, maximum) = lower_single_target(&effect.target);
                let target_id = format!("{mode_id}:{base_id}");
                (
                    target_id.clone(),
                    candidates,
                    minimum,
                    maximum,
                    json!({
                        "kind": "destroyPermanent",
                        "permanent": chosen_target(&target_id),
                    }),
                )
            }
        };
        let target = chosen_target(&target_id);
        decisions.push(json!({
            "id": target_id,
            "kind": "chooseTargets",
            "condition": selected.clone(),
            "minimum": minimum,
            "maximum": maximum,
            "candidates": candidates,
        }));
        effects.push(json!({
            "kind": "conditional",
            "condition": selected,
            "then": [{
                "kind": "conditional",
                "condition": {
                    "kind": "objectMatchesFilter",
                    "object": target,
                    "where": {
                        "kind": "colorContains",
                        "value": canonical_color_name(mode.required_color),
                    },
                },
                "then": [operation],
            }],
        }));
        mode_ids.push(mode_id);
    }

    decisions.insert(
        0,
        json!({
            "id": "chosenModes",
            "kind": "chooseModes",
            "minimum": 1,
            "maximum": integer(1),
            "options": mode_ids,
            "allowRepeated": false,
        }),
    );
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
            "Recognize a choose-one header and each bullet through local token automata",
            "Delegate counter and destroy targets to shared selector children",
            "Keep target legality separate from the resolution-time color guard",
            "Guard declarations and effects with the selected mode",
            "Lower to chooseModes, chooseTargets, objectMatchesFilter, counter, and destroy primitives",
            &trace,
        ],
    ))
}
