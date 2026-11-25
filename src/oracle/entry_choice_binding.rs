//! Card-level binding of choices stored as a permanent enters (CR 614.12).
//!
//! `the chosen color` / `the chosen type` read by an ability that is not an
//! effect program has no resolution-time producer. On a permanent, its
//! producer is the `As ... enters, choose ...` replacement of the same face,
//! which stores the value on the permanent (`storeDecision`). Each ability is
//! parsed alone, so the reference stays symbolic with the resolution decision
//! id; this pass, run after card composition and before binding validation,
//! binds it to the stored decision when
//!
//! - the reading rule produces no value of that choice itself, and
//! - the same face has exactly one entry replacement storing a choice of a
//!   kind the reference reads.
//!
//! Otherwise the reference keeps its resolution id and the contextual
//! validator rejects it explicitly. The engine materializes the stored value
//! into the permanent's rules when the choice is made.
use serde_json::Value;

use crate::oracle::contextual_binding::{
    RESOLUTION_CARD_NAME_DECISION, RESOLUTION_COLOR_DECISION, RESOLUTION_CREATURE_TYPE_DECISION,
};
use crate::oracle::model::{ParsedOracleAbility, ParserDiagnostic};

/// One symbolic reference kind and the entry choices that can produce it.
struct ChoiceReference {
    reference_kind: &'static str,
    resolution_id: &'static str,
    producer_kinds: &'static [&'static str],
}

/// `of the chosen color` reads a color; `N mana of the chosen color`
/// produces it; `of the chosen type` reads the subtype chosen as the
/// permanent entered, a creature type or a basic land type (CR 205.3).
const CHOICE_REFERENCES: [ChoiceReference; 4] = [
    ChoiceReference {
        reference_kind: "chosenColor",
        resolution_id: RESOLUTION_COLOR_DECISION,
        producer_kinds: &["chooseColor"],
    },
    ChoiceReference {
        reference_kind: "storedColor",
        resolution_id: RESOLUTION_COLOR_DECISION,
        producer_kinds: &["chooseColor"],
    },
    ChoiceReference {
        reference_kind: "chosenCreatureType",
        resolution_id: RESOLUTION_CREATURE_TYPE_DECISION,
        producer_kinds: &["chooseCreatureType", "chooseBasicLandType"],
    },
    // `with the chosen name` reads the card name stored as it entered.
    ChoiceReference {
        reference_kind: "chosenCardName",
        resolution_id: RESOLUTION_CARD_NAME_DECISION,
        producer_kinds: &["chooseCardName"],
    },
];

/// A choice stored by an entry replacement: its face, kind and decision id.
struct StoredEntryChoice {
    face_id: Option<String>,
    kind: String,
    decision_id: String,
}

fn stored_entry_choices(rule: &Value) -> Vec<(String, String)> {
    if rule["kind"].as_str() != Some("replacementEffect")
        || rule["event"]["kind"].as_str() != Some("wouldEnterBattlefield")
        || rule["event"]["object"]["kind"].as_str() != Some("self")
    {
        return Vec::new();
    }
    let stored = rule["replacement"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|operation| operation["kind"].as_str() == Some("storeDecision"))
        .filter_map(|operation| operation["decisionId"].as_str())
        .collect::<Vec<_>>();
    rule["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|decision| {
            let id = decision["id"].as_str()?;
            let kind = decision["kind"].as_str()?;
            stored
                .contains(&id)
                .then(|| (kind.to_string(), id.to_string()))
        })
        .collect()
}

fn count_kind_with(value: &Value, kind: &str, field: &str, id: &str) -> usize {
    let own =
        usize::from(value["kind"].as_str() == Some(kind) && value[field].as_str() == Some(id));
    own + match value {
        Value::Object(values) => values
            .values()
            .map(|value| count_kind_with(value, kind, field, id))
            .sum(),
        Value::Array(values) => values
            .iter()
            .map(|value| count_kind_with(value, kind, field, id))
            .sum(),
        _ => 0,
    }
}

fn rebind(value: &mut Value, reference: &ChoiceReference, decision_id: &str) {
    if value["kind"].as_str() == Some(reference.reference_kind)
        && value["decisionId"].as_str() == Some(reference.resolution_id)
    {
        value["decisionId"] = Value::String(decision_id.to_string());
        return;
    }
    match value {
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| rebind(value, reference, decision_id)),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| rebind(value, reference, decision_id)),
        _ => {}
    }
}

/// Binds each face's chosen-value references to the choice its entry
/// replacement stores, when that producer is unique.
pub(crate) fn bind_entry_choices(abilities: &mut [ParsedOracleAbility]) {
    let stored = abilities
        .iter()
        .filter(|ability| ability.status == "canonical")
        .filter_map(|ability| Some((ability.source.face_id.clone(), ability.rule.as_ref()?)))
        .flat_map(|(face_id, rule)| {
            stored_entry_choices(rule)
                .into_iter()
                .map(move |(kind, decision_id)| StoredEntryChoice {
                    face_id: face_id.clone(),
                    kind,
                    decision_id,
                })
        })
        .collect::<Vec<_>>();
    for ability in abilities.iter_mut() {
        if ability.status != "canonical" {
            continue;
        }
        let face_id = ability.source.face_id.clone();
        let Some(rule) = ability.rule.as_mut() else {
            continue;
        };
        for reference in &CHOICE_REFERENCES {
            if count_kind_with(
                rule,
                reference.reference_kind,
                "decisionId",
                reference.resolution_id,
            ) == 0
            {
                continue;
            }
            // A rule that chooses the value itself reads its own choice.
            let produces_own = reference
                .producer_kinds
                .iter()
                .any(|kind| count_kind_with(rule, kind, "id", reference.resolution_id) > 0);
            if produces_own {
                continue;
            }
            let mut producers = stored
                .iter()
                .filter(|choice| {
                    choice.face_id == face_id
                        && reference
                            .producer_kinds
                            .iter()
                            .any(|kind| *kind == choice.kind)
                })
                .map(|choice| choice.decision_id.as_str())
                .collect::<Vec<_>>();
            producers.sort_unstable();
            producers.dedup();
            if let [decision_id] = producers.as_slice() {
                let decision_id = (*decision_id).to_string();
                rebind(rule, reference, &decision_id);
            }
        }
        // A produced stored color reads only a value stored on the source;
        // without its entry producer it would produce nothing.
        if count_kind_with(rule, "storedColor", "decisionId", RESOLUTION_COLOR_DECISION) > 0 {
            reject_unbound_stored_color(ability);
        }
    }
}

fn reject_unbound_stored_color(ability: &mut ParsedOracleAbility) {
    let source_span_end = ability.source.text.len();
    ability.status = "unsupported".to_string();
    ability.rule = None;
    ability.iterations.clear();
    ability.diagnostics.push(ParserDiagnostic {
        code: "oracle_context_binding_failure".to_string(),
        message: "Mana of the chosen color reads the color its source stored as it entered; this face has no unique `As ... enters, choose a color` producer.".to_string(),
        severity: "unsupported".to_string(),
        automaton: Some("card-context-binding".to_string()),
        state: Some("binding.chosen-color.entry-source".to_string()),
        token_index: None,
        span_start: Some(0),
        span_end: Some(source_span_end),
        expected: Some(
            "exactly one entry replacement of the same face that stores a chosen color"
                .to_string(),
        ),
        found: Some("mana of the chosen color".to_string()),
        trace: Vec::new(),
        owner: None,
    });
}

#[cfg(test)]
#[path = "entry_choice_binding_tests.rs"]
mod tests;
