//! Shared lookup and current-state evaluation of declared stack targets.
//!
//! Resolution revalidates each target independently against the declaration's
//! candidate domain. Retargeting additionally validates constraints that apply
//! to the complete selection.

use serde_json::Value;
use std::collections::BTreeMap;

use super::{
    CardInstance, CastingDeclaration, GameEngine, StackObject, TargetRef, mana_value,
    matches_card_filter_with_decisions, value_kind,
};

#[path = "target_legality_joint.rs"]
mod joint;

#[derive(Clone, Copy)]
struct TargetSlot<'a> {
    rule: &'a Value,
    decision: &'a Value,
    declaration_id: &'a str,
}

fn target_ordinal(declaration_id: &str, target_id: &str) -> Option<Option<usize>> {
    if target_id == declaration_id {
        return Some(None);
    }
    let suffix = target_id.strip_prefix(declaration_id)?.strip_prefix(':')?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse::<usize>().ok().map(Some)
}

fn declaration_rules(stack: &StackObject) -> Vec<&Value> {
    stack.ability_rule.as_ref().map_or_else(
        || {
            stack
                .card
                .definition
                .printed_rules()
                .iter()
                .filter(|rule| value_kind(rule) == Some("spellAbility"))
                .collect()
        },
        |rule| vec![rule],
    )
}

fn declared_slots<'a>(stack: &'a StackObject, target_id: &str) -> Vec<TargetSlot<'a>> {
    let mut slots = declaration_rules(stack)
        .into_iter()
        .flat_map(|rule| {
            rule["declaration"]["decisions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |decision| (rule, decision))
        })
        .filter(|(_, decision)| value_kind(decision) == Some("chooseTargets"))
        .filter_map(|(rule, decision)| {
            let declaration_id = decision["id"].as_str()?;
            target_ordinal(declaration_id, target_id)?;
            Some(TargetSlot {
                rule,
                decision,
                declaration_id,
            })
        })
        .collect::<Vec<_>>();
    let longest = slots.iter().map(|slot| slot.declaration_id.len()).max();
    slots.retain(|slot| Some(slot.declaration_id.len()) == longest);
    slots
}

/// Match a committed member to its owning declaration using the same longest
/// binding rule as legality/retargeting. Explicit legacy map-only designations
/// retain their established ordinal semantics; they do not prove an empty set.
pub(super) fn target_belongs_to_declaration(
    stack: &StackObject,
    declaration_id: &str,
    target_id: &str,
) -> bool {
    target_ordinal(declaration_id, target_id).is_some()
        && declared_slots(stack, target_id)
            .iter()
            .all(|slot| slot.declaration_id == declaration_id)
}

/// The chooseTargets slot ids a rule declares.
fn declared_target_slot_ids(rule: &Value) -> Vec<&str> {
    rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|decision| value_kind(decision) == Some("chooseTargets"))
        .filter_map(|decision| decision["id"].as_str())
        .collect()
}

/// The slot of `rule` that owns the target key `target_id`: the longest
/// declared id the key names, bare or with an ordinal (`mode1:target`,
/// `mode1:target:0`), as `target_ordinal` reads it.
pub(super) fn owning_target_slot<'a>(rule: &'a Value, target_id: &str) -> Option<&'a str> {
    declared_target_slot_ids(rule)
        .into_iter()
        .filter(|slot| target_ordinal(slot, target_id).is_some())
        .max_by_key(|slot| slot.len())
}

/// The `distinctFrom` relations of a rule: each later slot with the earlier
/// slots whose objects it must not name.
pub(super) fn distinct_slot_relations(rule: &Value) -> Vec<(&str, Vec<&str>)> {
    rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|decision| {
            Some((
                decision["id"].as_str()?,
                decision["distinctFrom"]
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>(),
            ))
        })
        .collect()
}

/// A rule's distinct target relations (CR 115.3): a slot's `distinctFrom`
/// names earlier slots whose objects it must not name, compared by owning
/// slot so that any number of chosen objects is checked. Legacy
/// `distinctTargets` groups name exact target ids.
pub(super) fn declaration_respects_distinct_targets(
    rule: &Value,
    targets: &BTreeMap<String, TargetRef>,
) -> bool {
    let slot_objects = |slot: &str| {
        targets
            .iter()
            .filter(|(target_id, _)| owning_target_slot(rule, target_id) == Some(slot))
            .map(|(_, target)| target)
            .collect::<Vec<_>>()
    };
    let typed = distinct_slot_relations(rule)
        .iter()
        .all(|(later, earlier_slots)| {
            let later_objects = slot_objects(later);
            earlier_slots.iter().all(|earlier| {
                slot_objects(earlier)
                    .iter()
                    .all(|object| !later_objects.contains(object))
            })
        });
    let legacy = rule["distinctTargets"]
        .as_array()
        .into_iter()
        .flatten()
        .all(|group| {
            let values = group
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|id| targets.get(id))
                .collect::<Vec<_>>();
            values
                .iter()
                .enumerate()
                .all(|(index, value)| !values[..index].contains(value))
        });
    typed && legacy
}

/// Whether `rule` declared every target `targets` names: the rule that
/// produced the declaration (a card's other spell rules do not judge it).
pub(super) fn rule_declares_targets(rule: &Value, targets: &BTreeMap<String, TargetRef>) -> bool {
    targets
        .keys()
        .all(|target_id| owning_target_slot(rule, target_id).is_some())
}

/// A committed empty selection is not a missing target producer. Reuse the
/// same declaration rules as legality/retargeting and require the recorded
/// decision, rather than guessing from an absent map entry.
pub(super) fn declared_unselected_target_ids(
    stack: &StackObject,
) -> std::collections::BTreeSet<String> {
    declaration_rules(stack)
        .into_iter()
        .flat_map(|rule| {
            rule["declaration"]["decisions"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter(|decision| value_kind(decision) == Some("chooseTargets"))
        // A literal required selection cannot be authenticated by an empty
        // receipt. For dynamic bounds trust the already committed selection;
        // never re-evaluate a world-sensitive casting minimum at resolution.
        .filter(
            |decision| match super::integer_value(&decision["minimum"]) {
                Some(minimum) => minimum == 0,
                None => super::numeric_expression_supported(&decision["minimum"]),
            },
        )
        .filter_map(|decision| decision["id"].as_str())
        .filter(|id| {
            stack
                .decisions
                .get(*id)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        })
        .filter(|id| {
            !stack
                .targets
                .keys()
                .any(|key| target_belongs_to_declaration(stack, id, key))
        })
        .map(str::to_owned)
        .collect()
}

fn source_context<'a>(engine: &'a GameEngine, stack: &'a StackObject) -> &'a CardInstance {
    let live =
        engine
            .permanent_position(&stack.card.instance_id)
            .map(|(player_index, card_index)| {
                &engine.state.players[player_index].battlefield[card_index]
            });
    super::source_lki::source_object_input(stack, live).card
}

/// Resolved context is produced by contextualize_candidates; historical raw
/// callers retain their established controller-relative shorthand.
pub(super) fn candidate_controller_matches(candidates: &Value, actual: &str, actor: &str) -> bool {
    if let Some(players) = candidates.get("resolvedControllerIds") {
        return players
            .as_array()
            .is_some_and(|players| players.iter().any(|id| id.as_str() == Some(actual)));
    }
    candidates
        .get("controller")
        .is_none_or(|reference| match value_kind(reference) {
            Some("controllerOf") => actual == actor,
            Some("opponentsOf" | "playersExcept") => actual != actor,
            _ => false,
        })
}

fn contextualize_candidates(
    engine: &GameEngine,
    stack: &StackObject,
    candidates: &mut Value,
    source: &CardInstance,
) -> Option<()> {
    engine.bind_source_candidates(candidates, source)?;
    if let Some(reference) = candidates.get("controller") {
        let players = match value_kind(reference) {
            Some("eachPlayer") => engine
                .state
                .players
                .iter()
                .filter(|p| !p.has_lost)
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            Some("opponentsOf" | "playersExcept") => {
                let actor =
                    engine.resolve_player(&reference["player"], stack, &Default::default())?;
                engine
                    .state
                    .players
                    .iter()
                    .filter(|p| !p.has_lost && p.id != actor)
                    .map(|p| p.id.clone())
                    .collect()
            }
            Some("randomPlayer" | "randomOpponentOf") => return None,
            _ => vec![engine.resolve_player(reference, stack, &Default::default())?],
        };
        candidates["resolvedControllerIds"] =
            Value::Array(players.into_iter().map(Value::String).collect());
    }
    if candidates["excludeSource"].as_bool() == Some(true) {
        candidates["sourceInstanceId"] = Value::String(source.instance_id.clone());
    }
    if value_kind(&candidates["exclude"]) == Some("attachedPermanent")
        && value_kind(&candidates["exclude"]["attachment"]) == Some("self")
    {
        if let Some(attached_id) = &source.attached_to {
            candidates["excludeInstanceId"] = Value::String(attached_id.clone());
        }
        candidates.as_object_mut()?.remove("exclude");
    }
    if value_kind(candidates) == Some("union") {
        for set in candidates["sets"].as_array_mut()? {
            contextualize_candidates(engine, stack, set, source)?;
        }
    }
    if value_kind(candidates) == Some("cards")
        && !matches!(value_kind(&candidates["zone"]), Some("anyGraveyard"))
        && candidates["zone"].get("player").is_some()
    {
        let player_id =
            engine.resolve_player(&candidates["zone"]["player"], stack, &Default::default())?;
        candidates["resolvedZonePlayerId"] = Value::String(player_id);
    }
    Some(())
}

fn selection_with_replacement(
    stack: &StackObject,
    declaration_id: &str,
    replacement_id: &str,
    replacement: &TargetRef,
) -> Vec<TargetRef> {
    let mut selected = stack
        .targets
        .iter()
        .filter_map(|(target_id, target)| {
            target_ordinal(declaration_id, target_id).map(|ordinal| {
                (
                    ordinal.unwrap_or(0),
                    if target_id == replacement_id {
                        replacement.clone()
                    } else {
                        target.clone()
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|(ordinal, _)| *ordinal);
    selected.into_iter().map(|(_, target)| target).collect()
}

impl GameEngine {
    pub(super) fn permanent_matches_target_filter_with_decisions(
        &self,
        permanent: &CardInstance,
        filter: &Value,
        controller_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        match value_kind(filter) {
            Some("or") => filter["operands"].as_array().is_some_and(|operands| {
                operands.iter().any(|operand| {
                    self.permanent_matches_target_filter_with_decisions(
                        permanent,
                        operand,
                        controller_id,
                        decisions,
                    )
                })
            }),
            Some("and") => filter["operands"].as_array().is_some_and(|operands| {
                operands.iter().all(|operand| {
                    self.permanent_matches_target_filter_with_decisions(
                        permanent,
                        operand,
                        controller_id,
                        decisions,
                    )
                })
            }),
            Some("not") => !self.permanent_matches_target_filter_with_decisions(
                permanent,
                &filter["operand"],
                controller_id,
                decisions,
            ),
            Some("compare")
                if matches!(
                    value_kind(&filter["right"]),
                    Some("sourceCastXValue" | "decisionResult")
                ) =>
            {
                let decision_id = if value_kind(&filter["right"]) == Some("sourceCastXValue") {
                    Some("xValue")
                } else {
                    filter["right"]["decisionId"].as_str()
                };
                let Some(right) = decision_id
                    .and_then(|id| decisions.get(id))
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                else {
                    return false;
                };
                let left = match value_kind(&filter["left"]) {
                    Some("manaValueOf") => mana_value(&permanent.definition),
                    Some("powerOf") => self.current_power(permanent),
                    Some("toughnessOf") => self.current_toughness(permanent),
                    Some("colorCountOf") => self.permanent_color_symbols(permanent).len() as i32,
                    _ => return false,
                };
                match filter["operator"].as_str() {
                    Some(">=") => left >= right,
                    Some(">") => left > right,
                    Some("<=") => left <= right,
                    Some("<") => left < right,
                    Some("==") => left == right,
                    _ => false,
                }
            }
            Some("manaValueParity" | "chosenCreatureType") => matches_card_filter_with_decisions(
                super::object_predicate::DefinitionView::for_permanent(permanent),
                filter,
                decisions,
            ),
            _ => self.permanent_matches_target_filter(permanent, filter, controller_id),
        }
    }

    /// Returns the current individual candidate domain for an explicitly
    /// declared target. `None` means there is no reliably evaluable explicit
    /// declaration, so callers preserve the pre-existing implicit-target path.
    pub(super) fn declared_stack_target_candidates(
        &self,
        stack: &StackObject,
        target_id: &str,
    ) -> Option<Vec<TargetRef>> {
        let slots = declared_slots(stack, target_id);
        if slots.is_empty() {
            return None;
        }
        let source = source_context(self, stack);
        let mut resolved_any = false;
        let mut result = Vec::new();
        for slot in slots {
            let was_committed = stack
                .targets
                .keys()
                .any(|target_id| target_ordinal(slot.declaration_id, target_id).is_some());
            let active = was_committed
                || slot.decision.get("condition").is_none_or(|condition| {
                    self.condition_value_with_source(
                        condition,
                        &stack.controller,
                        &stack.decisions,
                        Some(source),
                    )
                });
            if !active {
                resolved_any = true;
                continue;
            }
            let mut candidates = slot.decision["candidates"].clone();
            if contextualize_candidates(self, stack, &mut candidates, source).is_none() {
                continue;
            }
            let Some(current) = self.target_candidates_with_decisions(
                &stack.controller,
                &candidates,
                &stack.decisions,
            ) else {
                continue;
            };
            resolved_any = true;
            for candidate in current {
                if !result.contains(&candidate) {
                    result.push(candidate);
                }
            }
        }
        resolved_any.then_some(result)
    }

    fn replacement_preserves_joint_target_constraints(
        &self,
        stack: &StackObject,
        slot: TargetSlot<'_>,
        target_id: &str,
        candidate: &TargetRef,
    ) -> bool {
        let mut assignment = CastingDeclaration {
            decisions: stack.decisions.clone(),
            targets: stack.targets.clone(),
            target_order: stack.targets.keys().cloned().collect(),
        };
        assignment
            .targets
            .insert(target_id.to_string(), candidate.clone());
        let selection =
            selection_with_replacement(stack, slot.declaration_id, target_id, candidate);
        assignment.decisions.insert(
            slot.declaration_id.to_string(),
            Value::Array(
                selection
                    .iter()
                    .cloned()
                    .filter_map(|target| serde_json::to_value(target).ok())
                    .collect(),
            ),
        );
        let selection_is_valid =
            slot.decision
                .get("selectionConstraint")
                .is_none_or(|constraint| {
                    self.target_selection_satisfies_constraint(&selection, constraint, &assignment)
                });
        let distinct_targets_are_valid =
            declaration_respects_distinct_targets(slot.rule, &assignment.targets);
        selection_is_valid && distinct_targets_are_valid
    }

    /// Candidate domain for choosing a replacement target. Unlike resolution,
    /// a new choice must leave the complete target selection valid.
    pub(super) fn retarget_candidates_for_stack_target(
        &self,
        stack: &StackObject,
        target_id: &str,
    ) -> Vec<TargetRef> {
        let slots = declared_slots(stack, target_id);
        let Some(candidates) = self.declared_stack_target_candidates(stack, target_id) else {
            return Vec::new();
        };
        candidates
            .into_iter()
            .filter(|candidate| {
                slots.iter().copied().any(|slot| {
                    self.replacement_preserves_joint_target_constraints(
                        stack, slot, target_id, candidate,
                    )
                })
            })
            .collect()
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "target_legality_tests.rs"]
mod tests;
