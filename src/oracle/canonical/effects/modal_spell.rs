use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a spell whose body carries the modal container's structural
/// markers (`choose` and a mode bullet, or `Spree` and a `+ COST` mode).
/// Once claimed, the header and every mode fail explicitly in their own
/// states; nothing falls back.
pub(in crate::oracle::canonical) fn claims_promoted_effect_sequence_modal_family(
    input: &AbilityInput<'_>,
) -> bool {
    let probe = OracleTokenCursor::new("modal-body-probe", input.tokens);
    modal_body_starts(&probe) || spree_body_starts(&probe)
}

/// The Spree container (CR 702.172) is claimed by its keyword and `+ COST`
/// markers before any keyword-line family can read `Spree` alone.
pub(in crate::oracle::canonical) fn claims_promoted_spree_family(input: &AbilityInput<'_>) -> bool {
    spree_body_starts(&OracleTokenCursor::new("spree-body-probe", input.tokens))
}

pub(in crate::oracle::canonical) fn parse_promoted_effect_sequence_modal(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("modal-effect-program", input.tokens);
    let modal = if spree_body_starts(&cursor) {
        recognize_spree_modal(&mut cursor, input.name_links, input.name_resolution_failure)?
    } else {
        recognize_effect_sequence_modal(
            &mut cursor,
            input.name_links,
            input.name_resolution_failure,
        )?
    };
    let lowered = lower_effect_sequence_modal(&mut cursor, &modal)?;
    let mut declaration = json!({
        "kind": "castingDeclaration",
        "decisions": lowered.decisions,
    });
    let additional_costs = lower_mode_additional_costs(&modal);
    if !additional_costs.is_empty() {
        declaration["additionalCosts"] = Value::Array(additional_costs);
    }
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": declaration,
            "effects": lowered.effects,
        }),
        &[
            "Recognize the modal header frame and bullet boundaries from tokens",
            "Delegate every printed mode to the shared effect program",
            "Scope only mode-local declarations to their selected mode",
            "Preserve printed mode and effect order during lowering",
        ],
    ))
}

/// CR 702.172a: each Spree mode's cost is an additional cost paid exactly
/// when that mode is chosen. Bulleted modes carry no cost.
fn lower_mode_additional_costs(modal: &EffectSequenceModalNode) -> Vec<Value> {
    modal
        .mode_costs
        .iter()
        .enumerate()
        .map(|(index, cost)| {
            json!({
                "kind": "conditional",
                "condition": selection("chosenModes", &format!("mode{}", index + 1)),
                "then": [lower_resource_mana_payment(cost)],
            })
        })
        .collect()
}

/// One lowered occurrence of a mode: its selection condition and program.
struct LoweredModeOccurrence {
    selected: Value,
    lowered: LoweredEffectSequence,
}

/// Lowers one mode program under `prefix`: declarations and resolution-cost
/// results are local to that occurrence, so two selected modes (or two
/// occurrences of a repeated mode) never share one id.
fn lower_mode_occurrence(
    cursor: &mut OracleTokenCursor<'_>,
    mode: &EffectProgramNode,
    prefix: &str,
    selected: Value,
) -> MachineResult<LoweredModeOccurrence> {
    // A mode reads the spell face's X (CR 107.3a); its producer is checked
    // once the card is composed, like any other effect program of the face.
    let mut lowered = lower_effect_program_with_context(
        cursor,
        mode,
        EffectSequenceLoweringContext::ValidateXAfterCardComposition,
    )?;
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
        .map(|id| (id.to_string(), format!("{prefix}:{id}")))
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
    Ok(LoweredModeOccurrence { selected, lowered })
}

/// `the chosen modes contain this mode at least N times` (CR 700.2d).
fn selected_at_least(mode_id: &str, occurrences: u32) -> Value {
    json!({
        "kind": "selectionCountAtLeast",
        "selection": { "kind": "decisionResult", "decisionId": "chosenModes" },
        "value": mode_id,
        "minimum": integer(i64::from(occurrences)),
    })
}

/// Shared modal lowering (CR 700.2): one `chooseModes` declaration, each mode's
/// own declarations scoped to its selection, and one conditional per mode.
/// A repeatable mode (CR 700.2d) is lowered once per possible occurrence, in
/// printed order, each with its own declarations. Spells and triggered
/// abilities (modes chosen as the trigger is put on the stack, CR 700.2b)
/// reuse it unchanged.
pub(in crate::oracle::canonical) fn lower_effect_sequence_modal(
    cursor: &mut OracleTokenCursor<'_>,
    modal: &EffectSequenceModalNode,
) -> MachineResult<LoweredEffectSequence> {
    if modal.cardinality.minimum == 0 {
        cursor.enter("modal.lower.cardinality");
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::ZeroModeChoice,
            "a mode count of at least one (`up to N` and `any number` allow zero modes)",
        ));
    }
    let mode_ids = (1..=modal.modes.len())
        .map(|index| format!("mode{index}"))
        .collect::<Vec<_>>();
    let mut occurrences = Vec::new();
    for (mode, mode_id) in modal.modes.iter().zip(&mode_ids) {
        if modal.cardinality.allow_repeated {
            for occurrence in 1..=modal.cardinality.maximum {
                occurrences.push(lower_mode_occurrence(
                    cursor,
                    mode,
                    &format!("{mode_id}#{occurrence}"),
                    selected_at_least(mode_id, occurrence),
                )?);
            }
        } else {
            occurrences.push(lower_mode_occurrence(
                cursor,
                mode,
                mode_id,
                selection("chosenModes", mode_id),
            )?);
        }
    }

    let mut decisions = vec![json!({
        "id": "chosenModes",
        "kind": "chooseModes",
        "minimum": modal.cardinality.minimum,
        "maximum": integer(i64::from(modal.cardinality.maximum)),
        "options": mode_ids,
        "allowRepeated": modal.cardinality.allow_repeated,
    })];
    let mut effects = Vec::with_capacity(occurrences.len());
    for LoweredModeOccurrence { selected, lowered } in occurrences {
        decisions.extend(lowered.decisions);
        effects.push(json!({
            "kind": "conditional",
            "condition": selected,
            "then": lowered.effects,
        }));
    }
    // A shared X is declared once, whichever mode reads it. A mode that only
    // reads X declares nothing: the face's producer is checked after card
    // composition (an explicit declaration here would manufacture one).
    let mut seen_x = false;
    decisions.retain(|decision| {
        if decision["id"].as_str() != Some("xValue") {
            return true;
        }
        !std::mem::replace(&mut seen_x, true)
    });
    Ok(LoweredEffectSequence { effects, decisions })
}

#[cfg(test)]
#[path = "modal_spell_tests.rs"]
mod tests;
