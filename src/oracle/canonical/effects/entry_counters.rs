use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

/// Claims a resolved source reference followed by `enters with`. The child
/// automaton validates quantity, counter kind, recipient and terminator, so a
/// condition or unsupported quantity fails at a precise state.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_counters_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("source-entry-counters-claim", input.tokens);
    // `If CONDITION, SOURCE enters with ...`: the modification starts after
    // the condition's comma; the recognizer reads the condition itself.
    if input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
    {
        let Some(comma) = input.tokens.iter().position(|token| token.raw == ",") else {
            return false;
        };
        if cursor.advance_to(comma + 1).is_err() {
            return false;
        }
    }
    // `You may have SOURCE enter with ...`: the optional form.
    let optional = cursor.consume_phrase(&["you", "may", "have"]).is_ok();
    if !matches!(
        recognize_source_reference(&mut cursor, input.name_links, input.name_resolution_failure),
        Ok(LocalObjectReference::SourceSelf)
    ) {
        return false;
    }
    let position = cursor.position();
    input.tokens.get(position).is_some_and(|token| {
        token
            .raw
            .eq_ignore_ascii_case(if optional { "enter" } else { "enters" })
    }) && input
        .tokens
        .get(position + 1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("with"))
}

const OPTIONAL_ENTRY_DECISION: &str = "optionalEntryCounters";
/// The counter kind the controller chose as the source enters.
const ENTRY_COUNTER_KIND_DECISION: &str = "entryCounterKind";

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_counters(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-entry-counters", input.tokens);
    let node = recognize_source_entry_counters(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("replacement.entry-counters.validation");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent card face for an entering source",
    )?;
    let count = cursor.require_value(
        lower_numeric_expression(&node.count),
        "a counter quantity the entry replacement can evaluate",
    )?;
    let placement = json!({
        "kind": "putEnteringCounters",
        "counter": node.counter.as_str(),
        "count": count,
    });
    let mut decisions = Vec::new();
    let replacement = if node.alternatives.is_empty() {
        vec![placement]
    } else {
        // `your choice of A or B` (CR 614.1c): the controller chooses one
        // kind as the source enters; only that placement is applied.
        let mut placements = vec![(node.counter.as_str(), placement)];
        for option in &node.alternatives {
            let count = cursor.require_value(
                lower_numeric_expression(&option.count),
                "a counter quantity the entry replacement can evaluate",
            )?;
            placements.push((
                option.counter.as_str(),
                json!({
                    "kind": "putEnteringCounters",
                    "counter": option.counter.as_str(),
                    "count": count,
                }),
            ));
        }
        decisions.push(json!({
            "id": ENTRY_COUNTER_KIND_DECISION,
            "kind": "chooseOption",
            "options": placements.iter().map(|(counter, _)| *counter).collect::<Vec<_>>(),
        }));
        placements
            .into_iter()
            .map(|(counter, placement)| {
                json!({
                    "kind": "conditional",
                    "condition": {
                        "kind": "selectionContains",
                        "selection": {
                            "kind": "decisionResult",
                            "decisionId": ENTRY_COUNTER_KIND_DECISION,
                        },
                        "value": counter,
                    },
                    "then": [placement],
                })
            })
            .collect()
    };
    let mut rule = json!({
        "kind": "replacementEffect",
        "source": self_ref(),
        "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
        "replacement": replacement,
    });
    let mut conditions = Vec::new();
    if node.optional {
        // CR 614.1c: the controller chooses, as it enters, whether the
        // optional replacement applies.
        decisions.insert(
            0,
            json!({ "id": OPTIONAL_ENTRY_DECISION, "kind": "chooseWhether" }),
        );
        conditions.push(json!({
            "kind": "decisionWas",
            "decisionId": OPTIONAL_ENTRY_DECISION,
            "value": true,
        }));
    }
    if !decisions.is_empty() {
        rule["decisions"] = Value::Array(decisions);
    }
    if let Some(condition) = &node.condition {
        conditions.push(lower_condition(condition));
    }
    match conditions.len() {
        0 => {}
        1 => rule["condition"] = conditions.remove(0),
        _ => rule["condition"] = and(conditions),
    }
    Ok(draft(
        rule,
        &[
            "Recognize an entering source and its counter quantity",
            "Compose the shared counter name and numeric quantity",
            "Apply one primitive entering-counter replacement",
        ],
    ))
}

#[cfg(test)]
#[path = "entry_counters_tests.rs"]
mod tests;
