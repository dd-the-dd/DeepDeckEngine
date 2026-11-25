//! `Do this only once each turn.` (Ondu Spiritdancer, Donal, Irreverent
//! Gremlin): an optional instruction of an ability carries
//! `oncePerTurn: ID`. The rider limits performing the instruction, not the
//! ability: the ability still triggers and resolves each time, and a
//! declined or impossible instruction leaves the limit unspent.
//!
//! ```text
//! optionalEffects { player, effects, oncePerTurn? }
//! optionalAction  { player, action|actions, onPerformed, onDeclined?, oncePerTurn? }
//! optionalPayCostPerformEffects { player, cost, effects, declineEffects?, oncePerTurn? }
//! ```
//!
//! Each performance records one `oncePerTurnInstructionPerformed` event for
//! the resolving ability's source object, carrying the rule's
//! `abilityDigest` and the rider's id. The instruction is spent for the rest
//! of the turn once such an event exists for the same source, rule and id.
//! A permanent that left and returned is a new object (CR 400.7): only the
//! events after its latest entry count. A spent instruction is not offered;
//! it counts as not performed (an unpaid cost), so an `If you don't`
//! outcome follows (CR 608.2c, 608.2d). `optionalEffects` and a paid cost
//! record when the instruction is chosen and paid; `optionalAction` records
//! when its measured instruction was actually performed.
use super::*;

/// The field naming the rider on an optional instruction.
pub(super) const ONCE_PER_TURN_FIELD: &str = "oncePerTurn";
/// The event recording one performance of a limited instruction.
const PERFORMED_EVENT: &str = "oncePerTurnInstructionPerformed";
/// The event that starts a new battlefield object (CR 400.7).
const ENTRY_EVENT: &str = "permanentEnteredBattlefield";

/// Static contract: the rider is absent, or a nonempty id.
pub(super) fn once_per_turn_supported(effect: &Value) -> bool {
    effect
        .get(ONCE_PER_TURN_FIELD)
        .is_none_or(|id| id.as_str().is_some_and(|id| !id.is_empty()))
}

impl GameEngine {
    /// True when `effect` carries the rider and this ability of this object
    /// already performed it this turn.
    pub(super) fn once_per_turn_spent(&self, effect: &Value, stack_object: &StackObject) -> bool {
        let Some(id) = effect[ONCE_PER_TURN_FIELD].as_str() else {
            return false;
        };
        let source = stack_object.card.instance_id.as_str();
        let digest = resolution_records::stack_ability_digest(stack_object);
        let since = self
            .state
            .events
            .iter()
            .rposition(|event| {
                event.kind == ENTRY_EVENT && event.card_instance_id.as_deref() == Some(source)
            })
            .map_or(0, |index| index + 1);
        self.state.events[since..].iter().any(|event| {
            event.turn_number == self.state.turn_number
                && event.kind == PERFORMED_EVENT
                && event.card_instance_id.as_deref() == Some(source)
                && event.detail["abilityDigest"] == digest
                && event.detail["id"].as_str() == Some(id)
        })
    }

    /// Records that a spent limited instruction was not offered, the same
    /// observation an impossible optional action makes (CR 608.2d).
    pub(super) fn record_once_per_turn_unavailable(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
    ) {
        self.record_event(
            "optionalActionUnavailable",
            Some(stack_object.controller.clone()),
            Some(stack_object.card.instance_id.clone()),
            json!({
                "instruction": value_kind(effect),
                "oncePerTurn": effect[ONCE_PER_TURN_FIELD],
                "stackId": stack_object.id,
            }),
        );
    }

    /// Records one performance of a limited instruction; an instruction
    /// without the rider records nothing.
    pub(super) fn record_once_per_turn(&mut self, effect: &Value, stack_object: &StackObject) {
        let Some(id) = effect[ONCE_PER_TURN_FIELD].as_str() else {
            return;
        };
        self.record_event(
            PERFORMED_EVENT,
            Some(stack_object.controller.clone()),
            Some(stack_object.card.instance_id.clone()),
            json!({
                "id": id,
                "abilityDigest": resolution_records::stack_ability_digest(stack_object),
                "stackId": stack_object.id,
            }),
        );
    }
}
