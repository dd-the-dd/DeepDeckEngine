//! Hidden-information visibility grants (CR 708.5, 401.2). A face-down
//! permanent's controller may look at it at any time; any other player sees
//! its front face only when an effect lets them look at it. A look is one
//! viewer's knowledge of one object incarnation, recorded as a
//! `knownFaceDownPermanent` modifier; it never reveals the object to the
//! other players and it ends when the permanent is no longer that face-down
//! object (turned face up, or left the battlefield: CR 400.7, 708.8).
use super::*;

/// `lookAtFaceDownPermanents`: the objects looked at and the viewer.
pub(super) fn look_at_face_down_supported(effect: &Value) -> bool {
    singular_player_reference_supported(&effect["viewer"])
        && (target_reference_supported(&effect["objects"])
            || object_expression_supported(&effect["objects"]))
}

impl GameEngine {
    /// The battlefield permanents an object reference names, face down now.
    fn face_down_permanent_ids(
        &self,
        objects: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Vec<String> {
        let ids = if target_reference_supported(objects) {
            match self.resolve_target(objects, stack_object, bindings) {
                Some(TargetRef::Permanent { instance_id }) => vec![instance_id],
                _ => Vec::new(),
            }
        } else {
            self.resolve_runtime_object_ids(objects, stack_object, bindings, decisions)
                .unwrap_or_default()
        };
        ids.into_iter()
            .filter(|instance_id| self.permanent_is_face_down(instance_id))
            .collect()
    }

    fn permanent_is_face_down(&self, instance_id: &str) -> bool {
        self.permanent_position(instance_id)
            .is_some_and(|(player_index, card_index)| {
                self.state.players[player_index].battlefield[card_index]
                    .flags
                    .get("faceDown")
                    == Some(&true)
            })
    }

    /// `look at target face-down creature` (CR 708.5): the viewer learns
    /// each named face-down permanent; nothing is revealed to anyone else.
    /// A permanent that is no longer face down shows nothing hidden.
    pub(super) fn look_at_face_down_permanents(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        self.forget_stale_face_down_looks();
        let Some(viewer) = self.resolve_player(&effect["viewer"], stack_object, bindings) else {
            return Ok(());
        };
        for instance_id in
            self.face_down_permanent_ids(&effect["objects"], stack_object, bindings, decisions)
        {
            let known = self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("knownFaceDownPermanent")
                    && modifier["viewerId"].as_str() == Some(viewer.as_str())
                    && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
            });
            if !known {
                self.state.rule_modifiers.push(json!({
                    "kind": "knownFaceDownPermanent",
                    "viewerId": viewer,
                    "cardInstanceId": instance_id,
                }));
            }
            self.record_event(
                "faceDownPermanentLookedAt",
                Some(viewer.clone()),
                Some(instance_id),
                json!({ "sourceCardInstanceId": stack_object.card.instance_id }),
            );
        }
        Ok(())
    }

    /// Whether `viewer` may see the front face of a face-down permanent:
    /// its controller always (CR 708.5), another player only through a
    /// recorded look at this same face-down object. A permanent that is not
    /// face down (or not on the battlefield) hides nothing here.
    pub(super) fn face_down_permanent_visible_to(&self, viewer: &str, instance_id: &str) -> bool {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return false;
        };
        let permanent = &self.state.players[player_index].battlefield[card_index];
        if permanent.flags.get("faceDown") != Some(&true) {
            return true;
        }
        permanent.controller == viewer
            || self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("knownFaceDownPermanent")
                    && modifier["viewerId"].as_str() == Some(viewer)
                    && modifier["cardInstanceId"].as_str() == Some(instance_id)
            })
    }

    /// CR 400.7, 708.8: a look at a face-down permanent ends with that
    /// object (it turned face up or left the battlefield).
    pub(super) fn forget_stale_face_down_looks(&mut self) {
        let stale = self
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| value_kind(modifier) == Some("knownFaceDownPermanent"))
            .filter_map(|modifier| modifier["cardInstanceId"].as_str())
            .filter(|instance_id| !self.permanent_is_face_down(instance_id))
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        if stale.is_empty() {
            return;
        }
        self.state.rule_modifiers.retain(|modifier| {
            value_kind(modifier) != Some("knownFaceDownPermanent")
                || !modifier["cardInstanceId"]
                    .as_str()
                    .is_some_and(|instance_id| stale.contains(instance_id))
        });
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "hidden_visibility_tests.rs"]
mod tests;
