//! Static combat restrictions gated on the defending player's state (`This
//! creature can't attack unless defending player controls an Island`, `can't
//! be blocked as long as defending player controls an artifact`).
//!
//! CR 506.2 defines the defending player of each attacking creature; CR
//! 508.1c and 509.1b check a restriction as attackers and blockers are
//! declared. Such a condition is therefore not a standing gate of the static
//! ability: it is evaluated for one player at each check, the player the
//! restricted creature would attack (an attack declaration) or attacks (a
//! block declaration). The compiler lowers `defending player` in that
//! condition as `{"kind":"defendingPlayer"}`; the check binds that one player
//! as the observed player of a pending evaluation of the restriction's
//! source, so the shared runtime condition evaluator reads every measure of
//! that player through its existing player resolution.
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{CardInstance, EngineError, GameEngine, StackObject, value_kind};

const RESTRICTION_KIND: &str = "defendingPlayerCombatRestriction";
const DEFENDING_PLAYER_KIND: &str = "defendingPlayer";

/// Validator of one per-combat restriction modifier: a supported restricted
/// object selector, one supported restriction and a condition the shared
/// evaluator supports once the defending player is bound.
pub(super) fn modifier_supported(modifier: &Value) -> bool {
    let restriction_supported = match modifier["restriction"].as_str() {
        Some("cantAttack" | "cantBeBlocked") => modifier.get("blockers").is_none(),
        Some("cantBeBlockedBy") => super::static_object_selector_supported(&modifier["blockers"]),
        _ => false,
    };
    restriction_supported
        && super::static_object_selector_supported(&modifier["objects"])
        && reads_defending_player(&modifier["condition"])
        && super::condition_supported(&bind_defending_player(&modifier["condition"]))
}

/// True when the lowered condition names the defending player somewhere.
fn reads_defending_player(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            value_kind(value) == Some(DEFENDING_PLAYER_KIND)
                || map.values().any(reads_defending_player)
        }
        Value::Array(values) => values.iter().any(reads_defending_player),
        _ => false,
    }
}

/// The condition with every `defendingPlayer` reference read as the observed
/// player of the pending check, which binds the defending player there.
fn bind_defending_player(value: &Value) -> Value {
    match value {
        Value::Object(map)
            if value_kind(value) == Some(DEFENDING_PLAYER_KIND) && map.len() == 1 =>
        {
            json!({ "kind": "triggeringPlayer" })
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), bind_defending_player(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(bind_defending_player).collect()),
        other => other.clone(),
    }
}

impl GameEngine {
    /// CR 508.1c: a restriction forbids `attacker` to attack a defender whose
    /// defending player is `defending_player_id`.
    pub(super) fn defending_player_forbids_attack(
        &self,
        attacker: &CardInstance,
        defending_player_id: &str,
    ) -> Result<bool, EngineError> {
        for (source, modifier) in self.battlefield_static_modifiers() {
            if value_kind(&modifier) == Some(RESTRICTION_KIND)
                && modifier["restriction"].as_str() == Some("cantAttack")
                && self.combat_restriction_selector_matches(&source, &modifier["objects"], attacker)
                && self.defending_player_condition_holds(
                    &source,
                    &modifier["condition"],
                    defending_player_id,
                )?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// CR 509.1b: a restriction forbids `blocker` to block `attacker` in a
    /// combat whose defending player is the attacker's (CR 506.2).
    pub(super) fn defending_player_forbids_block(
        &self,
        blocker: &CardInstance,
        attacker: &CardInstance,
    ) -> Result<bool, EngineError> {
        let Some(defending_player_id) = self
            .state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == attacker.instance_id)
            .map(|assignment| assignment.defending_player_id.clone())
        else {
            return Ok(false);
        };
        for (source, modifier) in self.battlefield_static_modifiers() {
            if value_kind(&modifier) != Some(RESTRICTION_KIND)
                || !self.combat_restriction_selector_matches(
                    &source,
                    &modifier["objects"],
                    attacker,
                )
            {
                continue;
            }
            let restricted = match modifier["restriction"].as_str() {
                Some("cantBeBlocked") => true,
                Some("cantBeBlockedBy") => self.combat_restriction_selector_matches(
                    &source,
                    &modifier["blockers"],
                    blocker,
                ),
                _ => false,
            };
            if restricted
                && self.defending_player_condition_holds(
                    &source,
                    &modifier["condition"],
                    &defending_player_id,
                )?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Evaluates the restriction's condition for one defending player, with
    /// the restriction's source and its controller as the evaluation's
    /// source and ability controller. A missing player or an unevaluable
    /// condition is an error, never a false condition.
    fn defending_player_condition_holds(
        &self,
        source: &CardInstance,
        condition: &Value,
        defending_player_id: &str,
    ) -> Result<bool, EngineError> {
        if defending_player_id.is_empty() {
            return Err(EngineError::new(
                "defending-player combat restriction: the combat records no defending player",
            ));
        }
        let decisions = BTreeMap::from([(
            "triggeringPlayerId".to_string(),
            Value::String(defending_player_id.to_string()),
        )]);
        let pending = StackObject {
            id: format!("defending-player-check:{}", source.instance_id),
            controller: source.controller.clone(),
            card: source.clone(),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("staticAbility".to_string()),
            ability_rule: None,
            decisions: decisions.clone(),
            targets: BTreeMap::new(),
        };
        self.checked_runtime_numeric_condition(
            &bind_defending_player(condition),
            &pending,
            &BTreeMap::new(),
            &decisions,
        )
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "defending_player_restriction_tests.rs"]
mod tests;
