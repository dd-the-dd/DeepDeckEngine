//! Bound objects a delayed trigger captured when it was created (CR 603.7c).
//! Each captured id travels with the identity of the object it named; the
//! trigger finds the object only while it is still that object (CR 400.7),
//! like the transported combat counterpart.

use super::*;

/// The decision key holding the identities of a captured object binding.
pub(super) fn captured_identities_key(binding_id: &str) -> String {
    format!("{binding_id}#identities")
}

impl GameEngine {
    /// The identities of the objects a binding names, captured with them.
    pub(super) fn captured_object_identities(&self, instance_ids: &[String]) -> Value {
        Value::Array(
            instance_ids
                .iter()
                .map(|instance_id| self.object_identity_value(instance_id))
                .collect(),
        )
    }

    /// The first object a delayed trigger captured under `binding_id`, while
    /// it is the same battlefield object it was at capture. `None` outside a
    /// captured-binding context (no identities recorded) or once the object
    /// left or became a new object.
    pub(super) fn captured_bound_object(
        &self,
        stack_object: &StackObject,
        binding_id: &str,
    ) -> Option<String> {
        let instance_id = stack_object
            .decisions
            .get(binding_id)?
            .as_array()?
            .first()?
            .as_str()?;
        let captured = stack_object
            .decisions
            .get(&captured_identities_key(binding_id))?
            .as_array()?
            .first()?;
        (self.permanent_position(instance_id).is_some()
            && &self.object_identity_value(instance_id) == captured)
            .then(|| instance_id.to_string())
    }
}
