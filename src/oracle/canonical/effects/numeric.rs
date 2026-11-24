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

/// Compiler-assigned identifier of the objects action `effect_index` of a
/// program affected. The program lowering installs it as that action's `bind`.
/// The pending reference to an earlier action's quantity, rewritten into a
/// `boundValue` by the program lowering that owns the producing action.
pub(in crate::oracle::canonical) const ACTION_RESULT_QUANTITY: &str = "pendingActionResultQuantity";

pub(in crate::oracle::canonical) fn action_result_binding(effect_index: usize) -> String {
    format!("actionResult:{effect_index}")
}

/// The pending count of the objects the action before a condition affected
/// (`a land card was milled this way`), rewritten by
/// `bind_action_result_objects` where the program lowers that condition.
pub(in crate::oracle::canonical) const ACTION_RESULT_OBJECTS: &str = "pendingActionResultObjects";

/// The pending count of a restated `OBJECTS VERBed this way`. A restated
/// noun is re-read on the bound objects only when the verb's action keeps
/// card characteristics; otherwise only the generic noun is accepted.
pub(in crate::oracle::canonical) fn lower_action_result_objects(
    reference: &ActionResultReferenceNode,
) -> Result<Value, &'static str> {
    let generic = matches!(
        reference.predicate,
        ObjectPredicate::ObjectKind(ObjectKind::Card | ObjectKind::Permanent)
    );
    let filter = if generic {
        Value::Null
    } else if reference.verb.preserves_card_characteristics()
        && reference.predicate.is_in_domain(ObjectKind::Card)
    {
        lower_card_object_predicate(&reference.predicate)
    } else {
        return Err("a restated noun the producing action guarantees or keeps readable");
    };
    Ok(json!({
        "kind": ACTION_RESULT_OBJECTS,
        "verb": reference.verb.ir_name(),
        "where": filter,
    }))
}

/// The objects one producing runtime action affected, as a stable object
/// expression: its existing `bind`, the already stable card set a reveal
/// names (`revealCards` of bound, chosen or targeted cards), or a `bind`
/// installed now under the compiler-assigned identifier of `effect_index`.
/// A reveal of a set that is re-evaluated later (the library's top cards)
/// has no stable result and is refused.
pub(in crate::oracle::canonical) fn action_result_objects(
    producer: &mut Value,
    effect_index: usize,
) -> Result<Value, &'static str> {
    if let Some(existing) = producer.get("bind").and_then(Value::as_str) {
        return Ok(json!({ "kind": "boundObjects", "binding": existing }));
    }
    if producer["kind"].as_str() == Some("revealCards") {
        return matches!(
            producer["cards"]["kind"].as_str(),
            Some("boundObjects" | "decisionResult" | "chosenTargets")
        )
        .then(|| producer["cards"].clone())
        .ok_or("a revealed card set the program bound or chose");
    }
    let binding = action_result_binding(effect_index);
    producer["bind"] = json!(binding);
    Ok(json!({ "kind": "boundObjects", "binding": binding }))
}

/// Replaces every `boundObjects` reference to `binding` under `value`.
pub(in crate::oracle::canonical) fn rewrite_bound_objects(
    value: &mut Value,
    binding: &str,
    replacement: &Value,
) {
    match value {
        Value::Object(map)
            if map.get("kind").and_then(Value::as_str) == Some("boundObjects")
                && map.get("binding").and_then(Value::as_str) == Some(binding) =>
        {
            *value = replacement.clone();
        }
        Value::Object(map) => map
            .values_mut()
            .for_each(|child| rewrite_bound_objects(child, binding, replacement)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| rewrite_bound_objects(child, binding, replacement)),
        _ => {}
    }
}

/// Binds every pending `OBJECTS VERBed this way` count under `value` to the
/// action immediately before action `reader` (CR 608.2c): its lowering must
/// contain exactly one top-level runtime action of the restated verb, whose
/// `bind` records the affected objects (an existing binding is reused).
/// Anything else is an explicit failure, never a count of zero.
pub(in crate::oracle::canonical) fn bind_action_result_objects(
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
    reader: usize,
    value: &mut Value,
) -> Result<(), &'static str> {
    bind_pending_action_result_objects(action_starts, effects, Some(reader), value)
}

/// Binds every pending `the number of OBJECTS VERBed this way` that records
/// its own reader action (a count inside an action's arguments). Each
/// top-level action is rebound on a copy, so the producer it binds (always
/// an earlier action) is updated in place.
pub(in crate::oracle::canonical) fn install_action_result_object_counts(
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
) -> Result<(), &'static str> {
    for position in 0..effects.len() {
        let mut effect = effects[position].clone();
        bind_pending_action_result_objects(action_starts, effects, None, &mut effect)?;
        effects[position] = effect;
    }
    Ok(())
}

fn bind_pending_action_result_objects(
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
    reader: Option<usize>,
    value: &mut Value,
) -> Result<(), &'static str> {
    match value {
        Value::Object(map)
            if map.get("kind").and_then(Value::as_str) == Some(ACTION_RESULT_OBJECTS) =>
        {
            let verb = map
                .get("verb")
                .and_then(Value::as_str)
                .and_then(ActionResultVerb::from_ir_name)
                .ok_or("a known restated verb")?;
            let reader = map
                .get("reader")
                .and_then(Value::as_u64)
                .and_then(|reader| usize::try_from(reader).ok())
                .or(reader)
                .ok_or("the action that reads the restated objects")?;
            // The nearest earlier action whose lowering runs the verb.
            let producer = (0..reader).rev().find_map(|index| {
                let (from, _) = *action_starts.get(index)?;
                // The last lowered action ends with the program lowered so far.
                let to = action_starts
                    .get(index + 1)
                    .map_or(effects.len(), |(to, _)| *to);
                let producers = (from..to)
                    .filter(|position| {
                        effects[*position]["kind"]
                            .as_str()
                            .is_some_and(|kind| verb.runtime_kinds().contains(&kind))
                    })
                    .collect::<Vec<_>>();
                (!producers.is_empty()).then_some((index, producers))
            });
            let (previous, producers) =
                producer.ok_or("an earlier action of this program that the phrase restates")?;
            let [position] = producers.as_slice() else {
                return Err(
                    "one preceding top-level runtime action recording the restated objects",
                );
            };
            let bound = action_result_objects(&mut effects[*position], previous)?;
            let filter = map.get("where").cloned().unwrap_or(Value::Null);
            *value = json!({
                "kind": "countObjects",
                "objects": if filter.is_null() {
                    bound
                } else {
                    json!({ "kind": "filterObjects", "objects": bound, "where": filter })
                },
            });
            Ok(())
        }
        Value::Object(map) => map.values_mut().try_for_each(|child| {
            bind_pending_action_result_objects(action_starts, effects, reader, child)
        }),
        Value::Array(items) => items.iter_mut().try_for_each(|child| {
            bind_pending_action_result_objects(action_starts, effects, reader, child)
        }),
        _ => Ok(()),
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
            match count.controller {
                None => {
                    json!({ "kind": "countPermanents", "where": predicate, "allPlayers": true })
                }
                Some(ObjectControllerScope::AbilityController) => {
                    json!({ "kind": "countPermanents", "where": predicate, "player": controller() })
                }
                // `your opponents control`: the engine's opponents-controlled
                // count, which has no source exclusion.
                Some(ObjectControllerScope::OpponentOfAbilityController)
                    if !count.exclude_source =>
                {
                    return Some(json!({
                        "kind": "countPermanentsControlledByOpponents",
                        "player": controller(),
                        "where": predicate,
                    }));
                }
                // `target opponent controls`, `its controller controls`:
                // the permanents of that one player.
                Some(ObjectControllerScope::Player(player)) => json!({
                    "kind": "countPermanents",
                    "where": predicate,
                    "player": lower_related_player(player),
                }),
                // `Swamps they control` after an event naming a player: the
                // player the triggering event records (CR 603.2).
                Some(ObjectControllerScope::EventPlayer) => json!({
                    "kind": "countPermanents",
                    "where": predicate,
                    "player": { "kind": "triggeringPlayer" },
                }),
                // `you don't control`: no engine aggregate counts for that
                // player relation yet.
                Some(_) => return None,
            }
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
        PermanentCountMeasure::DistinctBasicLandTypes => {
            // The engine's Domain count reads every land its player controls.
            if count.controller != Some(ObjectControllerScope::AbilityController)
                || count.exclude_source
                || count.predicate != ObjectPredicate::CardType(ObjectCardType::Land)
            {
                return None;
            }
            return Some(json!({ "kind": "domainCount", "player": controller() }));
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
            NumericQueryLoweringSupport::Unavailable => lower_scoped_object_query_amount(query),
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
        NumericExpression::ColorsOfManaSpentToCastSource => {
            Some(json!({ "kind": "colorsOfManaSpentToCastSource" }))
        }
        NumericExpression::EventValue(EventNumericBinding::DamageAmount) => {
            Some(decision_result("damageAmount"))
        }
        // The amounts the runtime records with each triggering occurrence.
        NumericExpression::EventValue(EventNumericBinding::LifeGained) => {
            Some(decision_result("lifeGainedAmount"))
        }
        NumericExpression::EventValue(EventNumericBinding::LifeLost) => {
            Some(decision_result("lifeLostAmount"))
        }
        NumericExpression::EventValue(EventNumericBinding::CountersPut) => {
            Some(decision_result("counterCount"))
        }
        NumericExpression::EventValue(EventNumericBinding::EventObjectCount) => {
            Some(decision_result("eventObjectCount"))
        }
        NumericExpression::EventValue(EventNumericBinding::CastSpellManaValue) => {
            Some(json!({ "kind": "triggeringSpellManaValue" }))
        }
        NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity) => (event_binding
            == Some(EventNumericBinding::ReplacementQuantity))
        .then(|| json!({ "kind": "eventQuantity" })),
        // Resolved and bound by the program lowering (see
        // `install_action_result_quantities`); left unresolved it is no
        // runtime expression and is refused.
        // `the number of cards revealed this way`: bound by the program
        // lowering to the nearest earlier action of the restated verb.
        NumericExpression::ActionResultQuantity {
            reader_effect,
            quantity: ActionResultQuantity::Objects(reference),
        } => {
            let mut pending = lower_action_result_objects(reference).ok()?;
            pending["reader"] = json!(reader_effect);
            Some(pending)
        }
        NumericExpression::ActionResultQuantity {
            reader_effect,
            quantity,
        } => Some(json!({
            "kind": ACTION_RESULT_QUANTITY,
            "reader": reader_effect,
            "quantity": quantity.ir_name(),
        })),
        NumericExpression::Devotion(color) => Some(json!({
            "kind": "devotion",
            "player": controller(),
            "color": canonical_color_name(*color),
        })),
        NumericExpression::EventHistoryCount(node) => Some(lower_event_history_count(
            &node.pattern,
            node.scope,
            node.aggregate,
        )),
        NumericExpression::OpponentCount(OpponentFact::AttackedByControllerThisCombat) => {
            Some(json!({ "kind": "countOpponentsAttackedThisCombat", "player": controller() }))
        }
        NumericExpression::OpponentCount(OpponentFact::Any) => {
            Some(json!({ "kind": "countOpponents", "player": controller() }))
        }
        NumericExpression::CountDistinctCardTypes { zone, predicate } => {
            let mut count = json!({
                "kind": "countDistinctCardTypes",
                "zone": match zone {
                    GraveyardScope::Controller => graveyard(controller()),
                    GraveyardScope::AllPlayers => json!({ "kind": "anyGraveyard" }),
                },
            });
            let filter = lower_card_object_predicate(predicate);
            if !filter.is_null() {
                count["where"] = filter;
            }
            Some(count)
        }
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
        NumericExpression::ActionResultCount {
            effect_index,
            filter,
        } => {
            let bound = json!({
                "kind": "boundObjects",
                "binding": action_result_binding(*effect_index),
            });
            Some(json!({
                "kind": "countObjects",
                "objects": match filter {
                    ActionResultFilter::All => bound,
                    ActionResultFilter::Cards(predicate) => json!({
                        "kind": "filterObjects",
                        "objects": bound,
                        "where": lower_card_object_predicate(predicate),
                    }),
                },
            }))
        }
        NumericExpression::PlayerFact {
            player: LocalPlayerReference::Controller,
            fact: PlayerNumericFact::Counters(counter),
        } => Some(json!({
            "kind": "countPlayerCounters",
            "player": controller(),
            "counter": counter.as_str(),
        })),
        NumericExpression::PlayerFact {
            player: LocalPlayerReference::Controller,
            fact,
        } => Some(json!({
            "kind": match fact {
                PlayerNumericFact::LifeGainedThisTurn => "lifeGainedThisTurn",
                PlayerNumericFact::LifeTotal => "lifeTotal",
                PlayerNumericFact::Counters(_) => unreachable!("lowered above"),
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
            aggregate: PropertyAggregate::Sum,
        } => match property {
            // The engine sums the current power of the controller's
            // permanents; no other property has a sum aggregate yet.
            SelectorNumericProperty::Power => Some(json!({
                "kind": "sumPowers",
                "player": controller(),
                "where": lower_permanent_object_predicate(predicate),
            })),
            SelectorNumericProperty::Toughness | SelectorNumericProperty::ManaValue => None,
        },
        NumericExpression::GreatestPropertyAmong {
            property,
            predicate,
            aggregate: PropertyAggregate::Greatest,
        } => Some(json!({
            "kind": match property {
                SelectorNumericProperty::Power => "greatestPower",
                SelectorNumericProperty::Toughness => "greatestToughness",
                SelectorNumericProperty::ManaValue => "greatestManaValue",
            },
            "player": controller(),
            "where": lower_permanent_object_predicate(predicate),
        })),
        // Only references that need no declared slot have a value here.
        NumericExpression::BoundObjectProperty { object, property } => Some(json!({
            "kind": match property {
                SelectorNumericProperty::Power => "powerOf",
                SelectorNumericProperty::Toughness => "toughnessOf",
                SelectorNumericProperty::ManaValue => "manaValueOf",
            },
            "object": bound_permanent_reference_value(*object, &[])?,
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
    Some(compare(
        operator.ir_operator(),
        left,
        lower_numeric_expression(right)?,
    ))
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

/// A target slot whose power and toughness are a creature's: the slot is
/// guaranteed to be a creature, or every branch names a creature subtype
/// head (`target Dinosaur you control`), whose permanent is a creature or a
/// Kindred permanent without power (CR 208.3) — never another value.
fn selector_reads_creature_power_toughness(selector: &TargetSelector) -> bool {
    if selector_implies_nominal(
        selector,
        &ObjectPredicate::CardType(ObjectCardType::Creature),
    ) {
        return true;
    }
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate.has_creature_subtype_head()
                && predicate.supports_domain(ObjectKind::Permanent)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(selector_reads_creature_power_toughness)
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
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
                && !selector_reads_creature_power_toughness(&target.selector)
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
            ) && !selector_reads_creature_power_toughness(&target.selector)
            {
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
