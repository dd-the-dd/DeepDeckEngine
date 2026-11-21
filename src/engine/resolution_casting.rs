//! Casting permission during resolution uses the normal declaration and cost
//! boundaries, with predicates evaluated against the resulting spell face.

use super::*;

fn resulting_spell_matches(
    definition: &CardDefinition,
    decisions: &BTreeMap<String, Value>,
    predicate: &Value,
) -> bool {
    match value_kind(predicate) {
        Some("compare")
            if value_kind(&predicate["left"]) == Some("manaValueOf")
                && value_kind(&predicate["left"]["object"]) == Some("candidate") =>
        {
            let Some(right) =
                integer_value(&predicate["right"]).and_then(|value| i32::try_from(value).ok())
            else {
                return false;
            };
            object_predicate::compare_integers(
                mana_value_with_decisions(definition, decisions),
                predicate["operator"].as_str(),
                right,
            )
        }
        Some("and") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .all(|operand| resulting_spell_matches(definition, decisions, operand))
        }),
        Some("or") => predicate["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .any(|operand| resulting_spell_matches(definition, decisions, operand))
        }),
        Some("not") => !resulting_spell_matches(definition, decisions, &predicate["operand"]),
        _ => matches_stack_filter(definition, predicate),
    }
}

impl GameEngine {
    pub(super) fn cast_one_card_actions(
        &self,
        player_index: usize,
        cards: &[CardInstance],
        source_zone: &str,
        normal_costs_apply: bool,
        spell_where: Option<&Value>,
    ) -> Result<Vec<LegalAction>, EngineError> {
        let player_id = &self.state.players[player_index].id;
        let mut actions = Vec::new();
        for original in cards {
            // This is the same face preparation used by normal priority casts.
            // The printed card remains in its zone until apply_priority_action
            // applies the selected face to the original physical instance.
            let faces = split_face_definitions(&original.definition);
            let mut variants = if faces.is_empty() {
                vec![(original.clone(), None, false)]
            } else {
                faces
                    .into_iter()
                    .enumerate()
                    .map(|(index, definition)| {
                        let mut casting_card = original.clone();
                        casting_card.definition = definition;
                        (casting_card, Some(index), false)
                    })
                    .collect::<Vec<_>>()
            };
            if source_zone == "hand"
                && let Some(definition) = fused_split_definition(&original.definition)
            {
                let mut casting_card = original.clone();
                casting_card.definition = definition;
                variants.push((casting_card, None, true));
            }
            for (casting_card, face_index, fused) in variants {
                if is_land(&casting_card.definition)
                    || self.card_casting_prohibited(player_id, &casting_card.definition)
                    || self.spell_casting_prohibited(player_id)
                    || self.spell_cast_from_zone_prohibited(
                        player_id,
                        source_zone,
                        &casting_card.definition,
                    )
                {
                    continue;
                }
                for mut action in self.cast_actions_from_zone(
                    player_index,
                    std::slice::from_ref(&casting_card),
                    "duringResolution",
                    source_zone,
                    normal_costs_apply,
                )? {
                    if spell_where.is_some_and(|predicate| {
                        !resulting_spell_matches(
                            &casting_card.definition,
                            &action.decisions,
                            predicate,
                        )
                    }) {
                        continue;
                    }
                    if let Some(face_index) = face_index {
                        action
                            .decisions
                            .insert("splitFaceIndex".to_string(), Value::from(face_index));
                        action.id = format!("{}:face:{face_index}", action.id);
                    }
                    if fused {
                        action
                            .decisions
                            .insert("fused".to_string(), Value::Bool(true));
                        action.id = format!("{}:fused", action.id);
                    }
                    actions.push(action);
                }
            }
        }
        Ok(actions)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "resolution_casting_tests.rs"]
mod tests;
