//! Scoped iteration over player identities. The body is an ordinary effect
//! sequence; it does not know about selection groups or sacrifice.

use super::*;

pub(super) fn player_iteration_supported(effect: &Value) -> Option<bool> {
    // The bind distinguishes the new general contract from the old template,
    // which is retained until its batch-sacrifice replacement is proved.
    if value_kind(effect) != Some("forEachPlayer") || effect.get("bind").is_none() {
        return None;
    }
    Some(
        effect["bind"].as_str().is_some_and(|id| !id.is_empty())
            && player_set_reference_supported(&effect["players"])
            && effect["effects"]
                .as_array()
                .is_some_and(|effects| !effects.is_empty() && effects.iter().all(effect_supported))
            && effect.get("collect").is_none_or(|collect| {
                collect["bind"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty() && Some(id) != effect["bind"].as_str())
                    && object_expression_supported(&collect["objects"])
            }),
    )
}

impl GameEngine {
    pub(super) fn player_iteration_ids(
        &mut self,
        selector: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Vec<String>, EngineError> {
        // Fail closed for an explicit player reference. An empty opponent set
        // is valid, whereas an unresolved reference is not an empty set.
        let reference = match value_kind(selector) {
            Some("eachPlayer" | "randomPlayer") => None,
            Some("opponentsOf" | "randomOpponentOf") => Some(&selector["player"]),
            _ => Some(selector),
        };
        if let Some(reference) = reference {
            let player_id = self
                .resolve_player(reference, stack_object, bindings)
                .ok_or_else(|| {
                    EngineError::new("player iteration has an unresolved player reference")
                })?;
            self.player_index(&player_id)?;
        }
        let mut players = self.resolve_players(selector, stack_object, bindings);
        players.retain(|id| {
            self.player_index(id)
                .is_ok_and(|index| !self.state.players[index].has_lost)
        });
        let player_count = self.state.players.len();
        let active = self.state.active_player;
        players.sort_by_key(|id| {
            self.player_index(id)
                .map(|index| (index + player_count - active) % player_count)
                .unwrap_or(player_count)
        });
        if matches!(
            value_kind(selector),
            Some("randomPlayer" | "randomOpponentOf")
        ) {
            players.shuffle(&mut self.rng);
            players.truncate(1);
        }
        Ok(players)
    }

    pub(super) fn execute_player_iteration<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("forEachPlayer") || effect.get("bind").is_none() {
            return Ok(false);
        }
        let binding_id = effect["bind"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| EngineError::new("player iteration requires a binding"))?;
        let players = self.player_iteration_ids(&effect["players"], stack_object, bindings)?;
        let mut collected = Vec::new();
        let mut seen = BTreeSet::new();
        // CR 101.4: the same instruction performed by each player is one
        // simultaneous event, so `one or more` triggers see it once.
        let children = effect["effects"].as_array().map_or(0, Vec::len);
        let enclosing = self.begin_shared_child_batches(children);
        let result = self.execute_player_iteration_children(
            effect,
            binding_id,
            players,
            stack_object,
            bindings,
            decisions,
            provider,
            &mut collected,
            &mut seen,
        );
        self.end_shared_child_batches(enclosing);
        result?;
        if let Some(collect) = effect.get("collect") {
            let id = collect["bind"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    EngineError::new("player iteration collection requires a binding")
                })?;
            bindings.insert(id.to_string(), RuntimeBinding::Objects(collected));
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_player_iteration_children<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        binding_id: &str,
        players: Vec<String>,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
        collected: &mut Vec<String>,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), EngineError> {
        for player_id in players {
            self.restart_shared_child_batches();
            // The initial set determines order, but an earlier child's effect
            // may have eliminated a later player before its iteration begins.
            let player_index = self.player_index(&player_id)?;
            if self.state.players[player_index].has_lost {
                continue;
            }
            // Only local interpreter environments are copied, never GameState.
            // Dropping these frames restores shadowed values on both Ok and Err.
            let mut local_bindings = bindings.clone();
            let mut local_decisions = decisions.clone();
            local_bindings.insert(binding_id.to_string(), RuntimeBinding::Player(player_id));
            self.execute_effects(
                &effect["effects"],
                stack_object,
                &mut local_bindings,
                &mut local_decisions,
                provider,
            )?;
            if self.state.outcome.is_some() {
                break;
            }
            if let Some(collect) = effect.get("collect") {
                let objects = self
                    .resolve_runtime_object_ids(
                        &collect["objects"],
                        stack_object,
                        &local_bindings,
                        &local_decisions,
                    )
                    .ok_or_else(|| EngineError::new("player iteration collection is unresolved"))?;
                for object in objects {
                    if seen.insert(object.clone()) {
                        collected.push(object);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_iteration_tests.rs"]
mod tests;
