//! `unattach OBJECT` (CR 701.3, 301.5c): an attached permanent becomes
//! attached to nothing and stays on the battlefield. An unattached Equipment
//! or Fortification stays where it is; an Aura attached to nothing is put
//! into its owner's graveyard by the state-based action of CR 704.5m. An
//! object that is attached to nothing is unchanged and no event happens.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(match value_kind(effect) {
        Some("unattachPermanent") => {
            target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"])
        }
        _ => false,
    })
}

impl GameEngine {
    pub(super) fn execute_attachment_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("unattachPermanent") {
            return Err(EngineError::new(format!(
                "unsupported effect reached execution: {}",
                value_kind(effect).unwrap_or("missingKind")
            )));
        }
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
            let permanent = &mut self.state.players[player_index].battlefield[card_index];
            let Some(host) = permanent.attached_to.take() else {
                continue;
            };
            let controller = permanent.controller.clone();
            self.record_event(
                "permanentUnattached",
                Some(controller),
                Some(instance_id.clone()),
                json!({ "from": host }),
            );
        }
        Ok(true)
    }
}
