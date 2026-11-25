//! CR 725 the initiative: a player designation with three inherent
//! triggered abilities.
//!
//! ```text
//! { kind: takeInitiative, player: <singular player reference> }
//! ```
//!
//! - CR 725.2: whenever a player takes the initiative, that player ventures
//!   into Undercity (CR 701.49). The venture is a triggered ability put on
//!   the stack after the instruction that gave the initiative resolves;
//!   taking it again while already holding it triggers again (CR 725.4).
//! - CR 725.2: at the beginning of the upkeep of the player who has the
//!   initiative, that player ventures into Undercity.
//! - CR 725.2: whenever one or more creatures a player controls deal combat
//!   damage to the player who has the initiative, that player takes the
//!   initiative. One combat-damage step publishes at most one such trigger
//!   for each damaging player, however many creatures dealt damage.
//! - CR 725.3: only one player can have the initiative; taking it removes it
//!   from its previous holder.
//!
//! The inherent abilities have no printed source object. They are published
//! with the Undercity game piece as their stack source and an emblem-like
//! provenance (no trigger-multiplicity effect applies to them), exactly as
//! dungeon room abilities are. Support is refused unless every rule of the
//! outside-game Undercity is itself executable: a venture that would enter an
//! unsupported room is never certified.
use super::*;

/// The venture procedure whose dungeon entry restriction names Undercity
/// (CR 309.2a, 725.2).
pub(super) const UNDERCITY_PROCEDURE: &str = "venture into Undercity";
const INITIATIVE_MODIFIER: &str = "initiative";
const VENTURE_EVENT: &str = "initiativeVentureTrigger";
const COMBAT_DAMAGE_EVENT: &str = "initiativeCombatDamageTrigger";

fn is_undercity(dungeon: &CardDefinition) -> bool {
    dungeon.printed_rules().iter().any(|rule| {
        value_kind(rule) == Some("staticAbility")
            && rule["modifiers"].as_array().is_some_and(|modifiers| {
                modifiers.iter().any(|modifier| {
                    value_kind(modifier) == Some("dungeonEntryRestriction")
                        && modifier["procedure"].as_str() == Some(UNDERCITY_PROCEDURE)
                })
            })
    })
}

fn undercity_definition() -> Option<&'static CardDefinition> {
    crate::model::builtin_dungeon_definitions()
        .ok()?
        .iter()
        .find(|dungeon| is_undercity(dungeon))
}

/// Every Undercity rule executes; computed once because the outside-game
/// definition is immutable.
fn undercity_executable() -> bool {
    static EXECUTABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *EXECUTABLE.get_or_init(|| {
        undercity_definition().is_some_and(|dungeon| {
            let rules = dungeon.printed_rules();
            !rules.is_empty() && rules.iter().all(rule_supported)
        })
    })
}

/// `takeInitiative` names one player; the venture it triggers must reach an
/// executable Undercity.
pub(super) fn take_initiative_supported(effect: &Value) -> bool {
    value_kind(effect) == Some("takeInitiative")
        && player_reference_supported(&effect["player"])
        && undercity_executable()
}

fn venture_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": VENTURE_EVENT, "player": { "kind": "abilityController" } },
        "effects": [{
            "kind": "ventureDungeon",
            "player": { "kind": "abilityController" },
            "procedure": UNDERCITY_PROCEDURE,
        }],
    })
}

fn combat_damage_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": COMBAT_DAMAGE_EVENT, "player": { "kind": "abilityController" } },
        "effects": [{
            "kind": "takeInitiative",
            "player": { "kind": "abilityController" },
        }],
    })
}

impl GameEngine {
    /// The player who has the initiative, if any.
    pub(super) fn initiative_holder(&self) -> Option<String> {
        self.state.rule_modifiers.iter().find_map(|modifier| {
            (value_kind(modifier) == Some(INITIATIVE_MODIFIER))
                .then(|| modifier["playerId"].as_str().map(ToOwned::to_owned))
                .flatten()
        })
    }

    /// CR 725.3-725.4: the player takes the initiative (removing it from any
    /// other player) and its venture trigger is published.
    pub(super) fn take_initiative(&mut self, player_id: &str) -> Result<(), EngineError> {
        self.player_index(player_id)?;
        self.state
            .rule_modifiers
            .retain(|modifier| value_kind(modifier) != Some(INITIATIVE_MODIFIER));
        self.state.rule_modifiers.push(json!({
            "kind": INITIATIVE_MODIFIER,
            "playerId": player_id,
        }));
        self.record_event(
            "tookInitiative",
            Some(player_id.to_string()),
            None,
            json!({}),
        );
        self.publish_initiative_trigger(player_id, venture_rule())
    }

    /// CR 725.2: the initiative's upkeep venture, for the active player.
    pub(super) fn enqueue_initiative_upkeep_venture(&mut self) {
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        if self.initiative_holder().as_deref() == Some(active_player_id.as_str())
            && let Err(error) = self.publish_initiative_trigger(&active_player_id, venture_rule())
        {
            self.defer_quantity_failure(error);
        }
    }

    /// CR 725.2: combat damage dealt to the holder by a creature another
    /// player controls triggers that player's taking of the initiative, once
    /// for each damaging player in one combat-damage step.
    pub(super) fn enqueue_initiative_combat_damage(
        &mut self,
        damaged_player_id: &str,
        damaging_player_id: &str,
        amount: i32,
    ) {
        if amount <= 0
            || damaged_player_id == damaging_player_id
            || self.initiative_holder().as_deref() != Some(damaged_player_id)
        {
            return;
        }
        let already_pending = self.state.stack.iter().any(|object| {
            object.controller == damaging_player_id
                && object
                    .ability_rule
                    .as_ref()
                    .is_some_and(|rule| value_kind(&rule["event"]) == Some(COMBAT_DAMAGE_EVENT))
        });
        if already_pending {
            return;
        }
        if let Err(error) =
            self.publish_initiative_trigger(damaging_player_id, combat_damage_rule())
        {
            self.defer_quantity_failure(error);
        }
    }

    fn publish_initiative_trigger(
        &mut self,
        player_id: &str,
        rule: Value,
    ) -> Result<(), EngineError> {
        let undercity = undercity_definition()
            .ok_or_else(|| EngineError::new("the outside-game Undercity is unavailable"))?
            .clone();
        if !self
            .state
            .game_pieces
            .iter()
            .any(|piece| piece.name.eq_ignore_ascii_case(&undercity.name))
        {
            self.state.game_pieces.push(undercity.clone());
        }
        let source = card_instance(player_id, &undercity, usize::MAX);
        match self.put_triggered_ability_on_stack_with_origin(
            &source,
            rule,
            BTreeMap::from([(
                "triggeringPlayerId".to_string(),
                Value::String(player_id.to_string()),
            )]),
            trigger_multiplicity::TriggerOrigin::Emblem,
        ) {
            trigger_multiplicity::TriggerPublicationOutcome::Failed => {
                self.check_quantity_failure()
            }
            _ => Ok(()),
        }
    }
}
