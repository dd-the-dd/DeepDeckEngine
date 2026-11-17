//! Synthetic punching-bag scenario infrastructure; this is not Oracle IR.
//!
//! Private typed actions retain historical training clocks, event payloads and
//! shared copy-sequence behavior. They are selected only for built-in game pieces
//! installed by configure_punching_bag, scheduled as ordinary counterable stack
//! objects, and executed only after those objects actually resolve. No parser,
//! contract whitelist or Oracle opcode can request a training action.

use super::trigger_multiplicity;
use super::{CardDefinition, CardInstance, EngineError, GameEngine, StackObject, card_instance};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum TrainingClock {
    Vitality,
    Supply,
}

fn clock_kind(source: &CardInstance) -> Option<TrainingClock> {
    if !source.definition.is_game_piece {
        return None;
    }
    match source.definition.id.as_str() {
        "punching-bag-vitality-clock" => Some(TrainingClock::Vitality),
        "punching-bag-supply-clock" => Some(TrainingClock::Supply),
        _ => None,
    }
}

fn clock_trigger() -> Value {
    json!({
        "kind":"triggeredAbility", "source":{"kind":"self"},
        "event":{"kind":"stepBegan","player":{"kind":"controllerOf","object":{"kind":"self"}},"step":"upkeep"},
        "effects":[],
    })
}

impl GameEngine {
    /// Installs the deterministic training opponent. Existing learner cards,
    /// generated-card rotation and scenario events retain their historical meaning.
    pub(super) fn configure_punching_bag_body(
        &mut self,
        player_id: &str,
    ) -> Result<(), EngineError> {
        let player_index = self.player_index(player_id)?;
        if self.state.players[player_index]
            .battlefield
            .iter()
            .any(|card| card.definition.id == "punching-bag-vitality-clock")
        {
            return Ok(());
        }

        let returned_cards = self.state.players[player_index]
            .hand
            .drain(..)
            .collect::<Vec<_>>();
        self.state.players[player_index]
            .library
            .extend(returned_cards);

        let mut clocks = [
            (
                "punching-bag-vitality-clock",
                "Punching Bag Vitality Clock",
                "Artifact",
            ),
            (
                "punching-bag-supply-clock",
                "Punching Bag Supply Clock",
                "Enchantment",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (id, name, type_line))| {
            let mut rules = Vec::new();
            if index == 0 {
                rules.push(json!({
                    "kind": "staticAbility",
                    "source": { "kind": "self" },
                    "activeWhile": {
                        "kind": "inZone",
                        "object": { "kind": "self" },
                        "zone": { "kind": "battlefield" },
                    },
                    "modifiers": [{
                        "kind": "skipDrawStep",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                    }],
                }));
            }
            card_instance(
                player_id,
                &CardDefinition {
                    id: id.to_string(),
                    name: name.to_string(),
                    type_line: type_line.to_string(),
                    is_commander: false,
                    is_token: false,
                    is_game_piece: true,
                    is_sideboard: false,
                    mana_cost: String::new(),
                    power: None,
                    toughness: None,
                    rules,
                },
                usize::MAX - index,
            )
        })
        .collect::<Vec<_>>();
        for clock in &mut clocks {
            self.prepare_battlefield_entry_identity(clock);
        }
        self.state.players[player_index].battlefield.extend(clocks);
        self.record_event(
            "punchingBagConfigured",
            Some(player_id.to_string()),
            None,
            json!({
                "openingHandCount": self.state.players[player_index].hand.len(),
                "libraryCount": self.state.players[player_index].library.len(),
            }),
        );
        Ok(())
    }

    pub(super) fn enqueue_training_clock_for_source(
        &mut self,
        source: &CardInstance,
        source_zone: &str,
        step: &str,
        active_player: &str,
    ) {
        if source_zone == "battlefield"
            && step == "upkeep"
            && source.controller == active_player
            && clock_kind(source).is_some()
        {
            self.put_triggered_ability_on_stack_with_origin(
                source,
                clock_trigger(),
                BTreeMap::new(),
                trigger_multiplicity::TriggerOrigin::Infrastructure,
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    pub(super) fn resolve_training_clock(
        &mut self,
        stack_object: &StackObject,
        rule: &Value,
    ) -> Result<bool, EngineError> {
        let Some(action) = clock_kind(&stack_object.card) else {
            return Ok(false);
        };
        if stack_object.ability_kind.as_deref() != Some("triggeredAbility")
            || rule != &clock_trigger()
        {
            return Ok(false);
        };
        let controller_id = stack_object.controller.clone();
        match action {
            TrainingClock::Vitality => {
                let source_id = stack_object.card.instance_id.clone();
                let mut gained_life = false;
                let mut counter_count = 0;
                if let Some((player_index, card_index)) = self.permanent_position(&source_id) {
                    let counter = self.state.players[player_index].battlefield[card_index]
                        .counters
                        .entry("clock".to_string())
                        .or_default();
                    *counter += 1;
                    if *counter >= 3 {
                        *counter -= 3;
                        gained_life = true;
                    }
                    counter_count = *counter;
                }
                if gained_life {
                    self.gain_life(&controller_id, 1)?;
                }
                self.record_event(
                    "punchingBagVitalityAdvanced",
                    Some(controller_id),
                    Some(source_id),
                    json!({
                        "counterCount": counter_count,
                        "lifeGained": i32::from(gained_life),
                    }),
                );
            }
            TrainingClock::Supply => {
                let source_id = stack_object.card.instance_id.clone();
                let mut create_card = false;
                let mut counter_count = 0;
                if let Some((player_index, card_index)) = self.permanent_position(&source_id) {
                    let counter = self.state.players[player_index].battlefield[card_index]
                        .counters
                        .entry("clock".to_string())
                        .or_default();
                    *counter += 1;
                    if *counter >= 7 {
                        *counter -= 7;
                        create_card = true;
                    }
                    counter_count = *counter;
                }
                let created_card_id = if create_card {
                    self.copy_sequence += 1;
                    let (suffix, type_line, power, toughness) = match self.copy_sequence % 3 {
                        0 => ("artifact", "Artifact", None, None),
                        1 => (
                            "creature",
                            "Creature — Construct",
                            Some("0".to_string()),
                            Some("1".to_string()),
                        ),
                        _ => ("enchantment", "Enchantment", None, None),
                    };
                    let definition = CardDefinition {
                        id: format!("punching-bag-generated-{suffix}"),
                        name: format!("Punching Bag Generated {suffix}"),
                        type_line: type_line.to_string(),
                        is_commander: false,
                        is_token: false,
                        is_game_piece: false,
                        is_sideboard: false,
                        mana_cost: "{0}".to_string(),
                        power,
                        toughness,
                        rules: Vec::new(),
                    };
                    let card = card_instance(
                        &controller_id,
                        &definition,
                        usize::MAX / 2 + self.copy_sequence,
                    );
                    let card_id = card.instance_id.clone();
                    let player_index = self.player_index(&controller_id)?;
                    self.state.players[player_index].library.push(card);
                    Some(card_id)
                } else {
                    None
                };
                self.record_event(
                    "punchingBagSupplyAdvanced",
                    Some(controller_id),
                    Some(source_id),
                    json!({
                        "counterCount": counter_count,
                        "createdCardInstanceId": created_card_id,
                    }),
                );
            }
        }
        Ok(true)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "training_tests.rs"]
mod tests;
