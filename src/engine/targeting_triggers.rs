//! Generic `becameTarget` event dispatch.
//!
//! Target declarations are already represented as typed [`TargetRef`] values.
//! This module matches those identities against the event's object selector and
//! independently matches the targeting stack object against its source
//! selector. It never interprets Oracle text.

use serde_json::Value;
use std::collections::BTreeMap;

use super::{
    ActionKind, CardInstance, DecisionKind, DecisionProvider, EngineDecisionRequest, EngineError,
    GameEngine, LegalAction, StackObject, TargetRef, matches_stack_filter,
    matches_stack_item_filter, stack_object_is_spell, target_candidates_supported,
    triggered_event_variants, triggered_rule_supported, value_kind,
};

pub(super) fn became_target_event_supported(event: &Value) -> Option<bool> {
    if value_kind(event) != Some("becameTarget") {
        return None;
    }
    // The generic form: a targeted subject and a typed targeting stack-object
    // selector, matched by `enqueue_generic_became_target_triggers`.
    if is_generic_became_target_event(event) {
        return Some(
            event.as_object().is_some_and(|fields| {
                fields.keys().all(|key| {
                    matches!(
                        key.as_str(),
                        "kind" | "object" | "targetingStackObject" | "turnOrdinal"
                    )
                })
            }) && (value_kind(&event["object"]) == Some("self")
                || became_target_object_selector_supported(&event["object"]))
                && targeting_stack_selector_supported(&event["targetingStackObject"])
                && event
                    .get("turnOrdinal")
                    .is_none_or(|ordinal| targeting_ordinal_supported(event, ordinal)),
        );
    }
    if value_kind(&event["object"]) == Some("self") {
        return Some(
            event
                .get("controlledByOpponent")
                .is_none_or(Value::is_boolean)
                && event
                    .get("stackObjectKind")
                    .is_none_or(|kind| matches!(kind.as_str(), Some("spell" | "spellOrAbility"))),
        );
    }
    Some(false)
}

/// A `becameTarget` event with a typed targeting stack-object selector. The
/// historical self form (`stackObjectKind`, `controlledByOpponent`, ward)
/// has its own publication path and never carries this selector.
pub(super) fn is_generic_became_target_event(event: &Value) -> bool {
    value_kind(event) == Some("becameTarget") && event.get("targetingStackObject").is_some()
}

/// The action log entry of a permanent becoming the target of one stack
/// object (CR 115.1): the targeting controller, the stack object's kind and
/// the permanent's incarnation, so later patterns can count matching events.
const PERMANENT_TARGETED_EVENT: &str = "permanentBecameTarget";

/// `for the first time each turn` on the source as targeted object: the
/// ordinal counts logged targeting events whose stack object satisfied the
/// same selector. The log keeps only the stack object's kind and controller,
/// so the selector's filter must read only kinds.
fn targeting_ordinal_supported(event: &Value, ordinal: &Value) -> bool {
    value_kind(&event["object"]) == Some("self")
        && ordinal.as_u64().is_some_and(|ordinal| ordinal > 0)
        && stack_kind_filter_supported(&event["targetingStackObject"]["where"])
}

fn stack_kind_filter_supported(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("isSpell" | "isActivatedAbility" | "isTriggeredAbility") => true,
        Some("or") => filter["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty() && operands.iter().all(stack_kind_filter_supported)
        }),
        _ => false,
    }
}

/// `stack_kind_filter_supported` filters read on a logged stack object, with
/// the same meaning as `matches_stack_item_filter` on the live one.
fn logged_stack_kind_matches(filter: &Value, is_spell: bool, ability_kind: Option<&str>) -> bool {
    match value_kind(filter) {
        Some("isSpell") => is_spell,
        Some("isActivatedAbility") => ability_kind == Some("activatedAbility"),
        Some("isTriggeredAbility") => ability_kind == Some("triggeredAbility"),
        Some("or") => filter["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .any(|operand| logged_stack_kind_matches(operand, is_spell, ability_kind))
        }),
        _ => false,
    }
}

fn controller_reference_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        Some("opponentsOf") => controller_reference_supported(&reference["player"]),
        _ => false,
    }
}

fn became_target_object_selector_supported(selector: &Value) -> bool {
    match value_kind(selector) {
        // The permanent the observing Aura or Equipment is attached to.
        Some("attachedPermanent") => value_kind(&selector["attachment"]) == Some("self"),
        Some("union") => selector["sets"].as_array().is_some_and(|sets| {
            !sets.is_empty() && sets.iter().all(became_target_object_selector_supported)
        }),
        Some("permanents" | "spells") => {
            selector
                .get("controller")
                .is_none_or(controller_reference_supported)
                && target_candidates_supported(selector)
        }
        _ => false,
    }
}

/// An absent controller is a stack object of any controller.
fn targeting_stack_selector_supported(selector: &Value) -> bool {
    value_kind(selector) == Some("stackItems")
        && selector
            .get("controller")
            .is_none_or(controller_reference_supported)
        && target_candidates_supported(selector)
}

pub(super) fn newly_targeted_identities(
    previous: &BTreeMap<String, TargetRef>,
    current: &BTreeMap<String, TargetRef>,
) -> BTreeMap<String, TargetRef> {
    let previous_identities = previous.values().collect::<Vec<_>>();
    let mut added = BTreeMap::new();
    let mut seen = Vec::<TargetRef>::new();
    for (target_id, target) in current {
        if !previous_identities.contains(&target) && !seen.contains(target) {
            seen.push(target.clone());
            added.insert(target_id.clone(), target.clone());
        }
    }
    added
}

fn controller_reference_matches(
    reference: &Value,
    candidate_controller: &str,
    ability_controller: &str,
) -> bool {
    match value_kind(reference) {
        Some("controllerOf" | "abilityController") => candidate_controller == ability_controller,
        Some("opponentsOf") => candidate_controller != ability_controller,
        _ => false,
    }
}

impl GameEngine {
    /// Pre-mutation capability check using the same activated cost-choice
    /// exclusion as stack publication. target_order alone includes cost choices.
    pub(super) fn validate_action_target_cost_programs(
        &self,
        action: &LegalAction,
    ) -> Result<(), EngineError> {
        let targets = if action.kind == ActionKind::ActivateAbility {
            let rule = if let Some(rule) = action.decisions.get("grantedAbilityRule") {
                Some(rule)
            } else if let Some(index) = action
                .decisions
                .get("abilityRuleIndex")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
            {
                action
                    .card_instance_id
                    .as_deref()
                    .and_then(|id| {
                        self.permanent_position(id)
                            .map(|(p, c)| &self.state.players[p].battlefield[c])
                            .or_else(|| self.card_outside_battlefield(id))
                    })
                    .and_then(|card| card.definition.printed_rules().get(index))
            } else {
                None
            };
            match rule {
                Some(rule) => Self::activated_effect_targets(rule, &action.targets),
                None if action.decisions.get("manaAbility").and_then(Value::as_bool)
                    == Some(true)
                    && !action.decisions.contains_key("abilityRuleIndex") =>
                {
                    BTreeMap::new()
                }
                // Special actions (CR 116.2), such as turning a face-down
                // permanent face up (116.2b), putting a companion into hand
                // (116.2g) or unlocking a Room door, have no ability rule and
                // choose no targets, so there is no ward cost to validate.
                None if action.targets.is_empty() => BTreeMap::new(),
                None => {
                    return Err(EngineError::new(
                        "activated target validation has no selected ability rule",
                    ));
                }
            }
        } else {
            action
                .targets
                .iter()
                .filter(|(id, _)| action.target_order.contains(id))
                .map(|(id, target)| (id.clone(), target.clone()))
                .collect()
        };
        self.validate_target_cost_programs(&action.player_id, &targets)
    }

    pub(super) fn validate_target_cost_programs(
        &self,
        actor: &str,
        targets: &BTreeMap<String, TargetRef>,
    ) -> Result<(), EngineError> {
        for target in targets.values() {
            let TargetRef::Permanent { instance_id } = target else {
                continue;
            };
            let Some((player, index)) = self.permanent_position(instance_id) else {
                continue;
            };
            let permanent = &self.state.players[player].battlefield[index];
            if permanent.controller == actor {
                continue;
            }
            for cost in self.ward_costs_for_permanent(permanent, actor) {
                super::resolution_costs::CostProgram::decode(&cost)?;
            }
        }
        Ok(())
    }

    pub(super) fn commit_pending_triggered_targets<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.check_quantity_failure()?;
        let mut processed = 0usize;
        loop {
            let pending_ids = self
                .state
                .stack
                .iter()
                .filter(|object| {
                    object.ability_kind.as_deref() == Some("triggeredAbility")
                        && object.ability_rule.as_ref().is_some_and(|rule| {
                            rule["declaration"]["decisions"]
                                .as_array()
                                .is_some_and(|decisions| !decisions.is_empty())
                        })
                        && object.targets.is_empty()
                        && object.decisions.get("triggerTargetsCommitted")
                            != Some(&Value::Bool(true))
                })
                .map(|object| object.id.clone())
                .collect::<Vec<_>>();
            if pending_ids.is_empty() {
                break;
            }

            for stack_id in pending_ids {
                self.check_quantity_failure()?;
                processed += 1;
                if processed > 1024 {
                    return Err(EngineError::new(
                        "triggered target commitment did not reach quiescence",
                    ));
                }
                let Some(stack_object) = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)
                    .cloned()
                else {
                    continue;
                };
                let rule = stack_object
                    .ability_rule
                    .as_ref()
                    .expect("pending triggered declaration has its rule");
                let declarations = self
                    .declarations_for_triggered_rule(
                        rule,
                        &stack_object.controller,
                        &stack_object.card,
                        &stack_object.decisions,
                    )?
                    .unwrap_or_default();
                if declarations.is_empty() {
                    self.retain_stack_objects_with_lki(|object| object.id != stack_id);
                    self.record_event(
                        "abilityFizzled",
                        Some(stack_object.controller),
                        Some(stack_object.card.instance_id),
                        serde_json::json!({ "reason": "noLegalTargets", "stackId": stack_id }),
                    );
                    continue;
                }
                let options = declarations
                    .into_iter()
                    .enumerate()
                    .map(|(index, declaration)| LegalAction {
                        id: format!("triggerTargets:{stack_id}:{index}"),
                        kind: ActionKind::ChooseResolution,
                        player_id: stack_object.controller.clone(),
                        label: format!("Choose targets for {}", stack_object.card.definition.name),
                        card_instance_id: Some(stack_object.card.instance_id.clone()),
                        payment_sources: Vec::new(),
                        decisions: declaration.decisions,
                        targets: declaration.targets,
                        target_order: declaration.target_order,
                        attacker_id: None,
                        blocker_id: None,
                    })
                    .collect::<Vec<_>>();
                let choice = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!("triggerTargets:{stack_id}"),
                        kind: DecisionKind::ResolutionChoice,
                        player_id: stack_object.controller.clone(),
                        source_card: Some(stack_object.card.clone()),
                        source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                        choice: None,
                        options,
                    },
                )?;
                self.validate_target_cost_programs(&stack_object.controller, &choice.targets)?;
                let mut committed_targets = BTreeMap::new();
                if let Some(pending) = self
                    .state
                    .stack
                    .iter_mut()
                    .find(|object| object.id == stack_id)
                {
                    pending.decisions.extend(choice.decisions);
                    pending
                        .decisions
                        .insert("triggerTargetsCommitted".to_string(), Value::Bool(true));
                    pending.targets = choice.targets;
                    committed_targets = pending.targets.clone();
                }
                self.record_event(
                    "triggeredAbilityTargetsChosen",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    serde_json::json!({
                        "stackId": stack_id,
                        "targets": committed_targets,
                    }),
                );
                self.enqueue_targeting_triggers(&stack_id, &committed_targets)?;
            }
        }
        Ok(())
    }

    fn targeting_stack_matches_event(
        &self,
        selector: &Value,
        targeting_stack: &StackObject,
        source: &CardInstance,
    ) -> bool {
        value_kind(selector) == Some("stackItems")
            && selector.get("controller").is_none_or(|controller| {
                controller_reference_matches(
                    controller,
                    &targeting_stack.controller,
                    &source.controller,
                )
            })
            && matches_stack_item_filter(targeting_stack, &selector["where"])
    }

    fn target_matches_event_object(
        &self,
        selector: &Value,
        target: &TargetRef,
        source: &CardInstance,
    ) -> bool {
        match value_kind(selector) {
            // The observing permanent itself (`this creature becomes the
            // target of ...`), while it is on the battlefield.
            Some("self") => matches!(
                target,
                TargetRef::Permanent { instance_id } if *instance_id == source.instance_id
            ),
            Some("union") => selector["sets"].as_array().is_some_and(|sets| {
                sets.iter()
                    .any(|set| self.target_matches_event_object(set, target, source))
            }),
            // The permanent the observing attachment is attached to.
            Some("attachedPermanent") => matches!(
                target,
                TargetRef::Permanent { instance_id }
                    if source.attached_to.as_deref() == Some(instance_id.as_str())
            ),
            Some("permanents") => {
                let TargetRef::Permanent { instance_id } = target else {
                    return false;
                };
                let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
                    return false;
                };
                let permanent = &self.state.players[player_index].battlefield[card_index];
                selector.get("controller").is_none_or(|controller| {
                    controller_reference_matches(
                        controller,
                        &permanent.controller,
                        &source.controller,
                    )
                }) && self.permanent_matches_target_filter(
                    permanent,
                    &selector["where"],
                    &source.controller,
                )
            }
            Some("spells") => {
                let TargetRef::StackObject { stack_id } = target else {
                    return false;
                };
                let Some(object) = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == *stack_id)
                else {
                    return false;
                };
                stack_object_is_spell(object)
                    && selector.get("controller").is_none_or(|controller| {
                        controller_reference_matches(
                            controller,
                            &object.controller,
                            &source.controller,
                        )
                    })
                    && selector
                        .get("where")
                        .is_none_or(|filter| matches_stack_filter(&object.card.definition, filter))
            }
            _ => false,
        }
    }

    /// `for the first time each turn`: the number of logged targeting events
    /// this turn of the targeted permanent's current incarnation whose stack
    /// object satisfied the pattern's selector equals the ordinal (the
    /// current event is logged first). Without an ordinal, any event.
    fn targeting_turn_ordinal_matches(
        &self,
        event: &Value,
        target: &TargetRef,
        source: &CardInstance,
    ) -> bool {
        let Some(ordinal) = event.get("turnOrdinal") else {
            return true;
        };
        let (Some(ordinal), TargetRef::Permanent { instance_id }) = (ordinal.as_u64(), target)
        else {
            return false;
        };
        let selector = &event["targetingStackObject"];
        let identity = self.object_identity_value(instance_id);
        let count = self
            .state
            .events
            .iter()
            .filter(|logged| {
                logged.turn_number == self.state.turn_number
                    && logged.kind == PERMANENT_TARGETED_EVENT
                    && logged.card_instance_id.as_deref() == Some(instance_id.as_str())
                    && logged.detail.get("objectIdentity").unwrap_or(&Value::Null) == &identity
                    && selector.get("controller").is_none_or(|controller| {
                        logged
                            .player_id
                            .as_deref()
                            .is_some_and(|targeting_controller| {
                                controller_reference_matches(
                                    controller,
                                    targeting_controller,
                                    &source.controller,
                                )
                            })
                    })
                    && logged_stack_kind_matches(
                        &selector["where"],
                        logged.detail["stackIsSpell"].as_bool() == Some(true),
                        logged.detail["stackAbilityKind"].as_str(),
                    )
            })
            .count();
        u64::try_from(count).is_ok_and(|count| count == ordinal)
    }

    fn target_event_identity(&self, target: &TargetRef) -> Option<(String, String, String)> {
        match target {
            TargetRef::Permanent { instance_id } => {
                let (player_index, card_index) = self.permanent_position(instance_id)?;
                let permanent = &self.state.players[player_index].battlefield[card_index];
                Some((
                    "permanent".to_string(),
                    instance_id.clone(),
                    permanent.controller.clone(),
                ))
            }
            TargetRef::StackObject { stack_id } => {
                let object = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == *stack_id)?;
                Some((
                    "stackObject".to_string(),
                    stack_id.clone(),
                    object.controller.clone(),
                ))
            }
            TargetRef::Player { .. } | TargetRef::Card { .. } => None,
        }
    }

    pub(super) fn enqueue_generic_became_target_triggers(
        &mut self,
        targeting_stack_id: &str,
        targets: &BTreeMap<String, TargetRef>,
    ) {
        let Some(targeting_stack) = self
            .state
            .stack
            .iter()
            .find(|object| object.id == targeting_stack_id)
            .cloned()
        else {
            return;
        };
        let targeting_controller_id = targeting_stack.controller.clone();
        let mut distinct_targets = Vec::new();
        for target in targets.values() {
            if matches!(
                target,
                TargetRef::Permanent { .. } | TargetRef::StackObject { .. }
            ) && !distinct_targets.contains(target)
            {
                distinct_targets.push(target.clone());
            }
        }

        let mut pending = Vec::new();
        for target in distinct_targets {
            let Some((target_kind, target_id, target_controller_id)) =
                self.target_event_identity(&target)
            else {
                continue;
            };
            // The action log entry precedes trigger collection, so a
            // per-turn ordinal counts the current event (CR 603.2).
            if let TargetRef::Permanent { instance_id } = &target {
                let identity = self.object_identity_value(instance_id);
                self.record_event(
                    PERMANENT_TARGETED_EVENT,
                    Some(targeting_controller_id.clone()),
                    Some(instance_id.clone()),
                    serde_json::json!({
                        "stackId": targeting_stack_id,
                        "stackIsSpell": stack_object_is_spell(&targeting_stack),
                        "stackAbilityKind": targeting_stack.ability_kind,
                        "objectIdentity": identity,
                    }),
                );
            }
            for source in self
                .state
                .players
                .iter()
                .filter(|player| !player.has_lost)
                .flat_map(|player| player.battlefield.iter())
            {
                for rule in self.effective_rules(&source).iter().filter(|rule| {
                    triggered_rule_supported(rule)
                        && triggered_event_variants(&rule["event"])
                            .iter()
                            .any(|event| {
                                is_generic_became_target_event(event)
                                    && self.target_matches_event_object(
                                        &event["object"],
                                        &target,
                                        source,
                                    )
                                    && self.targeting_stack_matches_event(
                                        &event["targetingStackObject"],
                                        &targeting_stack,
                                        source,
                                    )
                                    && self.targeting_turn_ordinal_matches(event, &target, source)
                            })
                }) {
                    pending.push((
                        source.clone(),
                        rule.clone(),
                        BTreeMap::from([
                            (
                                "targetingStackId".to_string(),
                                Value::String(targeting_stack_id.to_string()),
                            ),
                            (
                                "triggeringStackId".to_string(),
                                Value::String(targeting_stack_id.to_string()),
                            ),
                            (
                                "targetingPlayerId".to_string(),
                                Value::String(targeting_controller_id.clone()),
                            ),
                            (
                                "targetingControllerId".to_string(),
                                Value::String(targeting_controller_id.clone()),
                            ),
                            (
                                "targetedObjectKind".to_string(),
                                Value::String(target_kind.clone()),
                            ),
                            (
                                "targetedObjectId".to_string(),
                                Value::String(target_id.clone()),
                            ),
                            (
                                "targetedObjectControllerId".to_string(),
                                Value::String(target_controller_id.clone()),
                            ),
                        ]),
                    ));
                }
            }
        }
        for (source, rule, decisions) in pending {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "targeting_triggers_tests.rs"]
mod tests;
