//! Explicit construction/setup barriers, never called by resolution or queries.
use super::object_identity::existing_object_identity_of;
use super::object_identity::{
    IdentityAssignment, install_object_identities, prepare_object_identity_requests,
};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) enum ConstructionAdmission {
    NewGame,
    #[cfg(test)]
    FixtureExtensions,
}

use super::card_zone_transition::ObjectZone;

#[derive(Clone)]
struct AdmittedCarrier {
    zone: ObjectZone,
    identity: Option<String>,
}

fn live_carriers(state: &GameState) -> Vec<(ObjectZone, &CardInstance)> {
    let mut cards = Vec::new();
    for player in &state.players {
        for (zone, values) in [
            (ObjectZone::Library(player.id.clone()), &player.library),
            (ObjectZone::Hand(player.id.clone()), &player.hand),
            (ObjectZone::Graveyard(player.id.clone()), &player.graveyard),
            (ObjectZone::Battlefield, &player.battlefield),
            (ObjectZone::Exile, &player.exile),
            (ObjectZone::Command, &player.command_zone),
        ] {
            cards.extend(values.iter().map(|card| (zone.clone(), card)));
        }
    }
    cards
}

fn collect_admitted_carriers(
    state: &GameState,
    require_identity: bool,
) -> Result<BTreeMap<String, AdmittedCarrier>, EngineError> {
    let owners = state
        .players
        .iter()
        .map(|player| player.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut identities = BTreeSet::new();
    let mut result = BTreeMap::new();
    for (zone, card) in live_carriers(state) {
        if card.instance_id.is_empty()
            || result.contains_key(&card.instance_id)
            || !owners.contains(card.owner.as_str())
        {
            return Err(EngineError::new(
                "invalid or duplicate live carrier at setup admission",
            ));
        }
        if matches!(&zone, ObjectZone::Library(owner) | ObjectZone::Hand(owner) | ObjectZone::Graveyard(owner) if owner != &card.owner)
        {
            return Err(EngineError::new(
                "private setup zone does not belong to card owner",
            ));
        }
        let current_identity = existing_object_identity_of(card)?;
        let identity = current_identity
            .as_ref()
            .map(|value| value.zone_identity.as_str());
        if require_identity && identity.is_none() {
            return Err(EngineError::new(
                "missing live incarnation at setup admission",
            ));
        }
        if let Some(identity) = identity {
            if !identities.insert(identity.to_string()) {
                return Err(EngineError::new(
                    "duplicate live incarnation at setup admission",
                ));
            }
            for prefix in ["object:incarnation:", "battlefield:incarnation:"] {
                if let Some(encoded) = identity.strip_prefix(prefix) {
                    let (serial, instance_id) = encoded
                        .split_once(':')
                        .ok_or_else(|| EngineError::new("malformed incarnation marker"))?;
                    let serial = serial
                        .parse::<usize>()
                        .map_err(|_| EngineError::new("invalid incarnation serial"))?;
                    if serial == 0
                        || serial > state.battlefield_incarnation_sequence
                        || instance_id != card.instance_id
                    {
                        return Err(EngineError::new(
                            "incarnation marker exceeds allocator high-water mark",
                        ));
                    }
                }
            }
        }
        result.insert(
            card.instance_id.clone(),
            AdmittedCarrier {
                zone,
                identity: identity.map(str::to_owned),
            },
        );
    }
    Ok(result)
}

fn assign_setup_requests(
    state: &mut GameState,
    requests: &BTreeMap<String, IdentityAssignment>,
) -> Result<(), EngineError> {
    // Stable vector order is shared between preparation and installation.
    let prepared = {
        let cards = live_carriers(state);
        let selected = cards
            .iter()
            .filter_map(|(_, card)| {
                requests
                    .get(&card.instance_id)
                    .map(|assignment| (*assignment, *card))
            })
            .collect::<Vec<_>>();
        if selected.len() != requests.len() {
            return Err(EngineError::new("setup identity carrier disappeared"));
        }
        prepare_object_identity_requests(state.battlefield_incarnation_sequence, &selected)?
    };
    let GameState {
        players,
        battlefield_incarnation_sequence,
        ..
    } = state;
    let mut selected = Vec::new();
    for player in players {
        for zone in [
            &mut player.library,
            &mut player.hand,
            &mut player.graveyard,
            &mut player.battlefield,
            &mut player.exile,
            &mut player.command_zone,
        ] {
            for card in zone {
                if requests.contains_key(&card.instance_id) {
                    selected.push(card);
                }
            }
        }
    }
    install_object_identities(battlefield_incarnation_sequence, &mut selected, prepared)?;
    Ok(())
}

impl GameEngine {
    pub(super) fn admit_initial_object_identities(&mut self) -> Result<(), EngineError> {
        self.admit_construction_objects(ConstructionAdmission::NewGame)
    }

    pub(super) fn admit_construction_objects(
        &mut self,
        mode: ConstructionAdmission,
    ) -> Result<(), EngineError> {
        if !self.state.stack.is_empty()
            || !self.state.permissions.is_empty()
            || !self.state.rule_modifiers.is_empty()
        {
            return Err(EngineError::new(
                "initial admission requires a reference-free starting state",
            ));
        }
        let carriers = collect_admitted_carriers(&self.state, false)?;
        if matches!(mode, ConstructionAdmission::NewGame)
            && carriers.values().any(|carrier| carrier.identity.is_some())
        {
            return Err(EngineError::new(
                "initial admission cannot recapture an admitted state",
            ));
        }
        let requests = carriers
            .into_iter()
            .filter(|(_, carrier)| carrier.identity.is_none())
            .map(|(id, _)| (id, IdentityAssignment::Admission))
            .collect();
        assign_setup_requests(&mut self.state, &requests)
    }

    fn with_setup_identity_update<F>(&mut self, configure: F) -> Result<(), EngineError>
    where
        F: FnOnce(&mut Self) -> Result<(), EngineError>,
    {
        if self.quantity_transaction_in_progress() {
            return Err(EngineError::new(
                "cannot configure setup inside an active transaction",
            ));
        }
        let before = collect_admitted_carriers(&self.state, true)?;
        // Setup may clear event history; the gameplay append-only checkpoint is
        // intentionally inapplicable. This snapshot is construction-only.
        let previous = self.clone();
        let result = configure(self).and_then(|()| {
            let after = collect_admitted_carriers(&self.state, false)?;
            let mut requests = BTreeMap::new();
            for (id, current) in &after {
                match before.get(id) {
                    Some(_) if current.identity.is_none() => {
                        return Err(EngineError::new("setup lost an admitted identity"));
                    }
                    Some(old) if current.zone != old.zone && current.identity == old.identity => {
                        requests.insert(id.clone(), IdentityAssignment::NewIncarnation);
                    }
                    None if current.identity.is_none() => {
                        requests.insert(id.clone(), IdentityAssignment::Admission);
                    }
                    _ => {}
                }
            }
            assign_setup_requests(&mut self.state, &requests)?;
            collect_admitted_carriers(&self.state, true)?;
            Ok(())
        });
        if result.is_err() {
            *self = previous;
        }
        result
    }

    pub fn configure_punching_bag(&mut self, player_id: &str) -> Result<(), EngineError> {
        self.with_setup_identity_update(|engine| engine.configure_punching_bag_body(player_id))
    }
    pub fn configure_fixed_opening_hand(
        &mut self,
        player_id: &str,
        definition_ids: &[String],
    ) -> Result<(), EngineError> {
        self.with_setup_identity_update(|engine| {
            engine.configure_fixed_opening_hand_body(player_id, definition_ids)
        })
    }
    pub fn configure_training_anchor(
        &mut self,
        player_id: &str,
        deadline_round: u32,
    ) -> Result<(), EngineError> {
        self.with_setup_identity_update(|engine| {
            engine.configure_training_anchor_body(player_id, deadline_round)
        })
    }
    pub fn configure_punching_bag_position(
        &mut self,
        learner_player_id: &str,
        learner_battlefield_definition_ids: &[String],
        learner_hand_definition_ids: &[String],
        opponent_player_id: &str,
        opponent_battlefield: &[CardDefinition],
        opponent_library_size: usize,
        opponent_skips_draw_step: bool,
    ) -> Result<(), EngineError> {
        self.with_setup_identity_update(|engine| {
            engine.configure_punching_bag_position_body(
                learner_player_id,
                learner_battlefield_definition_ids,
                learner_hand_definition_ids,
                opponent_player_id,
                opponent_battlefield,
                opponent_library_size,
                opponent_skips_draw_step,
            )
        })
    }
    pub fn configure_deterministic_position(
        &mut self,
        position: &DeterministicGamePosition,
    ) -> Result<(), EngineError> {
        self.with_setup_identity_update(|engine| {
            engine.configure_deterministic_position_body(position)
        })
    }
    pub fn configure_deterministic_battlefield_token(
        &mut self,
        owner_id: &str,
        battlefield_index: usize,
        definition: CardDefinition,
    ) -> Result<(), EngineError> {
        self.with_setup_identity_update(|engine| {
            engine.configure_deterministic_battlefield_token_body(
                owner_id,
                battlefield_index,
                definition,
            )
        })
    }
    pub fn configure_deterministic_graveyard(
        &mut self,
        owner_id: &str,
        definition_ids: &[String],
    ) -> Result<(), EngineError> {
        if definition_ids.is_empty() {
            self.player_index(owner_id)?;
            return Ok(());
        }
        self.with_setup_identity_update(|engine| {
            engine.configure_deterministic_graveyard_body(owner_id, definition_ids)
        })
    }
    pub fn configure_deterministic_command_zone(
        &mut self,
        owner_id: &str,
        definition_ids: &[String],
    ) -> Result<(), EngineError> {
        if definition_ids.is_empty() {
            self.player_index(owner_id)?;
            return Ok(());
        }
        self.with_setup_identity_update(|engine| {
            engine.configure_deterministic_command_zone_body(owner_id, definition_ids)
        })
    }
}
