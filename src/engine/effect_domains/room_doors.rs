//! `unlock a locked door of ROOM`, `lock an unlocked door of ROOM` and `lock
//! or unlock a door of ROOM` (CR 709.5f-g, 709.5j). The player chooses one
//! door of each Room whose designation the change allows; the permanent then
//! gains or loses that door's unlocked designation. Unlocking by an
//! instruction is an unlock like the special action: the door's `unlock`
//! triggers and the fully-unlocked triggers apply (CR 709.5h-i). A Room with
//! no door the change allows, or a permanent that isn't a Room, is unchanged.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
use super::super::*;

/// The doors of a Room permanent (CR 709.5: a left and a right half).
const ROOM_DOORS: [(usize, &str); 2] = [(0, "left"), (1, "right")];

fn door_flag(door_index: usize) -> String {
    format!("roomDoor{door_index}Unlocked")
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(match value_kind(effect) {
        Some("changeRoomDoor") => {
            (target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"]))
                && player_reference_supported(&effect["player"])
                && matches!(
                    effect["change"].as_str(),
                    Some("unlock" | "lock" | "lockOrUnlock")
                )
        }
        _ => false,
    })
}

impl GameEngine {
    pub(super) fn execute_room_doors_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("changeRoomDoor") {
            return Err(EngineError::new(format!(
                "unsupported effect reached execution: {}",
                value_kind(effect).unwrap_or("missingKind")
            )));
        }
        let (unlock_allowed, lock_allowed) = match effect["change"].as_str() {
            Some("unlock") => (true, false),
            Some("lock") => (false, true),
            Some("lockOrUnlock") => (true, true),
            _ => return Err(EngineError::new("unknown room door change")),
        };
        let Some(chooser) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return Ok(true);
        };
        let mut instance_ids = match value_kind(&effect["permanent"]) {
            Some("self" | "abilitySource") => vec![stack_object.card.instance_id.clone()],
            _ => self
                .resolve_runtime_object_ids(&effect["permanent"], stack_object, bindings, decisions)
                .unwrap_or_default(),
        };
        if instance_ids.is_empty()
            && let Some(TargetRef::Permanent { instance_id }) =
                self.resolve_target(&effect["permanent"], stack_object, bindings)
        {
            instance_ids.push(instance_id);
        }
        for instance_id in instance_ids {
            let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                continue;
            };
            let room = &self.state.players[player_index].battlefield[card_index];
            let printed = room.printed_definition.as_ref().unwrap_or(&room.definition);
            if !is_room_definition(printed) && !is_room_definition(&room.definition) {
                continue;
            }
            // Each allowed door, labeled by the change it receives.
            let options = ROOM_DOORS
                .iter()
                .filter_map(|(door_index, side)| {
                    let unlocked = room.flags.get(&door_flag(*door_index)).copied() == Some(true);
                    if unlocked && lock_allowed {
                        Some((*door_index, false, format!("Lock the {side} door")))
                    } else if !unlocked && unlock_allowed {
                        Some((*door_index, true, format!("Unlock the {side} door")))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let chosen = match options.as_slice() {
                [] => continue,
                [only] => Some(only.2.clone()),
                _ => self.choose_resolution_option(
                    stack_object,
                    provider,
                    &chooser,
                    options.iter().map(|(_, _, label)| label.clone()).collect(),
                    "Choose a door of the Room.",
                )?,
            };
            let Some((door_index, unlock, _)) =
                chosen.and_then(|label| options.into_iter().find(|option| option.2 == label))
            else {
                continue;
            };
            let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                continue;
            };
            let room = &mut self.state.players[player_index].battlefield[card_index];
            if unlock {
                room.flags.insert(door_flag(door_index), true);
            } else {
                room.flags.remove(&door_flag(door_index));
            }
            let room = room.clone();
            self.record_event(
                if unlock { "doorUnlocked" } else { "doorLocked" },
                Some(chooser.clone()),
                Some(instance_id.clone()),
                json!({ "doorIndex": door_index }),
            );
            if unlock {
                self.enqueue_door_unlocked_triggers(&room, door_index);
            }
        }
        Ok(true)
    }
}
