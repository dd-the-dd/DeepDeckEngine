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
            // `each other player` (CR 102.3: teammates included) is the
            // shared complement of the controller.
            && (player_population_supported(&effect["players"])
                || recorded_players_supported(&effect["players"]))
            && effect["effects"]
                .as_array()
                .is_some_and(|effects| !effects.is_empty() && effects.iter().all(effect_supported))
            && effect.get("record").is_none_or(|record| {
                record.as_str().is_some_and(|id| !id.is_empty())
                    && effect["bind"]
                        .as_str()
                        .is_some_and(|bind| recorded_body_supported(&effect["effects"], bind))
            })
            && effect.get("collect").is_none_or(|collect| {
                collect["bind"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty() && Some(id) != effect["bind"].as_str())
                    && object_expression_supported(&collect["objects"])
            }),
    )
}

/// `two|any number of target players each VERB` (CR 115.1, 601.2c): the
/// players declared under one target decision, each performing the body.
fn declared_player_targets_supported(players: &Value) -> bool {
    value_kind(players) == Some("chosenTargets") && players["id"].as_str().is_some()
}

/// A population a player loop ranges over directly: a player set, the
/// complement of the controller, or declared target players.
fn player_population_supported(players: &Value) -> bool {
    player_set_reference_supported(players)
        || player_complement_reference_supported(players)
        || declared_player_targets_supported(players)
}

/// `recordedPlayers` (`each player who does|doesn't ...`, CR 608.2c): the
/// members of a population whose performance of an earlier player loop's
/// instruction, recorded under `record` by that loop, is `performed`.
fn recorded_players_supported(players: &Value) -> bool {
    value_kind(players) == Some("recordedPlayers")
        && players["record"].as_str().is_some_and(|id| !id.is_empty())
        && players["performed"].is_boolean()
        && player_population_supported(&players["players"])
}

/// The action without its performer when the loop member `member` performs
/// it; `None` for an action another player performs.
fn performed_by_member(effect: &Value, member: &Value) -> Option<Value> {
    if effect.get("performer") != Some(member) {
        return None;
    }
    let mut action = effect.clone();
    action.as_object_mut()?.remove("performer");
    Some(action)
}

/// The actions of a recording loop's body and whether the member performs
/// them as its own instruction: either every action names the loop's member
/// as its `performer` (returned without it, `true`), or none names a
/// performer and the actions name the member in their own player slots
/// (`each opponent discards a card`, returned as written, `false`). A body
/// mixing both, or naming another performer, is `None`.
fn recorded_body_actions(effects: &Value, bind: &str) -> Option<(Vec<Value>, bool)> {
    let effects = effects.as_array()?;
    if effects
        .iter()
        .all(|effect| effect.get("performer").is_none())
    {
        return Some((effects.clone(), false));
    }
    let member = json!({"kind": "boundValue", "id": bind});
    effects
        .iter()
        .map(|effect| performed_by_member(effect, &member))
        .collect::<Option<Vec<_>>>()
        .map(|actions| (actions, true))
}

/// The body of a loop that records its members' performance: the member's
/// own actions (`recorded_body_actions`) forming a measured instruction
/// group (`optional_action.rs`: every measured primitive performed, every
/// choice the requested number), or the member's permission over such a
/// group (`each player may ...`, the performance being that the member
/// chose it and it was performed).
fn recorded_body_supported(effects: &Value, bind: &str) -> bool {
    let Some((actions, performed)) = recorded_body_actions(effects, bind) else {
        return false;
    };
    match actions.as_slice() {
        // A permission is the member's own only as a performed action.
        [optional] if value_kind(optional) == Some("optionalEffects") && !performed => false,
        [optional] if value_kind(optional) == Some("optionalEffects") => {
            optional.as_object().is_some_and(|fields| {
                fields
                    .keys()
                    .all(|key| matches!(key.as_str(), "kind" | "player" | "effects"))
            }) && player_reference_supported(&optional["player"])
                && super::optional_action::measured_group_supported(&optional["effects"])
        }
        _ => super::optional_action::measured_group_supported(&Value::Array(actions)),
    }
}

impl GameEngine {
    pub(super) fn player_iteration_ids(
        &mut self,
        selector: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Vec<String>, EngineError> {
        // Declared target players: every player the decision chose, in turn
        // order (a declaration of zero players is an empty iteration).
        if value_kind(selector) == Some("chosenTargets") {
            let decision_id = selector["id"]
                .as_str()
                .ok_or_else(|| EngineError::new("declared player targets require an id"))?;
            let mut players = stack_object
                .targets
                .iter()
                .filter(|(target_id, _)| {
                    target_id.as_str() == decision_id
                        || target_id.starts_with(&format!("{decision_id}:"))
                })
                .filter_map(|(_, target)| match target {
                    TargetRef::Player { player_id } => Some(player_id.clone()),
                    _ => None,
                })
                .filter(|id| {
                    self.player_index(id)
                        .is_ok_and(|index| !self.state.players[index].has_lost)
                })
                .collect::<Vec<_>>();
            players.dedup();
            let player_count = self.state.players.len();
            let active = self.state.active_player;
            players.sort_by_key(|id| {
                self.player_index(id)
                    .map(|index| (index + player_count - active) % player_count)
                    .unwrap_or(player_count)
            });
            return Ok(players);
        }
        // `each player who does|doesn't ...` (CR 608.2c): the members of the
        // base population, in turn order, whose performance the earlier
        // loop recorded as `performed`. A missing record is no empty set.
        if value_kind(selector) == Some("recordedPlayers") {
            let record = selector["record"]
                .as_str()
                .ok_or_else(|| EngineError::new("recorded players require a record"))?;
            let performed = selector["performed"]
                .as_bool()
                .ok_or_else(|| EngineError::new("recorded players require an outcome"))?;
            let Some(RuntimeBinding::Objects(recorded)) = bindings.get(record) else {
                return Err(EngineError::new(
                    "recorded players have no performance record of the earlier player loop",
                ));
            };
            let recorded = recorded.clone();
            let mut members =
                self.player_iteration_ids(&selector["players"], stack_object, bindings)?;
            members.retain(|member| recorded.contains(member) == performed);
            return Ok(members);
        }
        // A restricted population (`each player who ...`): its base
        // population in turn order, then its restriction.
        if value_kind(selector) == Some("filterPlayers") {
            return self.filtered_player_iteration_ids(selector, stack_object, bindings);
        }
        // Fail closed for an explicit player reference. An empty opponent set
        // is valid, whereas an unresolved reference is not an empty set.
        let reference = match value_kind(selector) {
            Some("eachPlayer" | "randomPlayer") => None,
            Some("opponentsOf" | "playersExcept" | "randomOpponentOf") => Some(&selector["player"]),
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
        // `record`: the members who performed the body's instruction, for a
        // later `each player who does|doesn't` (CR 608.2c).
        let record = match effect.get("record") {
            None => None,
            Some(record) => Some(
                record
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| EngineError::new("player iteration record requires an id"))?,
            ),
        };
        let mut performed_members = record.map(|_| Vec::new());
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
            performed_members.as_mut(),
        );
        self.end_shared_child_batches(enclosing);
        result?;
        // The record names players, not objects: only `recordedPlayers`
        // (this module) reads it, as the ids of the performing members.
        if let (Some(record), Some(members)) = (record, performed_members) {
            bindings.insert(record.to_string(), RuntimeBinding::Objects(members));
        }
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
        mut performed_members: Option<&mut Vec<String>>,
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
            local_bindings.insert(
                binding_id.to_string(),
                RuntimeBinding::Player(player_id.clone()),
            );
            if let Some(members) = performed_members.as_deref_mut() {
                if self.run_recorded_member(
                    &effect["effects"],
                    binding_id,
                    &player_id,
                    stack_object,
                    &mut local_bindings,
                    &mut local_decisions,
                    provider,
                )? {
                    members.push(player_id.clone());
                }
            } else {
                self.execute_effects(
                    &effect["effects"],
                    stack_object,
                    &mut local_bindings,
                    &mut local_decisions,
                    provider,
                )?;
            }
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

    /// Runs one member's iteration of a recording loop and returns whether
    /// the member performed its instruction (CR 608.2c, 609.3): the body's
    /// actions run as the member's own (the same as each action's
    /// `performer`, `effect_performer.rs`) and are measured as one
    /// instruction group by the shared performance witnesses. A permission
    /// (`each player may ...`) is performed when the member chose it and
    /// its group was performed; the choice is the ordinary optional
    /// effects' decision.
    #[allow(clippy::too_many_arguments)]
    fn run_recorded_member<P: DecisionProvider>(
        &mut self,
        body: &Value,
        binding_id: &str,
        player_id: &str,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        let (actions, own) = recorded_body_actions(body, binding_id).ok_or_else(|| {
            EngineError::new("a recording player loop performs only its member's actions")
        })?;
        // The member's own instruction runs with the member as its
        // controller, as `execute_performed_effect` runs a performer's
        // action; a body naming the member in its player slots runs as
        // written.
        let mut performed = stack_object.clone();
        if own {
            performed.controller = player_id.to_string();
        }
        if let [optional] = actions.as_slice()
            && value_kind(optional) == Some("optionalEffects")
        {
            let chooser = self
                .resolve_player(&optional["player"], &performed, bindings)
                .ok_or_else(|| EngineError::new("the recorded permission has no chooser"))?;
            if !self.confirm_resolution_action(
                &performed,
                provider,
                &chooser,
                "Perform the optional effects",
            )? {
                return Ok(false);
            }
            let instruction = json!({"actions": optional["effects"].clone()});
            return self.run_measured_instruction(
                &instruction,
                &performed,
                bindings,
                decisions,
                provider,
            );
        }
        let instruction = json!({"actions": Value::Array(actions)});
        self.run_measured_instruction(&instruction, &performed, bindings, decisions, provider)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_iteration_tests.rs"]
mod tests;
