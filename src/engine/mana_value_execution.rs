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
        let context = ManaProjectionContext::for_value_interpretation(self);
        context
            .executable_outputs_with_scalar(value, controller_id, &stack_object.card, &scalar)
            .map_err(|failure| failure.into_engine_error())
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
