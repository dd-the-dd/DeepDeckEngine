//! Players related to a chosen target after it left the zone it was targeted
//! in (CR 608.2h): the controller or owner (CR 108.3, 109.4) of that exact
//! incarnation, read from its last known information.

use serde_json::Value;

use super::{GameEngine, StackObject, TargetRef, value_kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ObjectPlayerRelation {
    Controller,
    Owner,
}

impl GameEngine {
    /// The related player of a `chosenTarget` reference whose object is no
    /// longer where it was targeted. A permanent uses the identity-captured
    /// departure record of the targeted incarnation; a countered spell uses
    /// the controller its counter event recorded. Any other reference, or an
    /// object without such a record, yields no player rather than a guess.
    pub(super) fn departed_target_player(
        &self,
        relation: ObjectPlayerRelation,
        reference: &Value,
        stack_object: &StackObject,
    ) -> Option<String> {
        if value_kind(reference) != Some("chosenTarget") {
            return None;
        }
        let target_id = reference["id"].as_str()?;
        match stack_object.targets.get(target_id)? {
            TargetRef::Permanent { instance_id } => {
                let original =
                    Self::captured_target_numeric_binding(stack_object, target_id).ok()?;
                let active = self
                    .target_numeric_contexts
                    .binding(&stack_object.id, target_id);
                let binding = active.unwrap_or(&original);
                if binding.identity != original.identity {
                    return None;
                }
                if let Some((player, index)) = self.permanent_position(instance_id) {
                    let card = &self.state.players[player].battlefield[index];
                    if super::object_identity::object_identity_of(card).ok()
                        == Some(binding.identity.clone())
                    {
                        // Still the targeted incarnation: current information.
                        return Some(match relation {
                            ObjectPlayerRelation::Controller => card.controller.clone(),
                            ObjectPlayerRelation::Owner => card.owner.clone(),
                        });
                    }
                }
                let last_known = binding.last_known.as_ref()?;
                match relation {
                    ObjectPlayerRelation::Controller => last_known.controller.clone(),
                    ObjectPlayerRelation::Owner => last_known.owner.clone(),
                }
            }
            TargetRef::StackObject { stack_id } => {
                if self.state.stack.iter().any(|object| object.id == *stack_id) {
                    return None;
                }
                let countered = self.state.events.iter().rev().find(|event| {
                    event.kind == "spellCountered"
                        && event.detail["stackId"].as_str() == Some(stack_id.as_str())
                })?;
                match relation {
                    ObjectPlayerRelation::Controller => countered.player_id.clone(),
                    ObjectPlayerRelation::Owner => countered
                        .card_instance_id
                        .as_deref()
                        .and_then(|instance_id| self.card_outside_battlefield(instance_id))
                        .map(|card| card.owner.clone()),
                }
            }
            TargetRef::Card { .. } | TargetRef::Player { .. } => None,
        }
    }
}
