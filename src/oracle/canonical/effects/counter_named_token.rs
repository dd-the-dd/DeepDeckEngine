use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a counter followed by a linked token-creation sentence. The claim
/// stops at the stable two-effect boundary; target, anaphora, quantity, token
/// registry, reminder, and full consumption remain child-machine decisions.
pub(in crate::oracle::canonical) fn claims_promoted_counter_named_token_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 10
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.windows(4).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("its")
                && window[3].raw.eq_ignore_ascii_case("creates")
        })
}

pub(in crate::oracle::canonical) fn lower_counter_spell_target(
    cursor: &OracleTokenCursor<'_>,
    counter: &CounterEffectNode,
) -> MachineResult<(&'static str, Value, i64, i64)> {
    let slot = counter
        .target
        .slots
        .first()
        .ok_or_else(|| cursor.fail("one counter target slot"))?;
    cursor.require(
        counter.target.slots.len() == 1,
        "exactly one counter target slot",
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
        TargetSelector::StackItem(_)
        | TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_) => {
            return Err(cursor.fail("a spell target for linked controller resolution"));
        }
    }
    Ok((
        "targetSpell",
        candidates,
        i64::from(slot.cardinality.minimum),
        i64::from(slot.cardinality.maximum),
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_named_token(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-named-token-sequence", input.tokens);
    let sequence = recognize_counter_named_token_sequence(&mut cursor)?;
    cursor.enter("sequence.counter-named-token.validation");
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    cursor.require(
        sequence.token_creation.player == LocalPlayerReference::ControllerOf(binding),
        "the counter target's controller as token controller",
    )?;
    let (target_id, candidates, minimum, maximum) =
        lower_counter_spell_target(&cursor, &sequence.counter)?;
    let target = chosen_target(target_id);
    let controller_binding_id = "counteredSpellController";
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
                {
                    "kind": "counterSpell",
                    "spell": target,
                },
                {
                    "kind": "createTokens",
                    "controller": { "kind": "boundValue", "id": controller_binding_id },
                    "quantity": lower_numeric_atom(sequence.token_creation.quantity),
                    "token": {
                        "kind": "namedToken",
                        "name": sequence.token_creation.token_name,
                    },
                },
            ],
        }),
        &[
            "Recognize counter selection and named-token creation as independent effects",
            "Resolve `its controller` through the prior target binding",
            "Validate the named token and its optional canonical reminder",
            "Snapshot the target controller before attempting to counter the spell",
            "Lower to bind, counterSpell, and createTokens primitives",
            &trace,
        ],
    ))
}
