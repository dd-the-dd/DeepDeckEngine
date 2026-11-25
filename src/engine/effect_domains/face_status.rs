//! Face status of a permanent changed by an instruction (CR 708): `turn
//! OBJECT face up` and `turn OBJECT face down`. The permanent stays the same
//! object (CR 708.8): its counters, attachments, damage and tapped status are
//! kept; only its copiable values change.
//!
//! - Face down (CR 708.2a): a 2/2 face-down creature with no text, name,
//!   subtypes or mana cost. A face-down permanent can't be turned face down
//!   again (CR 708.2b) and a double-faced permanent can't be turned face down
//!   (CR 712.13): nothing happens.
//! - Face up (CR 708.8): its copiable values revert to the printed card kept
//!   while it was face down. A face-up permanent is unchanged. A face-down
//!   instant or sorcery card is revealed and stays face down (CR 701.40g,
//!   701.58g), and `turned face up` triggers don't trigger.
//!
//! A permanent turned face down by an instruction is marked so that the
//! turn-face-up special action stays reserved to the rules that allow it
//! (morph or disguise, CR 702.37e, 702.168d); a manifested or cloaked card is
//! not marked (CR 701.40b, 701.58b).
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
use super::super::*;

/// The flag of a permanent an instruction turned face down (see module doc).
pub(in crate::engine) const TURNED_FACE_DOWN_BY_EFFECT: &str = "turnedFaceDownByEffect";

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(match value_kind(effect) {
        Some("turnPermanentFaceUp" | "turnPermanentFaceDown") => {
            target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"])
        }
        _ => false,
    })
}

/// CR 712.13: a transforming double-faced permanent has the shared
/// transform-faces marker on its printed card.
fn is_double_faced(card: &CardInstance) -> bool {
    card.printed_definition
        .as_ref()
        .unwrap_or(&card.definition)
        .printed_rules()
        .iter()
        .any(|rule| value_kind(rule) == Some("rulesMarker") && rule["transformFaces"].is_array())
}

impl GameEngine {
    pub(super) fn execute_face_status_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        let face_up = match value_kind(effect) {
            Some("turnPermanentFaceUp") => true,
            Some("turnPermanentFaceDown") => false,
            other => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    other.unwrap_or("missingKind")
                )));
            }
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
            if face_up {
                self.turn_permanent_face_up_by_effect(&instance_id, stack_object);
            } else {
                self.turn_permanent_face_down_by_effect(&instance_id, stack_object);
            }
        }
        Ok(true)
    }

    /// CR 708.8, 701.40g: see the module documentation.
    fn turn_permanent_face_up_by_effect(&mut self, instance_id: &str, stack_object: &StackObject) {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        let permanent = &mut self.state.players[player_index].battlefield[card_index];
        if permanent.flags.get("faceDown") != Some(&true) {
            return;
        }
        let Some(printed) = permanent.printed_definition.clone() else {
            return;
        };
        if type_line_contains(&printed.type_line, "Instant")
            || type_line_contains(&printed.type_line, "Sorcery")
        {
            // CR 701.40g, 701.58g: revealed, left face down, no trigger.
            self.record_event(
                "faceDownPermanentRevealed",
                Some(stack_object.controller.clone()),
                Some(instance_id.to_string()),
                json!({}),
            );
            return;
        }
        permanent.definition = printed;
        permanent.flags.remove("faceDown");
        permanent.flags.remove(TURNED_FACE_DOWN_BY_EFFECT);
        let turned = permanent.clone();
        let controller = turned.controller.clone();
        self.record_event(
            "permanentTurnedFaceUp",
            Some(controller),
            Some(instance_id.to_string()),
            json!({}),
        );
        self.enqueue_turned_face_up_triggers(&turned);
    }

    /// CR 708.2a-b, 712.13: see the module documentation.
    fn turn_permanent_face_down_by_effect(
        &mut self,
        instance_id: &str,
        stack_object: &StackObject,
    ) {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        let permanent = &mut self.state.players[player_index].battlefield[card_index];
        if permanent.flags.get("faceDown") == Some(&true) || is_double_faced(permanent) {
            return;
        }
        if permanent.printed_definition.is_none() {
            permanent.printed_definition = Some(permanent.definition.clone());
        }
        permanent.definition = face_down_creature_definition(&permanent.definition);
        permanent.flags.insert("faceDown".to_string(), true);
        permanent
            .flags
            .insert(TURNED_FACE_DOWN_BY_EFFECT.to_string(), true);
        self.record_event(
            "permanentTurnedFaceDown",
            Some(stack_object.controller.clone()),
            Some(instance_id.to_string()),
            json!({}),
        );
    }
}
