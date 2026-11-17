//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("lookAtTopCards") => {
            effect["bind"].is_string()
                && library_zone_supported(&effect["zone"])
                && singular_player_reference_supported(&effect["zone"]["player"])
                && effect
                    .get("player")
                    .is_none_or(singular_player_reference_supported)
                && numeric_expression_supported(&effect["count"])
        }
        Some("revealFromTopUntilCount") => {
            library_zone_supported(&effect["zone"])
                && numeric_expression_supported(&effect["count"])
                && card_filter_supported(&effect["where"])
                && effect["bind"].is_string()
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_library_observation_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_library_observation_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_library_observation_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("lookAtTopCards") => {
                let owner_id = self
                    .resolve_player(&effect["zone"]["player"], stack_object, bindings)
                    .ok_or_else(|| EngineError::new("unresolved lookAtTopCards library owner"))?;
                let viewer_id = if let Some(reference) = effect.get("player") {
                    self.resolve_player(reference, stack_object, bindings)
                        .ok_or_else(|| EngineError::new("unresolved lookAtTopCards viewer"))?
                } else {
                    stack_object.controller.clone()
                };
                let player_index = self.player_index(&owner_id)?;
                self.player_index(&viewer_id)?;
                let count = self
                    .checked_runtime_numeric_expression(
                        &effect["count"],
                        stack_object,
                        bindings,
                        decisions,
                        "lookAtTopCards.count",
                    )?
                    .max(0) as usize;
                let objects = self.state.players[player_index]
                    .library
                    .iter()
                    .rev()
                    .take(count)
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Objects(objects.clone()),
                    );
                }
                self.state.rule_modifiers.retain(|modifier| {
                    value_kind(modifier) != Some("knownLibraryCards")
                        || modifier["playerId"].as_str() != Some(owner_id.as_str())
                        || modifier["viewerId"].as_str() != Some(viewer_id.as_str())
                });
                if !objects.is_empty() {
                    self.state.rule_modifiers.push(json!({
                        "kind": "knownLibraryCards",
                        "playerId": owner_id,
                        "viewerId": viewer_id,
                        "cardInstanceIds": objects,
                    }));
                }
                self.record_event(
                    "cardsLookedAt",
                    Some(viewer_id.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    if owner_id==viewer_id {json!({ "cardIds": objects, "count": objects.len() })}
                    else {json!({ "cardIds": objects, "count": objects.len(),"zonePlayerId":owner_id })},
                );
            }
            Some("revealFromTopUntilCount") => {
                let Some(player_id) =
                    self.resolve_player(&effect["zone"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let wanted = self
                    .runtime_numeric_expression(&effect["count"], stack_object, bindings, decisions)
                    .unwrap_or(0)
                    .max(0) as usize;
                let mut matched = 0;
                let mut revealed = Vec::new();
                for card in self.state.players[player_index].library.iter().rev() {
                    if matched >= wanted {
                        break;
                    }
                    revealed.push(card.instance_id.clone());
                    if matches_card_filter(&card.definition, &effect["where"]) {
                        matched += 1;
                    }
                }
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Objects(revealed.clone()),
                    );
                }
                self.record_event(
                    "cardsRevealed",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "cardIds": revealed, "matchingCount": matched }),
                );
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        }
        Ok(())
    }
}
