//! One atomic reassignment of a stack object's complete target designation.
//! Optionality belongs to the enclosing program. Choices are prepared read-only.
//!
//! `mode` selects which complete assignments are admissible (CR 115.7):
//! absent, `choose new targets` (115.7d) admits any of them; `all`, `change
//! the target(s)` (115.7a), admits only those changing every target to
//! another legal one, else nothing changes; `one`, `change a target`
//! (115.7b), admits those changing exactly one target. `to` names the one
//! new target the changed target must become.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct PreparedTargetReassignment {
    pub(super) target_order: Vec<String>,
    pub(super) targets: BTreeMap<String, TargetRef>,
    pub(super) decisions: BTreeMap<String, Value>,
    pub(super) replaced: Vec<String>,
}

pub(super) fn node_requires_checkpoint(effect: &Value) -> bool {
    value_kind(effect) == Some("changeTargets")
}

impl GameEngine {
    pub(super) fn execute_target_reassignment<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        resolving: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let Some(TargetRef::StackObject { stack_id }) =
            self.resolve_target(&effect["object"], resolving, bindings)
        else {
            return Ok(());
        };
        let Some(original) = self.state.stack.iter().find(|s| s.id == stack_id).cloned() else {
            return Ok(());
        };
        if original.targets.is_empty() {
            return Ok(());
        }
        let player = self
            .resolve_player(&effect["player"], resolving, bindings)
            .ok_or_else(|| EngineError::new("target reassignment chooser is unresolved"))?;
        self.player_index(&player)?;
        let forced = match effect.get("to") {
            None => None,
            Some(reference) if matches!(value_kind(reference), Some("self" | "abilitySource")) => {
                // A source that left the battlefield is no new target.
                if self
                    .permanent_position(&resolving.card.instance_id)
                    .is_none()
                {
                    return Ok(());
                }
                Some(TargetRef::Permanent {
                    instance_id: resolving.card.instance_id.clone(),
                })
            }
            Some(reference) => match self.resolve_target(reference, resolving, bindings) {
                Some(target) => Some(target),
                None => return Ok(()),
            },
        };
        let mode = effect["mode"].as_str();
        let assignments = self
            .prepare_joint_target_assignments(&original)?
            .into_iter()
            .filter(|assignment| match mode {
                Some("all") => {
                    !assignment.target_order.is_empty()
                        && assignment.replaced.len() == assignment.target_order.len()
                }
                Some("one") => assignment.replaced.len() == 1,
                _ => true,
            })
            .filter(|assignment| {
                forced.as_ref().is_none_or(|forced| {
                    assignment
                        .replaced
                        .iter()
                        .all(|id| assignment.targets.get(id) == Some(forced))
                })
            })
            .collect::<Vec<_>>();
        // CR 115.7a/b: without an admissible complete assignment the
        // designation is unchanged.
        if assignments.is_empty() {
            return Ok(());
        }
        if effect["optional"].as_bool() == Some(true)
            && !self.confirm_resolution_action(
                resolving,
                provider,
                &player,
                "Choose new targets",
            )?
        {
            return Ok(());
        }
        let labels=assignments.iter().enumerate().map(|(index,assignment)|->Result<String,EngineError>{
            let mut display=original.clone();display.targets=assignment.targets.clone();display.decisions=assignment.decisions.clone();
            let slots=assignment.target_order.iter().map(|id|->Result<String,EngineError>{
                let target=&assignment.targets[id];let action=if assignment.replaced.contains(id){"Replace"}else{"Retain"};
                let description=match target{
                    TargetRef::Player{player_id}=>format!("player:{player_id}"),
                    TargetRef::Permanent{instance_id}=>{
                        let capture=Self::captured_target_numeric_binding(&display,id)
                            .map_err(|_|EngineError::new("target reassignment display requires authenticated incarnation"))?;
                        let name=if !assignment.replaced.contains(id)&&!self.target_incarnation_is_current(&original,id,target){
                            "historical permanent".to_string()
                        }else{self.card_name_for_instance(instance_id)};
                        format!("{name} (permanent:{instance_id}, incarnation:{})",capture.identity.zone_identity)
                    },
                    TargetRef::Card{instance_id}=>format!("card:{instance_id}"),
                    TargetRef::StackObject{stack_id}=>format!("stack:{stack_id}"),
                };
                Ok(format!("{id}: {action} -> {description}"))
            }).collect::<Result<Vec<_>,_>>()?.join("; ");
            Ok(format!("assignment:{index} [{slots}]"))
        }).collect::<Result<Vec<_>,_>>()?;
        let choice = self
            .choose_resolution_option(
                resolving,
                provider,
                &player,
                labels.clone(),
                "Choose a complete target assignment",
            )?
            .ok_or_else(|| EngineError::new("target reassignment has no selected assignment"))?;
        let index = labels
            .iter()
            .position(|label| label == &choice)
            .ok_or_else(|| EngineError::new("target reassignment returned an unknown option"))?;
        let prepared = &assignments[index];
        // The immutable canonical option is authority, not provider-supplied targets.
        self.validate_prepared_joint_targets(&original, prepared)?;
        let mut prospective = original.clone();
        prospective.targets = prepared.targets.clone();
        prospective.decisions = prepared.decisions.clone();
        let newly = self.newly_targeted_after_joint_reassignment(&original, &prospective)?;
        self.validate_target_cost_programs(&original.controller, &newly)?;
        let current = self
            .state
            .stack
            .iter_mut()
            .find(|s| s.id == stack_id)
            .ok_or_else(|| {
                EngineError::new("target reassignment stack object disappeared during choice")
            })?;
        // Providers receive immutable state: mutation between preparation and commit
        // would violate the decision contract. Recheck target/declaration snapshot.
        if current.targets != original.targets || current.decisions != original.decisions {
            return Err(EngineError::new(
                "target reassignment snapshot changed during choice",
            ));
        }
        current.targets = prospective.targets;
        current.decisions = prospective.decisions;
        if !prepared.replaced.is_empty() {
            self.record_event("targetsReassigned",Some(player),Some(resolving.card.instance_id.clone()),
                json!({"stackId":stack_id,"targetOrder":prepared.target_order,"replaced":prepared.replaced}));
        }
        if !newly.is_empty() {
            self.enqueue_targeting_triggers(&stack_id, &newly)?;
        }
        Ok(())
    }

    /// Object-level comparison: a swap of two already-targeted objects does
    /// not target either anew. A new incarnation at the same id DOES.
    fn newly_targeted_after_joint_reassignment(
        &self,
        before: &StackObject,
        after: &StackObject,
    ) -> Result<BTreeMap<String, TargetRef>, EngineError> {
        let mut result = BTreeMap::new();
        let mut seen = Vec::<(TargetRef, Option<String>)>::new();
        for (id, target) in &after.targets {
            let marker=match target {
                TargetRef::Permanent{..}=>Some(Self::captured_target_numeric_binding(after,id)
                    .map_err(|_|EngineError::new("target reassignment requires an authenticated battlefield incarnation"))?.identity.zone_identity),
                _=>None,
            };
            let identity = (target.clone(), marker.clone());
            if seen.contains(&identity) {
                continue;
            }
            seen.push(identity);
            let old = before.targets.iter().any(|(old_id, old_target)| {
                if old_target != target {
                    return false;
                }
                match marker.as_deref() {
                    None => true,
                    Some(marker) => Self::captured_target_numeric_binding(before, old_id)
                        .is_ok_and(|binding| binding.identity.zone_identity == marker),
                }
            });
            if !old {
                result.insert(id.clone(), target.clone());
            }
        }
        Ok(result)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "target_reassignment_tests.rs"]
mod tests;
