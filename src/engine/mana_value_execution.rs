//! Checked executable mana-value interpretation. No discovery caller is routed
//! here until its Result transport can be integrated atomically.
use super::mana_projection_context::{ExecutableManaOutputs, ManaProjectionContext};
use super::*;

impl GameEngine {
    pub(super) fn try_mana_options_for_value(
        &self,
        value: &Value,
        controller_id: &str,
        source: Option<&CardInstance>,
    ) -> Result<ExecutableManaOutputs, EngineError> {
        let source = source.ok_or_else(|| {
            EngineError::new(
                "unsupported mana evaluation: state=mana.execution.source-context missing source",
            )
        })?;
        let context = ManaProjectionContext::for_value_interpretation(self);
        context
            .executable_outputs(value, controller_id, source)
            .map_err(|failure| failure.into_engine_error())
    }
    pub(super) fn try_runtime_mana_options(
        &self,
        value: &Value,
        controller_id: &str,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<ExecutableManaOutputs, EngineError> {
        let scalar = |expression: &Value| {
            self.runtime_numeric_expression(expression, stack_object, bindings, decisions)
        };
        let produced;
        let value = if value_kind(value) == Some("manaTypesProducedByEvent") {
            produced = event_produced_mana_types(value, stack_object, decisions);
            &produced
        } else {
            value
        };
        let context = ManaProjectionContext::for_value_interpretation(self);
        context
            .executable_outputs_with_scalar(value, controller_id, &stack_object.card, &scalar)
            .map_err(|failure| failure.into_engine_error())
    }
}

/// `N mana of any type that land produced` (CR 106.1b, 605.1b): one type
/// among the mana the triggering mana ability produced, which its event
/// record (`manaProduced`) carries onto the triggered ability. The value is
/// the ordinary choice among those types; without a produced type (the
/// record is absent or produced nothing) it produces no mana, never a guess.
pub(super) fn event_produced_mana_types(
    value: &Value,
    stack_object: &StackObject,
    decisions: &BTreeMap<String, Value>,
) -> Value {
    let amount = integer_value(&value["amount"]).unwrap_or(1).max(0);
    let types = decisions
        .get("manaProduced")
        .or_else(|| stack_object.decisions.get("manaProduced"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|symbol| super::mana_projection_contract::ManaType::from_symbol(symbol).is_some())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    let options = types
        .into_iter()
        .map(|symbol| json!({ "kind": "fixedMana", "symbol": symbol, "amount": amount }))
        .collect::<Vec<_>>();
    match options.len() {
        0 => json!({ "kind": "fixedMana", "symbol": "C", "amount": 0 }),
        1 => options.into_iter().next().expect("one produced type"),
        _ => json!({ "kind": "chooseOne", "options": options }),
    }
}

/// Shallow predicate only. The shared resolution_preflight owns recursion.
pub(super) fn node_requires_checkpoint(node: &Value) -> bool {
    if value_kind(node) != Some("addMana") {
        return false;
    }
    let known_actor = value_kind(&node["player"]) == Some("abilityController")
        || (value_kind(&node["player"]) == Some("controllerOf")
            && matches!(
                value_kind(&node["player"]["object"]),
                Some("self" | "abilitySource")
            ));
    let Some(literal) = node["mana"].as_str() else {
        return true;
    };
    let symbols = parse_mana_symbols(literal);
    !known_actor
        || symbols.is_empty()
        || symbols
            .iter()
            .any(|s| super::mana_projection_contract::ManaType::from_symbol(s).is_none())
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_value_execution_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_resolution_tests.rs"]
mod resolution_tests;
