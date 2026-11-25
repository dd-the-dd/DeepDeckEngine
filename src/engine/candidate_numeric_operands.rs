//! Candidate-independent amounts on the right of a candidate comparison
//! (`target creature with mana value less than or equal to the number of
//! lands you control`, `a creature with power greater than or equal to your
//! life total`).
//!
//! The comparison is a property of each candidate; its threshold is an
//! amount relative to the player choosing the candidates. The amount is read
//! each time the candidates are enumerated (CR 601.2c when targets are
//! chosen, CR 608.2b when they are rechecked on resolution), and bound into
//! the filter before enumeration, so every candidate matcher keeps comparing
//! a candidate property with an integer.
//!
//! Only the amounts below are bound. Their evaluator reads the player whose
//! candidates are enumerated; every player reference they carry must denote
//! that player (or every player). Any other operand is left as it is, and
//! the support checks of its consumer decide.

use std::borrow::Cow;

use serde_json::Value;

use super::numeric_evaluation::evaluate_checked_numeric_tree;
use super::{GameEngine, integer_value, value_kind};

/// A player reference that denotes the player whose candidates are
/// enumerated: absent, the controller, or the controller of the source.
fn enumerating_player(reference: Option<&Value>) -> bool {
    match reference {
        None => true,
        Some(reference) => match value_kind(reference) {
            Some("controller" | "abilityController") => true,
            Some("controllerOf") => matches!(
                value_kind(&reference["object"]),
                Some("self" | "abilitySource")
            ),
            _ => false,
        },
    }
}

/// A count or player fact relative to the enumerating player that the
/// controller-relative numeric evaluator reads without a source object.
fn controller_relative_leaf(expression: &Value) -> bool {
    if expression["excludeSource"].as_bool() == Some(true) {
        return false;
    }
    let player = expression.get("player");
    match value_kind(expression) {
        Some("countPermanents") => {
            expression["allPlayers"].as_bool() == Some(true) || enumerating_player(player)
        }
        Some("countCards") => {
            matches!(
                value_kind(&expression["zone"]),
                Some("hand" | "graveyard" | "library")
            ) && (expression["allPlayers"].as_bool() == Some(true)
                || enumerating_player(expression["zone"].get("player")))
        }
        Some(
            "countPermanentsControlledByOpponents"
            | "countDistinctPermanentNames"
            | "domainCount"
            | "greatestPower"
            | "greatestToughness"
            | "sumPowers"
            | "countOpponents"
            | "countOpponentsAttackedThisCombat"
            | "devotion"
            | "lifeTotal"
            | "lifeGainedThisTurn"
            | "countPlayerCounters",
        ) => enumerating_player(player),
        _ => false,
    }
}

/// An amount the binder reads: arithmetic over integers and
/// controller-relative leaves, with at least one leaf to read.
fn bindable_amount(expression: &Value) -> bool {
    fn visit(expression: &Value, depth: usize, leaves: &mut usize) -> bool {
        if depth >= 32 {
            return false;
        }
        if integer_value(expression).is_some() {
            return true;
        }
        match value_kind(expression) {
            Some("add" | "subtract" | "multiply") => {
                visit(&expression["left"], depth + 1, leaves)
                    && visit(&expression["right"], depth + 1, leaves)
            }
            Some("divide") => {
                matches!(expression["round"].as_str(), Some("up" | "down"))
                    && visit(&expression["left"], depth + 1, leaves)
                    && visit(&expression["right"], depth + 1, leaves)
            }
            _ if controller_relative_leaf(expression) => {
                *leaves += 1;
                true
            }
            _ => false,
        }
    }
    let mut leaves = 0;
    visit(expression, 0, &mut leaves) && leaves > 0
}

/// Whether a candidate filter compares a candidate property with an amount
/// the binder reads.
fn filter_needs_binding(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("compare") => {
            value_kind(&filter["left"]["object"]) == Some("candidate")
                && bindable_amount(&filter["right"])
        }
        Some("and" | "or") => filter["operands"]
            .as_array()
            .is_some_and(|operands| operands.iter().any(filter_needs_binding)),
        Some("not") => filter_needs_binding(&filter["operand"]),
        _ => false,
    }
}

fn candidates_need_binding(candidates: &Value) -> bool {
    if value_kind(candidates) == Some("union") {
        return candidates["sets"]
            .as_array()
            .is_some_and(|sets| sets.iter().any(candidates_need_binding));
    }
    candidates.get("where").is_some_and(filter_needs_binding)
}

impl GameEngine {
    /// The amount relative to `controller_id`, read now. `None` when a
    /// leaf has no value (a missing player); the filter then stays unbound.
    fn controller_relative_amount(&self, expression: &Value, controller_id: &str) -> Option<i32> {
        evaluate_checked_numeric_tree(
            expression,
            &mut |leaf| self.numeric_expression_with_source(leaf, controller_id, None),
            &mut |_: &Value| None::<bool>,
        )
    }

    fn bind_candidate_filter(&self, filter: &mut Value, controller_id: &str) {
        match value_kind(filter) {
            Some("compare")
                if value_kind(&filter["left"]["object"]) == Some("candidate")
                    && bindable_amount(&filter["right"]) =>
            {
                if let Some(value) =
                    self.controller_relative_amount(&filter["right"], controller_id)
                {
                    filter["right"] = Value::from(value);
                }
            }
            Some("and" | "or") => {
                if let Some(operands) = filter["operands"].as_array_mut() {
                    for operand in operands {
                        self.bind_candidate_filter(operand, controller_id);
                    }
                }
            }
            Some("not") => self.bind_candidate_filter(&mut filter["operand"], controller_id),
            _ => {}
        }
    }

    fn bind_candidate_sets(&self, candidates: &mut Value, controller_id: &str) {
        if value_kind(candidates) == Some("union") {
            if let Some(sets) = candidates["sets"].as_array_mut() {
                for set in sets {
                    self.bind_candidate_sets(set, controller_id);
                }
            }
            return;
        }
        if let Some(filter) = candidates.get_mut("where") {
            self.bind_candidate_filter(filter, controller_id);
        }
    }

    /// The candidate domain with every controller-relative comparison
    /// amount read for `controller_id`. Borrowed unchanged when the domain
    /// compares with no such amount.
    pub(super) fn bind_candidate_amounts<'a>(
        &self,
        candidates: &'a Value,
        controller_id: &str,
    ) -> Cow<'a, Value> {
        if !candidates_need_binding(candidates) {
            return Cow::Borrowed(candidates);
        }
        let mut bound = candidates.clone();
        self.bind_candidate_sets(&mut bound, controller_id);
        Cow::Owned(bound)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "candidate_numeric_operands_tests.rs"]
mod tests;
