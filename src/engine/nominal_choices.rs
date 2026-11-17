//! Canonical creature-type choices, independent of hidden or visible game state.
use super::EngineError;
use deepdeck_card_domain::{all_creature_subtypes, resolve_creature_choice};
use serde_json::Value;
use std::collections::BTreeSet;

/// Explicit options restrict the legal set; omitted options mean every known
/// creature identity. Both use one stable registry order and IR spelling.
/// No provider or engine mutation occurs before this checked preparation.
pub(super) fn creature_type_options(explicit: Option<&Value>) -> Result<Vec<String>, EngineError> {
    let canonical = all_creature_subtypes().map_err(|error| {
        EngineError::new(format!("nominal.creature-choice.registry: {error:?}"))
    })?;
    let restricted = explicit
        .map(|value| {
            let values = value
                .as_array()
                .filter(|values| !values.is_empty())
                .ok_or_else(|| {
                    EngineError::new("nominal.creature-choice.options: expected nonempty array")
                })?;
            values
                .iter()
                .map(|value| {
                    let raw = value
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            EngineError::new(
                                "nominal.creature-choice.options: expected nonempty string",
                            )
                        })?;
                    let id = resolve_creature_choice(raw).map_err(|error| {
                        EngineError::new(format!("nominal.creature-choice.options: {error:?}"))
                    })?;
                    Ok(id.canonical_name().to_string())
                })
                .collect::<Result<BTreeSet<_>, EngineError>>()
        })
        .transpose()?;
    let options = canonical
        .iter()
        .filter(|id| {
            restricted
                .as_ref()
                .is_none_or(|requested| requested.contains(id.canonical_name()))
        })
        .map(|id| id.ir_name().to_string())
        .collect::<Vec<_>>();
    if options.is_empty() {
        return Err(EngineError::new(
            "nominal.creature-choice.options: no supported creature identity",
        ));
    }
    Ok(options)
}

/// A single entry rule owns one lexical decision scope. An identifier cannot
/// name two producers if either producer is a creature-type choice, even when
/// their option sets happen to agree. Separate rules retain separate scopes.
pub(super) fn validate_entry_creature_choice_scope(rule: &Value) -> Result<(), EngineError> {
    let mut producers = std::collections::BTreeMap::<&str, bool>::new();
    for decision in rule["decisions"].as_array().into_iter().flatten() {
        let creature = super::value_kind(decision) == Some("chooseCreatureType");
        let Some(id) = decision["id"].as_str() else {
            if creature {
                return Err(EngineError::new(
                    "nominal.entry-choice.binding-scope: missing producer identifier",
                ));
            }
            continue;
        };
        if creature && id.is_empty() {
            return Err(EngineError::new(
                "nominal.entry-choice.binding-scope: empty producer identifier",
            ));
        }
        if let Some(previous_creature) = producers.insert(id, creature) {
            if creature || previous_creature {
                return Err(EngineError::new(format!(
                    "nominal.entry-choice.binding-scope: duplicate producer `{id}` within one rule"
                )));
            }
        }
    }
    Ok(())
}

/// Constant nominal option schemas are checked before any entry mutation.
/// Only PRESENT initial bindings are consumed; normal choices remain in their
/// existing payment/condition order. No world-derived applicability is guessed.
pub(super) fn prepare_entry_creature_type_bindings<'a>(
    definition: &super::CardDefinition,
    initial: &'a std::collections::BTreeMap<String, Value>,
) -> Result<std::borrow::Cow<'a, std::collections::BTreeMap<String, Value>>, EngineError> {
    let mut entry_decisions = Vec::new();
    for rule in definition.rules.iter().filter(|rule| {
        super::value_kind(rule) == Some("replacementEffect")
            && super::value_kind(&rule["event"]) == Some("wouldEnterBattlefield")
    }) {
        validate_entry_creature_choice_scope(rule)?;
        entry_decisions.extend(rule["decisions"].as_array().into_iter().flatten());
    }
    let mut prepared = std::borrow::Cow::Borrowed(initial);
    let mut reused_sets = std::collections::BTreeMap::<String, Vec<String>>::new();
    for decision in entry_decisions
        .iter()
        .copied()
        .filter(|decision| super::value_kind(decision) == Some("chooseCreatureType"))
    {
        let id = decision["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| EngineError::new("nominal.entry-choice: missing producer identifier"))?;
        let options = creature_type_options(decision.get("options"))?;
        let Some(value) = initial.get(id) else {
            continue;
        };
        // Separate per-rule choices may share a label. Reusing one input across
        // different producer contracts has no single authenticated choice set.
        if entry_decisions.iter().any(|other| {
            other["id"].as_str() == Some(id)
                && super::value_kind(other) != Some("chooseCreatureType")
        }) {
            return Err(EngineError::new(format!(
                "nominal.entry-choice.binding-scope: ambiguous producer `{id}`"
            )));
        }
        if reused_sets
            .get(id)
            .is_some_and(|previous| previous != &options)
        {
            return Err(EngineError::new(format!(
                "nominal.entry-choice.binding-scope: incompatible options for `{id}`"
            )));
        }
        reused_sets.insert(id.to_string(), options.clone());
        let raw = value
            .as_str()
            .or_else(|| {
                value
                    .as_array()
                    .filter(|values| values.len() == 1)
                    .and_then(|values| values[0].as_str())
            })
            .ok_or_else(|| {
                EngineError::new(format!(
                    "nominal.entry-choice.binding: expected one creature type for `{id}`"
                ))
            })?;
        let nominal = resolve_creature_choice(raw).map_err(|error| {
            EngineError::new(format!("nominal.entry-choice.binding `{id}`: {error}"))
        })?;
        let canonical = nominal.ir_name();
        if !options.iter().any(|option| option == canonical) {
            return Err(EngineError::new(format!(
                "nominal.entry-choice.binding: `{id}` is outside its producer options"
            )));
        }
        let normalized = Value::String(canonical.to_string());
        if initial.get(id) != Some(&normalized) {
            prepared.to_mut().insert(id.to_string(), normalized);
        }
    }
    Ok(prepared)
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "nominal_choices_tests.rs"]
mod tests;
