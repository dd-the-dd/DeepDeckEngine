use super::super::*;

pub(in crate::oracle::canonical) fn numeric_atom_uses_x(atom: &NumericAtom) -> bool {
    atom == &NumericAtom::VariableX
}

pub(in crate::oracle::canonical) fn lower_numeric_atom(atom: NumericAtom) -> Value {
    match atom {
        NumericAtom::Integer(value) => integer(value),
        NumericAtom::VariableX => decision_result("xValue"),
    }
}

/// Target declarations historically encode literal bounds as scalar JSON,
/// while `X` remains a casting decision result. Keep that stable shape while
/// allowing the recognizer to retain one typed bound representation.
pub(in crate::oracle::canonical) fn lower_target_cardinality_bound(atom: NumericAtom) -> Value {
    match atom {
        NumericAtom::Integer(value) => json!(value),
        NumericAtom::VariableX => decision_result("xValue"),
    }
}

/// Stack value bound when the sacrifice cost is paid (`bindPowerAs` /
/// `bindToughnessAs` on the producing cost). Card composition installs the
/// binding on the same-face cost that produces it.
pub(in crate::oracle::canonical) fn sacrificed_cost_object_binding(
    property: NumericObjectProperty,
) -> &'static str {
    match property {
        NumericObjectProperty::Power => "sacrificedPermanentPower",
        NumericObjectProperty::Toughness => "sacrificedPermanentToughness",
    }
}

/// Lowers expressions that do not require a target-property binding. The
/// caller retains ownership of binding-aware forms such as `PowerOf`.
pub(in crate::oracle::canonical) fn lower_numeric_expression(
    expression: &NumericExpression,
) -> Option<Value> {
    lower_numeric_expression_in_context(
        expression,
        None,
        None,
        NumericQueryLoweringSupport::Unavailable,
    )
}

/// A parent may lower event quantities only with the binding it produced.
/// The default lowering path cannot emit an unbound eventQuantity leaf.
pub(in crate::oracle::canonical) fn lower_numeric_expression_for_event(
    expression: &NumericExpression,
    binding: EventNumericBinding,
) -> Option<Value> {
    lower_numeric_expression_in_context(
        expression,
        Some(binding),
        None,
        NumericQueryLoweringSupport::Unavailable,
    )
}

/// The single projection of a `for each` permanent count. Static continuous
/// modifiers and every action quantity share it, so the same typed count
/// always lowers to the same runtime expression.
pub(in crate::oracle::canonical) fn lower_permanent_count(
    count: &PermanentCountNode,
) -> Option<Value> {
    let predicate = lower_permanent_object_predicate(&count.predicate);
    let mut lowered = match count.measure {
        PermanentCountMeasure::Objects => {
            let mut lowered = json!({ "kind": "countPermanents", "where": predicate });
            match count.controller {
                None => lowered["allPlayers"] = json!(true),
                Some(ObjectControllerScope::AbilityController) => {
                    lowered["player"] = controller();
                }
                Some(_) => return None,
            }
            lowered
        }
        PermanentCountMeasure::DistinctColors => {
            if count.controller != Some(ObjectControllerScope::AbilityController) {
                return None;
            }
            json!({ "kind": "countDistinctColors", "player": controller(), "where": predicate })
        }
        PermanentCountMeasure::DistinctNames => {
            // The engine's name count has no source exclusion.
            if count.controller != Some(ObjectControllerScope::AbilityController)
                || count.exclude_source
            {
                return None;
            }
            json!({ "kind": "countDistinctPermanentNames", "player": controller(), "where": predicate })
        }
    };
    if count.exclude_source {
        lowered["excludeSource"] = json!(true);
    }
    Some(lowered)
}

/// Explicit consumer capability, not a new numeric-expression grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum NumericQueryLoweringSupport {
    Unavailable,
    CastingCost,
}

pub(in crate::oracle::canonical) fn lower_numeric_expression_with_query_support(
    expression: &NumericExpression,
    support: NumericQueryLoweringSupport,
) -> Option<Value> {
    lower_numeric_expression_in_context(expression, None, None, support)
}

fn lower_numeric_expression_in_context(
    expression: &NumericExpression,
    event_binding: Option<EventNumericBinding>,
    target_binding: Option<&dyn Fn(TargetBinding) -> Option<Value>>,
    query_support: NumericQueryLoweringSupport,
) -> Option<Value> {
    match expression {
        NumericExpression::Atom(atom) => Some(lower_numeric_atom(atom.clone())),
        NumericExpression::CountQuery(query) => match query_support {
            NumericQueryLoweringSupport::CastingCost => lower_scoped_object_query_count(query),
            NumericQueryLoweringSupport::Unavailable => None,
        },
        NumericExpression::ObjectProperty {
            object: LocalObjectReference::SourceSelf,
            property,
        } => Some(json!({
            "kind": match property {
                NumericObjectProperty::Power => "powerOf",
                NumericObjectProperty::Toughness => "toughnessOf",
            },
            "object": self_ref(),
        })),
        NumericExpression::CountCounters {
            object: LocalObjectReference::SourceSelf,
            counter,
        } => Some(json!({
            "kind":"countCounters", "object":self_ref(), "counter":counter.as_str(),
        })),
        // Bound per recipient by the enclosing action; never standalone.
        NumericExpression::CountersOnRecipient { .. } => None,
        NumericExpression::PermanentCount(count) => lower_permanent_count(count),
        NumericExpression::SourceCastX => Some(json!({ "kind": "sourceCastXValue" })),
        NumericExpression::EventValue(EventNumericBinding::DamageAmount) => {
            Some(decision_result("damageAmount"))
        }
        NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity) => (event_binding
            == Some(EventNumericBinding::ReplacementQuantity))
        .then(|| json!({ "kind": "eventQuantity" })),
        NumericExpression::CountDistinctCardTypes { zone } => Some(json!({
            "kind": "countDistinctCardTypes",
            "zone": match zone {
                GraveyardScope::Controller => graveyard(controller()),
                GraveyardScope::AllPlayers => json!({ "kind": "anyGraveyard" }),
            },
        })),
        NumericExpression::Add { left, right } => Some(json!({
            "kind": "add",
            "left": lower_numeric_expression_in_context(left, event_binding, target_binding, query_support)?,
            "right": lower_numeric_expression_in_context(right, event_binding, target_binding, query_support)?,
        })),
        NumericExpression::Subtract { left, right } => Some(json!({
            "kind": "subtract",
            "left": lower_numeric_expression_in_context(left, event_binding, target_binding, query_support)?,
            "right": lower_numeric_expression_in_context(right, event_binding, target_binding, query_support)?,
        })),
        NumericExpression::Multiply { left, right } => Some(json!({
            "kind": "multiply",
            "left": lower_numeric_expression_in_context(left, event_binding, target_binding, query_support)?,
            "right": lower_numeric_expression_in_context(right, event_binding, target_binding, query_support)?,
        })),
        NumericExpression::CardsInZone {
            player: LocalPlayerReference::Controller,
            zone: LocalZone::Library,
        } => Some(json!({
            "kind": "countCards",
            "zone": { "kind": "library", "player": controller() },
            "where": Value::Null,
        })),
        NumericExpression::CardsInZone { .. } => None,
        NumericExpression::PlayerFact {
            player: LocalPlayerReference::Controller,
            fact,
        } => Some(json!({
            "kind": match fact {
                PlayerNumericFact::LifeGainedThisTurn => "lifeGainedThisTurn",
                PlayerNumericFact::LifeTotal => "lifeTotal",
            },
            "player": controller(),
        })),
        NumericExpression::PlayerFact { .. } => None,
        NumericExpression::Divide {
            left,
            right,
            rounding,
        } => Some(json!({
            "kind": "divide",
            "left": lower_numeric_expression_in_context(left, event_binding, target_binding, query_support)?,
            "right": lower_numeric_expression_in_context(right, event_binding, target_binding, query_support)?,
            "round": match rounding {
                NumericRounding::Down => "down",
                NumericRounding::Up => "up",
            },
        })),
        NumericExpression::SacrificedCostObjectProperty(property) => {
            Some(decision_result(sacrificed_cost_object_binding(*property)))
        }
        NumericExpression::GreatestPropertyAmong {
            property,
            predicate,
        } => Some(json!({
            "kind": match property {
                NumericObjectProperty::Power => "greatestPower",
                NumericObjectProperty::Toughness => "greatestToughness",
            },
            "player": controller(),
            "where": lower_permanent_object_predicate(predicate),
        })),
        NumericExpression::PowerOf(binding) => Some(json!({
            "kind":"powerOf", "object":target_binding?(*binding)?,
        })),
        NumericExpression::ManaValueOf(binding) => Some(json!({
            "kind":"manaValueOf", "object":target_binding?(*binding)?,
        })),
        NumericExpression::QualifiedTargetProperty {
            object, property, ..
        } => Some(json!({
            "kind":match property {
                SelectorNumericProperty::Power => "powerOf",
                SelectorNumericProperty::Toughness => "toughnessOf",
                SelectorNumericProperty::ManaValue => "manaValueOf",
            }, "object":target_binding?(*object)?,
        })),
    }
}

pub(in crate::oracle::canonical) fn lower_selector_numeric_comparison(
    left: SelectorNumericProperty,
    operator: NumericComparisonOperator,
    right: &NumericExpression,
) -> Option<Value> {
    let left = match left {
        SelectorNumericProperty::ManaValue => json!({
            "kind": "manaValueOf",
            "object": { "kind": "candidate" },
        }),
        SelectorNumericProperty::Power => json!({
            "kind": "powerOf",
            "object": { "kind": "candidate" },
        }),
        SelectorNumericProperty::Toughness => json!({
            "kind": "toughnessOf",
            "object": { "kind": "candidate" },
        }),
    };
    let operator = match operator {
        NumericComparisonOperator::Equal => "==",
        NumericComparisonOperator::LessThanOrEqual => "<=",
        NumericComparisonOperator::GreaterThanOrEqual => ">=",
    };
    Some(compare(operator, left, lower_numeric_expression(right)?))
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;

/// Context supplied only by a parent that owns the original typed target slot.
pub(in crate::oracle::canonical) struct NumericTargetSlot {
    pub binding: TargetBinding,
    pub target_id: String,
    pub selector: TargetSelector,
    pub requires_post_zone_change_lki: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetNumericLoweringError {
    MissingBinding,
    NominalMismatch,
    UnprovedCharacteristicDomain,
    PostZoneChangeLki,
    UnsupportedExpression,
}

/// Preflight validates references without changing the arithmetic recursion.
/// Post-zone reads are proved only for the original battlefield permanent
/// incarnation. Stack/mixed/card domains remain capped; P/T additionally
/// requires the unchanged Creature characteristic-domain guard below.
fn validate_numeric_targets(
    expression: &NumericExpression,
    targets: &[NumericTargetSlot],
) -> Result<(), TargetNumericLoweringError> {
    match expression {
        NumericExpression::PowerOf(binding) | NumericExpression::ManaValueOf(binding) => {
            let target = targets
                .iter()
                .find(|target| target.binding == *binding)
                .ok_or(TargetNumericLoweringError::MissingBinding)?;
            if target.requires_post_zone_change_lki
                && !selector_implies_nominal(
                    &target.selector,
                    &ObjectPredicate::ObjectKind(ObjectKind::Permanent),
                )
            {
                return Err(TargetNumericLoweringError::PostZoneChangeLki);
            }
            if matches!(expression, NumericExpression::PowerOf(_))
                && !selector_implies_nominal(
                    &target.selector,
                    &ObjectPredicate::CardType(ObjectCardType::Creature),
                )
            {
                return Err(TargetNumericLoweringError::UnprovedCharacteristicDomain);
            }
            Ok(())
        }
        NumericExpression::QualifiedTargetProperty {
            object,
            required_nominal,
            property,
        } => {
            let target = targets
                .iter()
                .find(|target| target.binding == *object)
                .ok_or(TargetNumericLoweringError::MissingBinding)?;
            if !selector_implies_nominal(&target.selector, required_nominal) {
                return Err(TargetNumericLoweringError::NominalMismatch);
            }
            if target.requires_post_zone_change_lki
                && !selector_implies_nominal(
                    &target.selector,
                    &ObjectPredicate::ObjectKind(ObjectKind::Permanent),
                )
            {
                return Err(TargetNumericLoweringError::PostZoneChangeLki);
            }
            if matches!(
                property,
                SelectorNumericProperty::Power | SelectorNumericProperty::Toughness
            ) && !selector_implies_nominal(
                &target.selector,
                &ObjectPredicate::CardType(ObjectCardType::Creature),
            ) {
                return Err(TargetNumericLoweringError::UnprovedCharacteristicDomain);
            }
            Ok(())
        }
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            validate_numeric_targets(left, targets)?;
            validate_numeric_targets(right, targets)
        }
        NumericExpression::Atom(NumericAtom::Integer(value)) if i32::try_from(*value).is_err() => {
            Err(TargetNumericLoweringError::UnsupportedExpression)
        }
        _ => Ok(()),
    }
}

pub(in crate::oracle::canonical) fn lower_numeric_expression_for_targets(
    expression: &NumericExpression,
    targets: &[NumericTargetSlot],
) -> Result<Value, TargetNumericLoweringError> {
    validate_numeric_targets(expression, targets)?;
    let resolve = |binding| {
        targets
            .iter()
            .find(|target| target.binding == binding)
            .map(|target| chosen_target(&target.target_id))
    };
    lower_numeric_expression_in_context(
        expression,
        None,
        Some(&resolve),
        NumericQueryLoweringSupport::Unavailable,
    )
    .ok_or(TargetNumericLoweringError::UnsupportedExpression)
}

#[cfg(test)]
#[path = "numeric_target_context_tests.rs"]
mod target_context_tests;

#[cfg(test)]
#[path = "numeric_target_review_tests.rs"]
mod independent_review_tests;
