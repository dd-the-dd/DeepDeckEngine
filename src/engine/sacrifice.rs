//! Effect sacrifice of explicit object collections. Selection and iteration
//! are independent effects; this domain owns permission and simultaneous exits.
use super::battlefield_departure::{GraveyardMoveCause, PlannedGraveyardDeparture};
use super::cost_payment::SacrificeEventContext;
use super::*;

pub(super) fn explicit_object_sacrifice_supported(effect: &Value) -> Option<bool> {
    if value_kind(effect) != Some("sacrificePermanents") || effect.get("objects").is_none() {
        return None;
    }
    Some(
        object_expression_supported(&effect["objects"])
            && player_reference_supported(&effect["player"])
            && effect.get("bind").is_none_or(Value::is_string),
    )
}

fn sacrifice_event_controller_anchor_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        _ => false,
    }
}

/// Only event scopes anchored in the observing source can be resolved here.
/// In particular, never complement a failed/unbound opponent anchor.
pub(super) fn permanent_sacrificed_event_supported(event: &Value) -> bool {
    value_kind(event) == Some("permanentSacrificed")
        && event
            .get("object")
            .is_none_or(|object| value_kind(object) == Some("self"))
        && event
            .get("player")
            .is_none_or(|player| match value_kind(player) {
                Some("eachPlayer") => true,
                Some("opponentsOf") => {
                    sacrifice_event_controller_anchor_supported(&player["player"])
                }
                _ => sacrifice_event_controller_anchor_supported(player),
            })
        && event.get("where").is_none_or(card_filter_supported)
        && event.get("excludeSource").is_none_or(Value::is_boolean)
        && event.get("destination").is_none()
        && event.get("attackingAlone").is_none()
}

struct PreparedEffectSacrifice {
    permanent: CardInstance,
    destination: PlannedGraveyardDeparture,
    power: i32,
    was_creature: bool,
}

impl GameEngine {
    pub(super) fn permanent_can_be_sacrificed(&self, permanent: &CardInstance) -> bool {
        !permanent.flags.get("phasedOut").copied().unwrap_or(false)
            && !self.permanent_has_static_modifier(permanent, "cantBeSacrificed")
    }

    fn effect_can_cause_player_sacrifice(&self, player_id: &str, source: &StackObject) -> bool {
        source.controller == player_id
            || !self
                .player_has_battlefield_static_modifier(player_id, "preventOpponentForcedSacrifice")
    }

    fn record_effect_sacrifice_prevented(
        &mut self,
        player_id: &str,
        source: &StackObject,
        object: Option<&str>,
    ) {
        let mut detail = json!({"reason":"opponentSpellOrAbility"});
        if let Some(object) = object {
            detail["permanentId"] = json!(object);
        }
        self.record_event(
            "sacrificePrevented",
            Some(player_id.to_string()),
            Some(source.card.instance_id.clone()),
            detail,
        );
    }

    /// Used by action selectors. Independent earlier choices do not call this
    /// helper: their instruction still happens when a later sacrifice is blocked.
    pub(super) fn effect_sacrifice_player_allowed(
        &mut self,
        player_id: &str,
        source: &StackObject,
    ) -> bool {
        let allowed = self.effect_can_cause_player_sacrifice(player_id, source);
        if !allowed {
            self.record_effect_sacrifice_prevented(player_id, source, None);
        }
        allowed
    }

    pub(super) fn sacrifice_effect_collection(
        &mut self,
        objects: Vec<String>,
        allowed_players: &[String],
        source: &StackObject,
    ) -> Result<Vec<String>, EngineError> {
        let players = allowed_players
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        for player in &players {
            self.player_index(player)?;
        }
        let modifiers = self.battlefield_static_modifiers();
        let mut seen = BTreeSet::new();
        let mut prepared = Vec::new();
        let mut prevented = Vec::new();
        let mut prohibited = Vec::new();
        for instance_id in objects {
            if !seen.insert(instance_id.clone()) {
                continue;
            }
            let Some((player, index)) = self.permanent_position(&instance_id) else {
                continue;
            };
            let permanent = self.state.players[player].battlefield[index].clone();
            if !players.contains(permanent.controller.as_str()) {
                continue;
            }
            if !self.effect_can_cause_player_sacrifice(&permanent.controller, source) {
                prevented.push((permanent.controller, instance_id));
                continue;
            }
            if !self.permanent_can_be_sacrificed(&permanent) {
                prohibited.push(instance_id);
                continue;
            }
            self.player_index(&permanent.owner)?;
            let destination = self.plan_graveyard_departure(
                &permanent,
                &modifiers,
                GraveyardMoveCause::Sacrifice,
            )?;
            let power = self.current_power(&permanent);
            let was_creature = self.permanent_is_creature(&permanent);
            prepared.push(PreparedEffectSacrifice {
                permanent,
                destination,
                power,
                was_creature,
            });
        }
        // All plans and real recipients are known before any game mutation,
        // including LKI capture. A protected/refused candidate stays live.
        for (player, instance_id) in prevented {
            self.record_effect_sacrifice_prevented(&player, source, Some(&instance_id));
        }
        for instance_id in prohibited {
            self.record_event("sacrificeProhibited", None, Some(instance_id), json!({}));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }
        let observers = self.capture_battlefield_departure_trigger_sources();
        let death_sources = Self::death_trigger_snapshot_from_sources(observers.clone());
        for entry in &prepared {
            self.capture_pending_source_lki(&entry.permanent.instance_id);
        }
        let mut installed = Vec::new();
        for entry in prepared {
            let movement = self.install_planned_graveyard_departure(
                &entry.permanent,
                &entry.destination,
                "sacrificed",
            )?;
            if movement.movement.is_some() {
                installed.push((entry, movement));
            }
        }
        // Log the complete simultaneous event before any trigger condition can
        // inspect counts. Every selected object has already changed zones.
        for (entry, installed) in &installed {
            let movement = installed
                .movement
                .as_ref()
                .ok_or_else(|| EngineError::new("sacrifice batch lost its installed movement"))?;
            self.record_installed_battlefield_departure(movement, "sacrificed");
            self.publish_graveyard_departure_replacement_event(installed, "sacrificed");
            self.record_permanent_sacrificed_event(
                &entry.permanent,
                &SacrificeEventContext {
                    player_id: &entry.permanent.controller,
                    source_id: Some(&source.card.instance_id),
                    was_creature: Some(entry.was_creature),
                    trigger_sources: &observers,
                },
            );
        }
        let mut graveyard_groups = BTreeMap::<String, Vec<CardInstance>>::new();
        for (_, installed) in &installed {
            if let Some(died) = installed.died() {
                graveyard_groups
                    .entry(died.owner.clone())
                    .or_default()
                    .push(died);
            }
        }
        for (owner, cards) in &graveyard_groups {
            self.record_cards_entered_graveyard_event(owner, cards);
        }
        // Source-linked copy/control effects expire in the completed event's
        // resulting state. Frozen look-back observers remain independent.
        for (entry, _) in &installed {
            self.restore_departure_linked_copies(&entry.permanent.instance_id);
        }
        self.refresh_control_effects();
        // A one-or-more GY observer sees one complete list for this owner,
        // while conditions can already inspect every owner's recorded event.
        for (owner, cards) in &graveyard_groups {
            self.enqueue_recorded_cards_entered_graveyard_triggers(owner, cards);
        }

        for (entry, installed) in &installed {
            let movement = installed
                .movement
                .as_ref()
                .ok_or_else(|| EngineError::new("sacrifice batch lost its installed movement"))?;
            self.publish_installed_battlefield_leave_triggers(
                movement,
                Some(&observers),
                Some(entry.power),
            );
            if let Some((replacement_source, ability)) = &installed.on_replaced {
                self.put_triggered_ability_on_stack_with_origin(
                    replacement_source,
                    ability.clone(),
                    BTreeMap::new(),
                    trigger_multiplicity::TriggerOrigin::Unproven,
                );
                self.check_quantity_failure()?;
            }
            if let Some(died) = installed.died() {
                self.publish_committed_death_triggers(&died, "sacrificed", &death_sources);
            }
            self.enqueue_permanent_sacrificed_triggers_from_sources(&entry.permanent, &observers);
        }
        // Attachment cleanup is after primary event publication, never between
        // two members of the instruction. Its existing Aura/SBA behavior is not
        // being redefined by this batch.
        for (_, installed) in &installed {
            self.cleanup_installed_battlefield_departure(
                installed.movement.as_ref().ok_or_else(|| {
                    EngineError::new("sacrifice batch lost its installed movement")
                })?,
            );
        }
        // Preserve the existing Earthbend hook as a known separate debt, but
        // never let its return happen before the whole exit event is published.
        for (_, installed) in &installed {
            self.finish_battlefield_departure_continuation(
                installed.movement.as_ref().ok_or_else(|| {
                    EngineError::new("sacrifice batch lost its installed movement")
                })?,
            )?;
        }
        Ok(installed
            .into_iter()
            .map(|(entry, _)| entry.permanent.instance_id)
            .collect())
    }

    pub(super) fn sacrifice_permanent_for_effect(
        &mut self,
        instance_id: &str,
        source: &StackObject,
    ) -> Result<Vec<String>, EngineError> {
        let Some((player, index)) = self.permanent_position(instance_id) else {
            return Ok(Vec::new());
        };
        let controller = self.state.players[player].battlefield[index]
            .controller
            .clone();
        self.sacrifice_effect_collection(vec![instance_id.to_string()], &[controller], source)
    }

    pub(super) fn execute_explicit_object_sacrifice(
        &mut self,
        effect: &Value,
        source: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("sacrificePermanents") || effect.get("objects").is_none() {
            return Ok(false);
        }
        let players = self.player_iteration_ids(&effect["player"], source, bindings)?;
        let objects = self
            .resolve_runtime_object_ids(&effect["objects"], source, bindings, decisions)
            .ok_or_else(|| EngineError::new("sacrifice collection is unresolved"))?;
        let sacrificed = self.sacrifice_effect_collection(objects, &players, source)?;
        if let Some(id) = effect["bind"].as_str() {
            bindings.insert(id.to_string(), RuntimeBinding::Objects(sacrificed));
        }
        Ok(true)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "sacrifice_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "sacrifice_query_tests.rs"]
mod query_tests;
