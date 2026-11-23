//! Shared conservative preflight for fallible structured resolution programs.
//! Domain predicates stay shallow; this module owns the sole recursive walk.
//! This inspects IR, never Oracle text, and does not predict any decision.
use serde_json::Value;

pub(super) fn requires_checkpoint(program: &Value) -> bool {
    fn visit(value: &Value, depth: usize) -> bool {
        // Unknown depth cannot prove that execution has no permission.
        if depth >= 64 {
            return true;
        }
        match value {
            Value::Object(fields) => {
                super::effect_domains::node_requires_checkpoint(value)
                    || super::resolution_permissions::node_requires_checkpoint(value)
                    || super::resolution_costs::node_requires_checkpoint(value)
                    || super::target_reassignment::node_requires_checkpoint(value)
                    || super::life::node_requires_checkpoint(value)
                    || super::permanent_presence_query::node_requires_checkpoint(value)
                    || super::token_creation::node_requires_checkpoint(value)
                    || super::entry_observation::node_requires_checkpoint(value)
                    || super::permanent_subject::node_requires_checkpoint(value)
                    || super::damage_execution::node_requires_checkpoint(value)
                    || super::mana_value_execution::node_requires_checkpoint(value)
                    || fields.values().any(|child| visit(child, depth + 1))
            }
            Value::Array(children) => children.iter().any(|child| visit(child, depth + 1)),
            _ => false,
        }
    }
    visit(program, 0)
}

/// Target commitment can remove the old top or enqueue targeting triggers.
/// Inspect the whole existing stack and conservatively checkpoint any triggered
/// declaration before that phase; never infer that the old top will resolve.
pub(super) fn stack_requires_checkpoint(stack: &[super::StackObject]) -> bool {
    // Only the actual top can leave the stack during this resolution call.
    // Pending triggered declarations below retain the conservative guard:
    // committing them can remove the old top or enqueue another object.
    if stack
        .last()
        .is_some_and(super::card_zone_transition::stack_requires_checkpoint)
    {
        return true;
    }
    stack.iter().any(|object| {
        let pending_declaration = object.ability_kind.as_deref() == Some("triggeredAbility")
            && object.ability_rule.as_ref().is_some_and(|rule| {
                rule["declaration"]["decisions"]
                    .as_array()
                    .is_some_and(|decisions| !decisions.is_empty())
            });
        pending_declaration
            || object.ability_rule.as_ref().map_or_else(
                || {
                    object
                        .card
                        .definition
                        .printed_rules()
                        .iter()
                        .any(requires_checkpoint)
                },
                requires_checkpoint,
            )
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "resolution_zone_preflight_tests.rs"]
mod zone_tests;
