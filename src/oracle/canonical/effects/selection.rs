use super::super::*;

pub(in crate::oracle::canonical) fn lower_object_controller_scope(
    scope: ObjectControllerScope,
) -> Value {
    match scope {
        ObjectControllerScope::AbilityController => controller(),
        ObjectControllerScope::NotAbilityController => json!({
            "kind": "playersExcept",
            "player": controller(),
        }),
        ObjectControllerScope::OpponentOfAbilityController => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
        // The player the triggering event records (CR 506.2 defending
        // player), the same reference a `defending player` subject lowers to.
        ObjectControllerScope::EventPlayer => json!({ "kind": "triggeringPlayer" }),
        ObjectControllerScope::Player(player) => lower_related_player(player),
    }
}

/// Lowers one target-selector branch into the engine's generic candidate-set
/// vocabulary. Cross-domain unions remain one target slot whose candidates
/// are the union of independently lowered sets.
pub(in crate::oracle::canonical) fn lower_target_selector_candidates(
    selector: &TargetSelector,
) -> Option<(&'static str, Value)> {
    Some(match selector {
        TargetSelector::Any => ("targetDamageable", json!({ "kind": "anyTarget" })),
        TargetSelector::Union(branches) => {
            if branches.is_empty() {
                return None;
            }
            let sets = branches
                .iter()
                .map(|branch| lower_target_selector_candidates(branch).map(|(_, set)| set))
                .collect::<Option<Vec<_>>>()?;
            ("targetDamageable", json!({ "kind": "union", "sets": sets }))
        }
        TargetSelector::Player(PlayerPredicate::Any) => {
            ("targetPlayer", json!({ "kind": "players" }))
        }
        TargetSelector::Player(PlayerPredicate::Opponent) => (
            "targetOpponent",
            json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
            }),
        ),
        TargetSelector::Object(predicate) => {
            ("targetPermanent", lower_permanent_collection(predicate)?)
        }
        TargetSelector::ControlledObject {
            predicate,
            controller: controller_scope,
        } => {
            let mut candidates = lower_permanent_collection(predicate)?;
            candidates["controller"] = lower_object_controller_scope(*controller_scope);
            ("targetPermanent", candidates)
        }
        TargetSelector::StackItem(_) => return None,
    })
}

/// Lowers the complete target slot, including restrictions carried by its
/// determiner. Parents must not drop `another` when delegating the predicate.
pub(in crate::oracle::canonical) fn lower_target_slot_candidates(
    slot: &TargetSlot,
) -> Option<(&'static str, Value)> {
    let (id, mut candidates) = lower_target_selector_candidates(&slot.selector)?;
    if slot.exclude_source {
        crate::oracle::canonical::automata::mark_other_target_slot(&mut candidates);
    }
    Some((id, candidates))
}

pub(in crate::oracle::canonical) fn target_decision(
    id: &str,
    candidates: Value,
    minimum: i64,
    maximum: i64,
) -> Value {
    json!({
        "id": id,
        "kind": "chooseTargets",
        "minimum": minimum,
        "maximum": maximum,
        "candidates": candidates,
    })
}

pub(in crate::oracle::canonical) fn chosen_target(id: &str) -> Value {
    json!({ "kind": "chosenTarget", "id": id })
}

pub(in crate::oracle::canonical) fn decision_result(id: &str) -> Value {
    json!({ "kind": "decisionResult", "decisionId": id })
}

pub(in crate::oracle::canonical) fn bound_objects(binding: &str) -> Value {
    json!({ "kind": "boundObjects", "binding": binding })
}

pub(in crate::oracle::canonical) fn library(player: Value) -> Value {
    json!({ "kind": "library", "player": player })
}

pub(in crate::oracle::canonical) fn graveyard(player: Value) -> Value {
    json!({ "kind": "graveyard", "player": player })
}

pub(in crate::oracle::canonical) fn lower_collection_binding(binding: CollectionBinding) -> String {
    match binding.0 {
        0 => "searchedCards".to_string(),
        index => format!("searchedCards:{index}"),
    }
}

#[cfg(test)]
#[path = "target_slot_lowering_tests.rs"]
mod target_slot_lowering_tests;

/// Shared spell candidate construction; counter consumers retain their exact
/// omission of an unconstrained `where` field and their established target IDs.
pub(in crate::oracle::canonical) fn lower_spell_target_candidates(
    predicate: &ObjectPredicate,
    scope: Option<ObjectControllerScope>,
) -> Value {
    let mut candidates = json!({"kind":"spells"});
    if let Some(scope) = scope {
        candidates["controller"] = lower_object_controller_scope(scope);
    }
    if let Some(filter) = lower_spell_object_predicate(predicate) {
        candidates["where"] = filter;
    }
    candidates
}

fn lower_projected_return_selector(selector: &TargetSelector) -> Option<(&'static str, Value)> {
    match selector {
        TargetSelector::Union(branches) => {
            if branches.is_empty() {
                return None;
            }
            let sets = branches
                .iter()
                .map(|branch| {
                    lower_projected_return_selector(branch).map(|(_, candidates)| candidates)
                })
                .collect::<Option<Vec<_>>>()?;
            Some(("targetObject", json!({"kind":"union", "sets":sets})))
        }
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            let scope = match selector {
                TargetSelector::ControlledObject { controller, .. } => Some(*controller),
                _ => None,
            };
            let projected = project_object_query_domains(
                predicate,
                &[ObjectKind::Spell, ObjectKind::Permanent],
            )
            .ok()?;
            let mut sets = Vec::new();
            let mut only_id = "targetObject";
            for branch in projected {
                let candidates = match branch.domain {
                    ObjectKind::Spell => {
                        if branch.predicate.contains_numeric_comparison() {
                            return None;
                        }
                        only_id = "targetSpell";
                        lower_spell_target_candidates(&branch.predicate, scope)
                    }
                    ObjectKind::Permanent => {
                        only_id = "targetPermanent";
                        let mut candidates = json!({"kind":"permanents", "where":lower_permanent_object_predicate(&branch.predicate)});
                        if let Some(scope) = scope {
                            candidates["controller"] = lower_object_controller_scope(scope);
                        }
                        candidates
                    }
                    ObjectKind::Card => return None,
                };
                sets.push(candidates);
            }
            if sets.len() == 1 {
                Some((only_id, sets.pop()?))
            } else {
                Some(("targetObject", json!({"kind":"union", "sets":sets})))
            }
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => None,
    }
}

/// Policy-aware candidate lowering for a zone-change parent. Unsupported
/// targets return None; callers turn this into a typed support diagnostic.
pub(in crate::oracle::canonical) fn lower_zone_change_target_candidates(
    effect: &ZoneChangeEffectNode,
) -> Option<(&'static str, Value, i64, i64)> {
    let [slot] = effect.target.slots.as_slice() else {
        return None;
    };
    lower_zone_change_slot_candidates(effect, slot)
}

/// Candidates and bounds of one slot of a zone-change target list. Only the
/// sole scalar slot of a return may project the spell/permanent union.
pub(in crate::oracle::canonical) fn lower_zone_change_slot_candidates(
    effect: &ZoneChangeEffectNode,
    slot: &TargetSlot,
) -> Option<(&'static str, Value, i64, i64)> {
    let (id, mut candidates) = if slot.selector.is_permanent_target() {
        lower_target_selector_candidates(&slot.selector)?
    } else if effect.action == ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand)
        && effect.target.slots.len() == 1
        && slot.cardinality.maximum == 1
    {
        validate_spell_or_permanent_target(&slot.selector).ok()?;
        lower_projected_return_selector(&slot.selector)?
    } else {
        return None;
    };
    if slot.exclude_source {
        crate::oracle::canonical::automata::mark_other_target_slot(&mut candidates);
    }
    Some((
        id,
        candidates,
        i64::from(slot.cardinality.minimum),
        i64::from(slot.cardinality.maximum),
    ))
}

#[cfg(test)]
#[path = "return_target_domain_tests.rs"]
mod return_target_domain_tests;
