use super::source_lki::{self, SourceNumericProperty, SourcePropertyInput};
use super::{CardInstance, EngineError, GameEngine, RuntimeBinding, StackObject};
use serde_json::Value;
use std::collections::BTreeMap;

/// Singular source-relative player references in numeric contexts without a
/// stack/target binding frame. Keep ability actor separate from physical source
/// controller, matching resolve_player's existing controllerOf(self) contract.
pub(super) fn source_relative_numeric_player<'a>(
    reference: &Value,
    ability_controller: &'a str,
    source: Option<&'a CardInstance>,
) -> Option<&'a str> {
    match reference["kind"].as_str() {
        None | Some("controller" | "abilityController") => Some(ability_controller),
        Some("controllerOf")
            if matches!(
                reference["object"]["kind"].as_str(),
                Some("self" | "abilitySource")
            ) =>
        {
            Some(ability_controller)
        }
        Some("controllerOfObject")
            if matches!(
                reference["object"]["kind"].as_str(),
                Some("self" | "abilitySource")
            ) =>
        {
            Some(source?.controller.as_str())
        }
        Some("ownerOf")
            if matches!(
                reference["object"]["kind"].as_str(),
                Some("self" | "abilitySource")
            ) =>
        {
            Some(source?.owner.as_str())
        }
        _ => None,
    }
}

/// Direction required by a bounded integer division in canonical arithmetic.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum IntegerRounding {
    Down,
    Up,
}

/// Checked arithmetic independent of a numeric leaf's resolution context.
/// Returning None never substitutes a numeric value for overflow or division by zero.
pub(super) fn checked_integer_arithmetic(
    operation: &str,
    left: i32,
    right: i32,
    rounding: Option<IntegerRounding>,
) -> Option<i32> {
    match operation {
        "add" if rounding.is_none() => left.checked_add(right),
        "subtract" if rounding.is_none() => left.checked_sub(right),
        "multiply" if rounding.is_none() => left.checked_mul(right),
        "divide" => {
            let rounding = rounding?;
            if right == 0 {
                return None;
            }
            let left = i64::from(left);
            let right = i64::from(right);
            let mut quotient = left / right;
            if left % right != 0 {
                match rounding {
                    IntegerRounding::Down if (left < 0) != (right < 0) => quotient -= 1,
                    IntegerRounding::Up if (left < 0) == (right < 0) => quotient += 1,
                    _ => {}
                }
            }
            i32::try_from(quotient).ok()
        }
        _ => None,
    }
}

/// Checked transport for canonical +, -, * and / trees. Contextual leaves
/// retain their existing resolver; arithmetic parents never reach that legacy
/// resolver (whose arithmetic remains unchanged for all other consumers).
pub(super) fn evaluate_checked_numeric_tree(
    expression: &Value,
    resolve_leaf: &mut impl FnMut(&Value) -> Option<i32>,
    resolve_condition: &mut impl FnMut(&Value) -> Option<bool>,
) -> Option<i32> {
    fn visit(
        expression: &Value,
        resolve_leaf: &mut impl FnMut(&Value) -> Option<i32>,
        resolve_condition: &mut impl FnMut(&Value) -> Option<bool>,
        depth: usize,
    ) -> Option<i32> {
        if depth >= 64 {
            return None;
        }
        match expression["kind"].as_str() {
            Some(
                operation @ ("add" | "subtract" | "multiply" | "divide" | "absoluteDifference"),
            ) => {
                let left = if operation == "multiply" {
                    expression.get("left").or_else(|| expression.get("value"))?
                } else {
                    expression.get("left")?
                };
                let right = if operation == "multiply" {
                    expression
                        .get("right")
                        .or_else(|| expression.get("factor"))?
                } else {
                    expression.get("right")?
                };
                let rounding = if operation == "divide" {
                    Some(match expression["round"].as_str()? {
                        "down" => IntegerRounding::Down,
                        "up" => IntegerRounding::Up,
                        _ => return None,
                    })
                } else {
                    None
                };
                let left = visit(left, resolve_leaf, resolve_condition, depth + 1)?;
                let right = visit(right, resolve_leaf, resolve_condition, depth + 1)?;
                if operation == "absoluteDifference" {
                    let difference = checked_integer_arithmetic("subtract", left, right, None)?;
                    if difference < 0 {
                        checked_integer_arithmetic("subtract", 0, difference, None)
                    } else {
                        Some(difference)
                    }
                } else {
                    checked_integer_arithmetic(operation, left, right, rounding)
                }
            }
            Some("negate") => {
                let value = visit(
                    expression.get("operand")?,
                    resolve_leaf,
                    resolve_condition,
                    depth + 1,
                )?;
                checked_integer_arithmetic("subtract", 0, value, None)
            }
            Some(operation @ ("minimum" | "maximum")) => {
                let mut operands = expression["operands"].as_array()?.iter();
                let first = visit(operands.next()?, resolve_leaf, resolve_condition, depth + 1)?;
                operands.try_fold(first, |current, operand| {
                    let value = visit(operand, resolve_leaf, resolve_condition, depth + 1)?;
                    Some(if operation == "minimum" {
                        current.min(value)
                    } else {
                        current.max(value)
                    })
                })
            }
            Some("conditionalValue") => {
                let branch = if resolve_condition(expression.get("condition")?)? {
                    expression.get("ifTrue")?
                } else {
                    expression.get("ifFalse")?
                };
                visit(branch, resolve_leaf, resolve_condition, depth + 1)
            }
            _ => resolve_leaf(expression),
        }
    }
    visit(expression, resolve_leaf, resolve_condition, 0)
}

impl GameEngine {
    /// Prepare only evaluated numeric condition arguments; the existing runtime
    /// condition service remains the sole owner of Boolean truth and short circuit.
    pub(super) fn checked_runtime_numeric_condition(
        &self,
        condition: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        fn prepare(
            engine: &GameEngine,
            value: &Value,
            stack: &StackObject,
            bindings: &BTreeMap<String, RuntimeBinding>,
            decisions: &BTreeMap<String, Value>,
            depth: usize,
        ) -> Result<Value, EngineError> {
            if depth >= 64 {
                return Err(EngineError::new(
                    "numeric condition nesting exceeds supported depth",
                ));
            }
            let mut prepared = value.clone();
            match value["kind"].as_str() {
                Some("compare") => {
                    for key in ["left", "right"] {
                        prepared[key] = Value::from(engine.checked_runtime_numeric_expression(
                            &value[key],
                            stack,
                            bindings,
                            decisions,
                            "numeric condition",
                        )?);
                    }
                }
                Some("objectMatchesFilter") => {
                    // Surface a missing binding, an ambiguous collection, a
                    // departed permanent or an invalid operand before the
                    // infallible evaluation below reads the same condition.
                    engine.checked_object_condition_matches(value, stack, bindings, decisions)?;
                }
                Some("not") => {
                    prepared["operand"] = prepare(
                        engine,
                        &value["operand"],
                        stack,
                        bindings,
                        decisions,
                        depth + 1,
                    )?
                }
                Some(kind @ ("and" | "or")) => {
                    let operands = value["operands"]
                        .as_array()
                        .ok_or_else(|| EngineError::new("missing logical condition operands"))?;
                    for (index, operand) in operands.iter().enumerate() {
                        let child =
                            prepare(engine, operand, stack, bindings, decisions, depth + 1)?;
                        let truth =
                            engine.runtime_condition_value(&child, stack, bindings, decisions);
                        prepared["operands"][index] = child;
                        if (kind == "and" && !truth) || (kind == "or" && truth) {
                            break;
                        }
                    }
                }
                _ => {}
            }
            Ok(prepared)
        }
        let prepared = prepare(self, condition, stack, bindings, decisions, 0)?;
        Ok(self.runtime_condition_value(&prepared, stack, bindings, decisions))
    }

    /// Strict consumer boundary: never substitute zero for missing bindings,
    /// invalid division or checked arithmetic overflow in these count trees.
    pub(super) fn checked_runtime_numeric_expression(
        &self,
        expression: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        field: &'static str,
    ) -> Result<i32, EngineError> {
        let mut query_error = None;
        let mut condition_error = None;
        let evaluated = evaluate_checked_numeric_tree(
            expression,
            &mut |leaf| {
                if leaf["kind"].as_str() == Some("countObjects") {
                    match self.checked_runtime_object_count(
                        &leaf["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    ) {
                        Ok(count) => Some(count),
                        Err(error) => {
                            query_error = Some(error);
                            None
                        }
                    }
                } else if leaf["kind"].as_str() == Some("countCounters")
                    && matches!(
                        leaf["object"]["kind"].as_str(),
                        Some("self" | "abilitySource")
                    )
                {
                    let count = (|| {
                        let source = self
                            .checked_battlefield_source_object(stack_object)
                            .map_err(|error| {
                                EngineError::new(format!(
                                    "source counter query for {field} on stack {}: {error}",
                                    stack_object.id,
                                ))
                            })?
                            .card;
                        let count = match leaf.get("counter") {
                            Some(Value::String(name)) => {
                                source.counters.get(name).copied().unwrap_or(0)
                            }
                            None => source.counters.values().try_fold(0_i32, |sum, amount| {
                                if *amount < 0 {
                                    return Err(EngineError::new(
                                        "invalid negative source counter count",
                                    ));
                                }
                                sum.checked_add(*amount).ok_or_else(|| {
                                    EngineError::new(
                                        "source counter count exceeds its integer bound",
                                    )
                                })
                            })?,
                            Some(_) => return Err(EngineError::new("invalid source counter name")),
                        };
                        if count < 0 {
                            return Err(EngineError::new("invalid negative source counter count"));
                        }
                        Ok(count)
                    })();
                    match count {
                        Ok(count) => Some(count),
                        Err(error) => {
                            query_error = Some(error);
                            None
                        }
                    }
                } else {
                    self.runtime_numeric_expression(leaf, stack_object, bindings, decisions)
                }
            },
            &mut |condition| match self.checked_runtime_numeric_condition(
                condition,
                stack_object,
                bindings,
                decisions,
            ) {
                Ok(value) => Some(value),
                Err(error) => {
                    condition_error = Some(error);
                    None
                }
            },
        );
        if let Some(error) = condition_error {
            return Err(error);
        }
        if let Some(error) = query_error {
            return Err(error);
        }
        evaluated.ok_or_else(|| {
            EngineError::new(format!(
                "unresolved or invalid numeric expression for {field} on stack {}: {expression}",
                stack_object.id,
            ))
        })
    }
}

/// Evaluates a numeric property from the live ability source, captured
/// last-known information, or the source snapshot selected by `source_lki`.
pub(super) fn evaluate_source_property(
    property_kind: &str,
    live_source: Option<&CardInstance>,
    stack_object: &StackObject,
    power: impl Fn(&CardInstance) -> i32,
    toughness: impl Fn(&CardInstance) -> i32,
) -> Option<i32> {
    let property = match property_kind {
        "powerOf" => SourceNumericProperty::Power,
        "toughnessOf" => SourceNumericProperty::Toughness,
        _ => return None,
    };
    match source_lki::property_input(stack_object, live_source, property)? {
        SourcePropertyInput::Captured(value) => Some(value),
        SourcePropertyInput::Live(source) | SourcePropertyInput::Snapshot(source) => {
            Some(match property {
                SourceNumericProperty::Power => power(source),
                SourceNumericProperty::Toughness => toughness(source),
            })
        }
    }
}

/// Authenticates one current battlefield object against the source snapshot.
/// Missing historical markers cannot prove that a same-id returned card is
/// the old object. A directly borrowed live object needs no inferred marker.
fn same_battlefield_source(source: &CardInstance, current: &CardInstance) -> Option<bool> {
    if source.instance_id != current.instance_id {
        return Some(false);
    }
    if std::ptr::eq(source, current) {
        return Some(true);
    }
    match (
        GameEngine::tracked_zone_identity(source),
        GameEngine::tracked_zone_identity(current),
    ) {
        (Some(captured), Some(live)) => Some(captured == live),
        _ => None,
    }
}

impl GameEngine {
    /// Applies source exclusion only to an authenticated current object that
    /// belongs to the exact set already counted by the caller. The callback
    /// receives the current permanent and its current battlefield player.
    ///
    /// Departed snapshots and returned incarnations never reduce the count.
    /// An untracked same-id snapshot remains unresolved instead of guessing.
    pub(super) fn exclude_current_source_from_count(
        &self,
        count: i32,
        source: Option<&CardInstance>,
        source_departed: bool,
        is_counted: impl FnOnce(&CardInstance, &str) -> bool,
    ) -> Option<i32> {
        if source_departed {
            return Some(count);
        }
        let Some(source) = source else {
            return Some(count);
        };
        let Some((player, index)) = self.permanent_position(&source.instance_id) else {
            return Some(count);
        };
        let current = &self.state.players[player].battlefield[index];
        if !is_counted(current, &self.state.players[player].id) {
            return Some(count);
        }
        if !same_battlefield_source(source, current)? {
            return Some(count);
        }
        // A member of a correctly counted set contributes exactly one.
        // If the callback and count disagree, do not synthesize a number.
        count.checked_sub(1).filter(|value| *value >= 0)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "numeric_evaluation_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "checked_numeric_expression_tests.rs"]
mod checked_expression_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "numeric_source_player_tests.rs"]
mod source_player_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "source_counter_query_tests.rs"]
mod source_counter_query_tests;
