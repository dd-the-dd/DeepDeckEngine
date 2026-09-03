use crate::engine::{
    ActionKind, DecisionKind, DecisionProvider, EngineDecisionRequest, EngineError, GameMode,
    GameSetup, GameState, GameStatus, GameStep, LegalAction,
};
use crate::pilot_catalog::{
    PILOT_DEFINITIONS, PilotCapabilities, pilot_definition, training_pilots,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

const ANALYTICS_SCHEMA_VERSION: &str = "mtg-deck-analytics/v2";
const DEFAULT_PLACKETT_LUCE_MU: f64 = 25.0;
const DEFAULT_PLACKETT_LUCE_SIGMA: f64 = 25.0 / 3.0;
const PLACKETT_LUCE_BETA: f64 = 25.0 / 6.0;
const PLACKETT_LUCE_LEARNING_RATE: f64 = 1.0;
const PLACKETT_LUCE_SIGMA_DECAY: f64 = 0.9975;
const MINIMUM_PLACKETT_LUCE_SIGMA: f64 = 1.0;
const LEGACY_ELO_BASELINE: f64 = 1500.0;
const LEGACY_ELO_LEARNING_RATE: f64 = 24.0;
const UNKNOWN_DECK_CREATOR: &str = "Inconnu";
const LEGACY_ANALYTICS_CONTEXT: &str = "legacy";
pub const PLAYER_MATCH_ANALYTICS_CONTEXT: &str = "player-match";

const METRIC_DEFINITIONS: &[(&str, &str)] = &[
    ("ownTurnOptions", "Options pendant son tour"),
    ("instantSpeedOptions", "Options instant speed"),
    ("creatures", "Créatures en jeu"),
    ("lands", "Terrains en jeu"),
    ("artifacts", "Artefacts en jeu"),
    ("enchantments", "Enchantements en jeu"),
    ("planeswalkers", "Planeswalkers en jeu"),
    ("battles", "Batailles en jeu"),
    ("tokens", "Tokens en jeu"),
    ("plusOnePlusOneCounters", "Marqueurs +1/+1"),
    ("minusOneMinusOneCounters", "Marqueurs -1/-1"),
    ("otherCounters", "Autres marqueurs"),
    ("sacrifices", "Sacrifices"),
    ("creaturesDied", "Créatures mortes"),
    ("creaturesEntered", "Créatures entrées"),
    ("lifeGained", "Points de vie gagnés"),
    ("lifeLost", "Points de vie perdus"),
    ("damage", "Dégâts"),
    ("cardsInHand", "Cartes en main"),
    ("draws", "Cartes piochées"),
    ("cardAdvantage", "Card advantage"),
    ("manaProduced", "Mana produit"),
    ("manaValuePlayed", "Mana value jouée"),
    ("instantsSorceriesCast", "Instants et rituels joués"),
    ("permanentsToGraveyard", "Permanents au cimetière"),
    ("permanentsToExile", "Permanents exilés"),
    (
        "opponentAbilitiesFizzledOrCountered",
        "Capacités adverses fizzled/countered",
    ),
    ("commanderDamage", "Commander damage"),
    ("combatDamage", "Dégâts de combat"),
    ("nonCombatDamage", "Dégâts non-combat"),
    ("cardsCast", "Cartes lancées"),
    ("cardsMilled", "Cartes meulées"),
    ("opponentCardsMilled", "Cartes adverses meulées"),
    ("cardsDiscarded", "Cartes défaussées"),
    ("opponentCardsDiscarded", "Cartes adverses défaussées"),
    ("creaturesKilledInCombat", "Créatures tuées au combat"),
    ("creaturesLostInCombat", "Créatures perdues au combat"),
    ("blocks", "Bloqueurs déclarés"),
    ("attacks", "Attaquants déclarés"),
];

fn empty_metric_sums() -> BTreeMap<String, f64> {
    METRIC_DEFINITIONS
        .iter()
        .map(|(key, _)| ((*key).to_string(), 0.0))
        .collect()
}

fn add_metric(metrics: &mut BTreeMap<String, f64>, key: &str, value: f64) {
    *metrics.entry(key.to_string()).or_default() += value;
}

fn meaningful_priority_actions(options: &[LegalAction]) -> Vec<&LegalAction> {
    options
        .iter()
        .filter(|action| action.kind != ActionKind::PassPriority)
        .collect()
}

fn action_signature(action: &LegalAction) -> String {
    serde_json::to_string(&json!({
        "attackerId": action.attacker_id,
        "blockerId": action.blocker_id,
        "cardInstanceId": action.card_instance_id,
        "decisions": action.decisions,
        "kind": action.kind,
        "targets": action.targets,
    }))
    .expect("analytics action signature serializes")
}

#[derive(Clone, Debug)]
pub struct DecisionObservation {
    turn_number: u32,
    player_id: String,
    own_turn_option_count: usize,
    instant_speed_action_signatures: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TurnSnapshot {
    turn_number: u32,
    player_id: String,
    metrics: BTreeMap<String, f64>,
}

fn type_line_has(type_line: &str, card_type: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case(card_type))
}

fn player_turn_snapshot(state: &GameState, player_id: &str) -> Option<TurnSnapshot> {
    let player = state.players.iter().find(|player| player.id == player_id)?;
    let opponents = state
        .players
        .iter()
        .filter(|opponent| opponent.id != player_id && !opponent.has_lost)
        .collect::<Vec<_>>();
    let opponent_hand_average = if opponents.is_empty() {
        0.0
    } else {
        opponents
            .iter()
            .map(|opponent| opponent.hand.len() as f64)
            .sum::<f64>()
            / opponents.len() as f64
    };
    let mut metrics = empty_metric_sums();
    add_metric(&mut metrics, "cardsInHand", player.hand.len() as f64);
    add_metric(
        &mut metrics,
        "cardAdvantage",
        player.hand.len() as f64 - opponent_hand_average,
    );

    let mut plus_one = 0_i32;
    let mut minus_one = 0_i32;
    let mut other = player
        .counters
        .values()
        .map(|count| (*count).max(0))
        .sum::<i32>();
    for permanent in state
        .players
        .iter()
        .flat_map(|zone_owner| &zone_owner.battlefield)
        .filter(|permanent| permanent.controller == player_id)
    {
        for (counter, count) in &permanent.counters {
            let count = (*count).max(0);
            match counter.as_str() {
                "+1/+1" => plus_one += count,
                "-1/-1" => minus_one += count,
                _ => other += count,
            }
        }
        for (metric, card_type) in [
            ("creatures", "Creature"),
            ("lands", "Land"),
            ("artifacts", "Artifact"),
            ("enchantments", "Enchantment"),
            ("planeswalkers", "Planeswalker"),
            ("battles", "Battle"),
        ] {
            if type_line_has(&permanent.definition.type_line, card_type) {
                add_metric(&mut metrics, metric, 1.0);
            }
        }
        if permanent.definition.is_token {
            add_metric(&mut metrics, "tokens", 1.0);
        }
    }
    add_metric(&mut metrics, "plusOnePlusOneCounters", f64::from(plus_one));
    add_metric(
        &mut metrics,
        "minusOneMinusOneCounters",
        f64::from(minus_one),
    );
    add_metric(&mut metrics, "otherCounters", f64::from(other));

    Some(TurnSnapshot {
        turn_number: state.turn_number,
        player_id: player_id.to_string(),
        metrics,
    })
}

pub struct ObservedDecisionProvider<P> {
    inner: P,
    observations: Vec<DecisionObservation>,
    snapshots: Vec<TurnSnapshot>,
}

impl<P> ObservedDecisionProvider<P> {
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            observations: Vec::new(),
            snapshots: Vec::new(),
        }
    }

    pub fn into_analytics(self) -> (Vec<DecisionObservation>, Vec<TurnSnapshot>) {
        (self.observations, self.snapshots)
    }

    pub fn take_analytics(&mut self) -> (Vec<DecisionObservation>, Vec<TurnSnapshot>) {
        (
            std::mem::take(&mut self.observations),
            std::mem::take(&mut self.snapshots),
        )
    }

    pub fn analytics_checkpoint(&self) -> (usize, usize) {
        (self.observations.len(), self.snapshots.len())
    }

    pub fn restore_analytics_checkpoint(&mut self, checkpoint: (usize, usize)) {
        self.observations.truncate(checkpoint.0);
        self.snapshots.truncate(checkpoint.1);
    }
}

impl<P: DecisionProvider> DecisionProvider for ObservedDecisionProvider<P> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::Priority {
            let active_player_id = state
                .players
                .get(state.active_player)
                .map(|player| player.id.as_str());
            let actions = meaningful_priority_actions(&request.options);
            let own_turn_window = active_player_id == Some(request.player_id.as_str())
                && state.stack.is_empty()
                && matches!(
                    state.step,
                    GameStep::PrecombatMain | GameStep::PostcombatMain
                );
            let signatures = actions
                .iter()
                .map(|action| action_signature(action))
                .collect::<BTreeSet<_>>();
            self.observations.push(DecisionObservation {
                turn_number: state.turn_number,
                player_id: request.player_id.clone(),
                own_turn_option_count: if own_turn_window { signatures.len() } else { 0 },
                instant_speed_action_signatures: if own_turn_window {
                    Vec::new()
                } else {
                    signatures.into_iter().collect()
                },
            });
        }
        self.inner.choose(state, request)
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        self.inner.choose_number(state, request)
    }

    fn choose_card_instance_ids(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        self.inner.choose_card_instance_ids(state, request)
    }

    fn choose_card_name(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<String, EngineError> {
        self.inner.choose_card_name(state, request)
    }

    fn requests_explicit_priority_pass(&self, player_id: &str) -> bool {
        self.inner.requests_explicit_priority_pass(player_id)
    }

    fn allows_combat_declaration_revisions(&self, player_id: &str) -> bool {
        self.inner.allows_combat_declaration_revisions(player_id)
    }

    fn observe_turn_completed(&mut self, state: &GameState) {
        if let Some(active_player) = state.players.get(state.active_player)
            && let Some(snapshot) = player_turn_snapshot(state, &active_player.id)
        {
            self.snapshots.push(snapshot);
        }
        self.inner.observe_turn_completed(state);
    }

    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}

#[derive(Clone, Debug)]
pub struct PlayerTurnAnalytics {
    pub round_number: u32,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Clone, Debug)]
pub struct PlayerGameAnalytics {
    pub player_id: String,
    pub deck_id: String,
    pub deck_name: String,
    pub pilot_id: String,
    pub player_count: usize,
    pub rounds_played: u32,
    pub turns: Vec<PlayerTurnAnalytics>,
    pub won: bool,
    pub lost: bool,
    pub elimination_round: Option<u32>,
    pub win_round: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct GameAnalyticsReport {
    pub context_id: String,
    pub game_mode: GameMode,
    pub status: GameStatus,
    pub players: Vec<PlayerGameAnalytics>,
    pub set_winner_player_id: Option<String>,
}

#[derive(Default)]
struct PlayerGameBuilder {
    turns: BTreeMap<u32, PlayerTurnBuilder>,
    elimination_round: Option<u32>,
}

#[derive(Default)]
struct PlayerTurnBuilder {
    observed: bool,
    metrics: BTreeMap<String, f64>,
}

fn normalized_deck_name(name: &str) -> String {
    let trimmed = name.trim();
    if let Some((base, suffix)) = trimmed.rsplit_once(" #")
        && !suffix.is_empty()
        && suffix.chars().all(|character| character.is_ascii_digit())
    {
        return base.trim().to_string();
    }
    trimmed.to_string()
}

fn analytics_deck_id(
    player_id: &str,
    deck_name: &str,
    deck_session_by_player_id: &BTreeMap<String, String>,
) -> String {
    deck_session_by_player_id
        .get(player_id)
        .cloned()
        .unwrap_or_else(|| slug(deck_name))
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut pending_separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_separator && !result.is_empty() {
                result.push('-');
            }
            pending_separator = false;
            result.push(character);
        } else {
            pending_separator = true;
        }
    }
    if result.is_empty() {
        "deck".to_string()
    } else {
        result
    }
}

fn round_number(setup: &GameSetup, global_turn_number: u32) -> u32 {
    if global_turn_number == 0 {
        return 0;
    }
    let player_count = setup.players.len().max(1);
    1 + (global_turn_number - 1) / player_count as u32
}

fn rounds_played(setup: &GameSetup, snapshots: &[TurnSnapshot], final_state: &GameState) -> u32 {
    let last_played_turn = snapshots
        .iter()
        .map(|snapshot| snapshot.turn_number)
        .max()
        .or_else(|| {
            final_state
                .outcome
                .as_ref()
                .map(|outcome| outcome.turn_number)
        })
        .unwrap_or_else(|| {
            if final_state.status == GameStatus::TurnLimitReached {
                final_state.turn_number.saturating_sub(1)
            } else {
                final_state.turn_number
            }
        });
    round_number(setup, last_played_turn)
}

fn turn_builder<'a>(
    players: &'a mut BTreeMap<String, PlayerGameBuilder>,
    setup: &GameSetup,
    player_id: &str,
    global_turn_number: u32,
) -> Option<&'a mut PlayerTurnBuilder> {
    let round = round_number(setup, global_turn_number);
    players
        .get_mut(player_id)
        .map(|player| player.turns.entry(round).or_default())
}

fn add_player_metric(
    players: &mut BTreeMap<String, PlayerGameBuilder>,
    setup: &GameSetup,
    player_id: &str,
    global_turn_number: u32,
    key: &str,
    value: f64,
) {
    if let Some(turn) = turn_builder(players, setup, player_id, global_turn_number) {
        add_metric(&mut turn.metrics, key, value);
    }
}

fn json_amount(value: &serde_json::Value, key: &str) -> f64 {
    value[key].as_f64().unwrap_or_default().max(0.0)
}

fn mana_count(detail: &serde_json::Value) -> f64 {
    detail["mana"]
        .as_array()
        .map(|mana| mana.len() as f64)
        .or_else(|| detail["mana"].as_str().map(|_| 1.0))
        .unwrap_or_default()
}

fn apply_decision_observations(
    players: &mut BTreeMap<String, PlayerGameBuilder>,
    setup: &GameSetup,
    observations: &[DecisionObservation],
) {
    let mut instant_options = BTreeMap::<(String, u32), BTreeSet<String>>::new();
    for observation in observations {
        let round = round_number(setup, observation.turn_number);
        add_player_metric(
            players,
            setup,
            &observation.player_id,
            observation.turn_number,
            "ownTurnOptions",
            observation.own_turn_option_count as f64,
        );
        instant_options
            .entry((observation.player_id.clone(), round))
            .or_default()
            .extend(observation.instant_speed_action_signatures.iter().cloned());
    }
    for ((player_id, round), signatures) in instant_options {
        if let Some(player) = players.get_mut(&player_id) {
            add_metric(
                &mut player.turns.entry(round).or_default().metrics,
                "instantSpeedOptions",
                signatures.len() as f64,
            );
        }
    }
}

pub fn build_game_analytics_report(
    setup: &GameSetup,
    pilot_by_player_id: &BTreeMap<String, String>,
    deck_session_by_player_id: &BTreeMap<String, String>,
    observations: &[DecisionObservation],
    snapshots: &[TurnSnapshot],
    final_state: &GameState,
) -> GameAnalyticsReport {
    let mut players = setup
        .players
        .iter()
        .map(|player| (player.id.clone(), PlayerGameBuilder::default()))
        .collect::<BTreeMap<_, _>>();

    for snapshot in snapshots {
        if let Some(turn) = turn_builder(
            &mut players,
            setup,
            &snapshot.player_id,
            snapshot.turn_number,
        ) {
            turn.observed = true;
            for (key, value) in &snapshot.metrics {
                turn.metrics.insert(key.clone(), *value);
            }
        }
    }

    apply_decision_observations(&mut players, setup, observations);

    let mut source_controllers = BTreeMap::<String, String>::new();
    let mut combat_sources_by_target = BTreeMap::<(u32, String), BTreeSet<String>>::new();
    for event in &final_state.events {
        if matches!(
            event.kind.as_str(),
            "spellCast"
                | "permanentEnteredBattlefield"
                | "activatedAbilityPutOnStack"
                | "triggeredAbilityPutOnStack"
        ) && let (Some(card_id), Some(player_id)) = (&event.card_instance_id, &event.player_id)
        {
            source_controllers.insert(card_id.clone(), player_id.clone());
        }

        match event.kind.as_str() {
            "cardDrawn" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "draws",
                        1.0,
                    );
                }
            }
            "cardDiscarded" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "cardsDiscarded",
                        1.0,
                    );
                    if let Some(source_controller) = event.detail["sourceControllerId"].as_str()
                        && source_controller != player_id
                    {
                        add_player_metric(
                            &mut players,
                            setup,
                            source_controller,
                            event.turn_number,
                            "opponentCardsDiscarded",
                            1.0,
                        );
                    }
                }
            }
            "cardMilled" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "cardsMilled",
                        1.0,
                    );
                    if let Some(source_controller) = event.detail["sourceControllerId"].as_str()
                        && source_controller != player_id
                    {
                        add_player_metric(
                            &mut players,
                            setup,
                            source_controller,
                            event.turn_number,
                            "opponentCardsMilled",
                            1.0,
                        );
                    }
                }
            }
            "manaAdded" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "manaProduced",
                        mana_count(&event.detail),
                    );
                }
            }
            "spellCast" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "cardsCast",
                        1.0,
                    );
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "manaValuePlayed",
                        json_amount(&event.detail, "manaValue"),
                    );
                    let type_line = event.detail["typeLine"].as_str().unwrap_or_default();
                    if type_line_has(type_line, "Instant") || type_line_has(type_line, "Sorcery") {
                        add_player_metric(
                            &mut players,
                            setup,
                            player_id,
                            event.turn_number,
                            "instantsSorceriesCast",
                            1.0,
                        );
                    }
                }
            }
            "permanentEnteredBattlefield" => {
                if event.detail["wasCreature"].as_bool() == Some(true)
                    && let Some(player_id) = &event.player_id
                {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "creaturesEntered",
                        1.0,
                    );
                }
            }
            "permanentDied" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "permanentsToGraveyard",
                        1.0,
                    );
                    if event.detail["reason"].as_str() == Some("sacrificed") {
                        add_player_metric(
                            &mut players,
                            setup,
                            player_id,
                            event.turn_number,
                            "sacrifices",
                            1.0,
                        );
                    }
                    if event.detail["wasCreature"].as_bool() == Some(true) {
                        add_player_metric(
                            &mut players,
                            setup,
                            player_id,
                            event.turn_number,
                            "creaturesDied",
                            1.0,
                        );
                        if event.detail["reason"].as_str() == Some("lethalDamage")
                            && event.step == GameStep::CombatDamage
                        {
                            add_player_metric(
                                &mut players,
                                setup,
                                player_id,
                                event.turn_number,
                                "creaturesLostInCombat",
                                1.0,
                            );
                            if let Some(card_id) = &event.card_instance_id
                                && let Some(source_players) = combat_sources_by_target
                                    .get(&(event.turn_number, card_id.clone()))
                            {
                                for source_player in source_players {
                                    if source_player != player_id {
                                        add_player_metric(
                                            &mut players,
                                            setup,
                                            source_player,
                                            event.turn_number,
                                            "creaturesKilledInCombat",
                                            1.0,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "permanentExiled" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "permanentsToExile",
                        1.0,
                    );
                }
            }
            "lifeGained" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "lifeGained",
                        json_amount(&event.detail, "amount"),
                    );
                }
            }
            "lifeLost" | "lifePaid" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "lifeLost",
                        json_amount(&event.detail, "amount"),
                    );
                }
            }
            "damageDealt" => {
                let source_controller = event.detail["sourceControllerId"]
                    .as_str()
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        event.detail["source"]
                            .as_str()
                            .and_then(|source| source_controllers.get(source).cloned())
                    });
                if let Some(source_controller) = source_controller {
                    let amount = json_amount(&event.detail, "amount");
                    add_player_metric(
                        &mut players,
                        setup,
                        &source_controller,
                        event.turn_number,
                        "damage",
                        amount,
                    );
                    let combat = event.detail["combat"].as_bool().unwrap_or(false);
                    add_player_metric(
                        &mut players,
                        setup,
                        &source_controller,
                        event.turn_number,
                        if combat {
                            "combatDamage"
                        } else {
                            "nonCombatDamage"
                        },
                        amount,
                    );
                    if combat && let Some(target_id) = &event.card_instance_id {
                        combat_sources_by_target
                            .entry((event.turn_number, target_id.clone()))
                            .or_default()
                            .insert(source_controller);
                    }
                }
            }
            "commanderDamageDealt" => {
                if let Some(controller_id) = event.detail["controllerId"].as_str() {
                    add_player_metric(
                        &mut players,
                        setup,
                        controller_id,
                        event.turn_number,
                        "commanderDamage",
                        json_amount(&event.detail, "amount"),
                    );
                }
            }
            "attackerDeclared" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "attacks",
                        1.0,
                    );
                }
            }
            "blockerDeclared" => {
                if let Some(player_id) = &event.player_id {
                    add_player_metric(
                        &mut players,
                        setup,
                        player_id,
                        event.turn_number,
                        "blocks",
                        1.0,
                    );
                }
            }
            "abilityFizzled" => {
                if let Some(controller_id) = &event.player_id {
                    for player_id in players.keys().cloned().collect::<Vec<_>>() {
                        if player_id != *controller_id {
                            add_player_metric(
                                &mut players,
                                setup,
                                &player_id,
                                event.turn_number,
                                "opponentAbilitiesFizzledOrCountered",
                                1.0,
                            );
                        }
                    }
                }
            }
            "spellCountered" if event.detail["objectKind"].as_str() != Some("spell") => {
                if let Some(controller_id) = &event.player_id {
                    for player_id in players.keys().cloned().collect::<Vec<_>>() {
                        if player_id != *controller_id {
                            add_player_metric(
                                &mut players,
                                setup,
                                &player_id,
                                event.turn_number,
                                "opponentAbilitiesFizzledOrCountered",
                                1.0,
                            );
                        }
                    }
                }
            }
            "playerLost" => {
                if let Some(player_id) = &event.player_id
                    && let Some(player) = players.get_mut(player_id)
                {
                    player.elimination_round = Some(round_number(setup, event.turn_number));
                }
            }
            _ => {}
        }
    }

    let winner = final_state
        .outcome
        .as_ref()
        .and_then(|outcome| outcome.winner.as_deref());
    let win_turn = final_state
        .outcome
        .as_ref()
        .map(|outcome| outcome.turn_number);
    let lost_players = final_state
        .outcome
        .as_ref()
        .map(|outcome| outcome.losers.iter().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let player_count = setup.players.len();
    let rounds_played = rounds_played(setup, snapshots, final_state);
    let report_players = setup
        .players
        .iter()
        .map(|player| {
            let deck_name = normalized_deck_name(&player.name);
            let builder = players.remove(&player.id).unwrap_or_default();
            let turns = builder
                .turns
                .into_iter()
                .filter(|(_, turn)| turn.observed)
                .map(|(round_number, mut turn)| {
                    for (key, _) in METRIC_DEFINITIONS {
                        turn.metrics.entry((*key).to_string()).or_default();
                    }
                    PlayerTurnAnalytics {
                        round_number,
                        metrics: turn.metrics,
                    }
                })
                .collect();
            PlayerGameAnalytics {
                player_id: player.id.clone(),
                deck_id: analytics_deck_id(&player.id, &deck_name, deck_session_by_player_id),
                deck_name,
                pilot_id: pilot_by_player_id
                    .get(&player.id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
                player_count,
                rounds_played,
                turns,
                won: winner == Some(player.id.as_str()),
                lost: lost_players.contains(&player.id),
                elimination_round: builder.elimination_round,
                win_round: (winner == Some(player.id.as_str()))
                    .then(|| win_turn.map(|turn| round_number(setup, turn)))
                    .flatten(),
            }
        })
        .collect();

    GameAnalyticsReport {
        context_id: LEGACY_ANALYTICS_CONTEXT.to_string(),
        game_mode: final_state.game_mode,
        status: final_state.status.clone(),
        players: report_players,
        set_winner_player_id: None,
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateRow {
    turn_count: u64,
    metric_sums: BTreeMap<String, f64>,
}

impl AggregateRow {
    fn add_turn(&mut self, metrics: &BTreeMap<String, f64>) {
        self.turn_count += 1;
        for (key, value) in metrics {
            add_metric(&mut self.metric_sums, key, *value);
        }
    }

    fn merge(&mut self, other: &AggregateRow) {
        self.turn_count += other.turn_count;
        for (key, value) in &other.metric_sums {
            add_metric(&mut self.metric_sums, key, *value);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StratumAggregate {
    #[serde(default = "default_legacy_analytics_context")]
    context_id: String,
    deck_id: String,
    deck_name: String,
    pilot_id: String,
    player_count: usize,
    #[serde(default)]
    game_mode: GameMode,
    match_count: u64,
    #[serde(default)]
    rounds_played_sum: u64,
    rated_match_count: u64,
    wins: u64,
    losses: u64,
    #[serde(default)]
    set_wins: u64,
    #[serde(default)]
    set_losses: u64,
    elimination_turn_sum: f64,
    elimination_count: u64,
    win_turn_sum: f64,
    win_count: u64,
    #[serde(default = "default_plackett_luce_mu")]
    plackett_luce_mu: f64,
    #[serde(default = "default_plackett_luce_sigma")]
    plackett_luce_sigma: f64,
    #[serde(
        default,
        rename = "plackettLuceRating",
        alias = "elo",
        skip_serializing
    )]
    legacy_elo_rating: Option<f64>,
    total: AggregateRow,
    turns: BTreeMap<u32, AggregateRow>,
}

impl StratumAggregate {
    fn new(player: &PlayerGameAnalytics, game_mode: GameMode, context_id: &str) -> Self {
        Self {
            context_id: context_id.to_string(),
            deck_id: player.deck_id.clone(),
            deck_name: player.deck_name.clone(),
            pilot_id: player.pilot_id.clone(),
            player_count: player.player_count,
            game_mode,
            match_count: 0,
            rounds_played_sum: 0,
            rated_match_count: 0,
            wins: 0,
            losses: 0,
            set_wins: 0,
            set_losses: 0,
            elimination_turn_sum: 0.0,
            elimination_count: 0,
            win_turn_sum: 0.0,
            win_count: 0,
            plackett_luce_mu: DEFAULT_PLACKETT_LUCE_MU,
            plackett_luce_sigma: DEFAULT_PLACKETT_LUCE_SIGMA,
            legacy_elo_rating: None,
            total: AggregateRow::default(),
            turns: BTreeMap::new(),
        }
    }
}

fn default_plackett_luce_mu() -> f64 {
    DEFAULT_PLACKETT_LUCE_MU
}

fn default_plackett_luce_sigma() -> f64 {
    DEFAULT_PLACKETT_LUCE_SIGMA
}

impl StratumAggregate {
    fn plackett_luce_ordinal(&self) -> f64 {
        self.plackett_luce_mu - 3.0 * self.plackett_luce_sigma
    }

    fn migrate_legacy_elo(&mut self) {
        let Some(legacy_elo) = self.legacy_elo_rating.take() else {
            return;
        };
        // Preserve the historical ordering while moving from the old Elo-shaped
        // scalar to the same mu/sigma scale used by the training leaderboard.
        self.plackett_luce_mu = DEFAULT_PLACKETT_LUCE_MU
            + (legacy_elo - LEGACY_ELO_BASELINE) / LEGACY_ELO_LEARNING_RATE;
        self.plackett_luce_sigma = (DEFAULT_PLACKETT_LUCE_SIGMA
            * PLACKETT_LUCE_SIGMA_DECAY.powf(self.rated_match_count as f64))
        .max(MINIMUM_PLACKETT_LUCE_SIGMA);
    }
}

fn default_legacy_analytics_context() -> String {
    LEGACY_ANALYTICS_CONTEXT.to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDeckAnalytics {
    schema_version: String,
    revision: u64,
    recorded_games: u64,
    #[serde(default)]
    recorded_games_by_context: BTreeMap<String, u64>,
    strata: BTreeMap<String, StratumAggregate>,
}

impl Default for PersistedDeckAnalytics {
    fn default() -> Self {
        Self {
            schema_version: ANALYTICS_SCHEMA_VERSION.to_string(),
            revision: 0,
            recorded_games: 0,
            recorded_games_by_context: BTreeMap::new(),
            strata: BTreeMap::new(),
        }
    }
}

fn game_mode_id(game_mode: GameMode) -> &'static str {
    match game_mode {
        GameMode::Free => "free",
        GameMode::Legacy => "legacy",
        GameMode::Commander => "commander",
        GameMode::DuelCommander => "duelCommander",
        GameMode::Training => "training",
        GameMode::Training2 => "training2",
    }
}

fn game_mode_label(game_mode: GameMode) -> &'static str {
    match game_mode {
        GameMode::Free => "Libre",
        GameMode::Legacy => "Legacy",
        GameMode::Commander => "Commander",
        GameMode::DuelCommander => "Duel Commander",
        GameMode::Training => "Entraînement simplifié",
        GameMode::Training2 => "Training 2",
    }
}

fn analytics_context_label(context_id: &str) -> String {
    if context_id == PLAYER_MATCH_ANALYTICS_CONTEXT {
        return "Matchs de joueurs".to_string();
    }
    if context_id == LEGACY_ANALYTICS_CONTEXT {
        return "Historique non classé".to_string();
    }
    if let Some(model_id) = context_id.strip_prefix("training:") {
        let model_label = pilot_definition(model_id)
            .map(|definition| definition.label)
            .unwrap_or(model_id);
        return format!("Entraînement · {model_label}");
    }
    if context_id == "multi-model-evaluation" {
        return "Évaluation multi-modèle · à venir".to_string();
    }
    context_id.to_string()
}

fn stratum_key(player: &PlayerGameAnalytics, game_mode: GameMode, context_id: &str) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        player.deck_id,
        player.pilot_id,
        player.player_count,
        game_mode_id(game_mode),
        context_id,
    )
}

fn persisted_stratum_key(stratum: &StratumAggregate) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        stratum.deck_id,
        stratum.pilot_id,
        stratum.player_count,
        game_mode_id(stratum.game_mode),
        stratum.context_id,
    )
}

fn plackett_luce_first_place_expected_scores(mus: &[f64]) -> Vec<f64> {
    let strengths = mus
        .iter()
        .map(|mu| (*mu / PLACKETT_LUCE_BETA).clamp(-30.0, 30.0).exp())
        .collect::<Vec<_>>();
    let total = strengths.iter().sum::<f64>();
    if total <= 0.0 {
        return vec![1.0 / mus.len() as f64; mus.len()];
    }
    strengths
        .into_iter()
        .map(|strength| strength / total)
        .collect()
}

fn multiplayer_actual_scores(players: &[PlayerGameAnalytics]) -> Vec<f64> {
    let winner_count = players.iter().filter(|player| player.won).count();
    if winner_count > 0 {
        return players
            .iter()
            .map(|player| {
                if player.won {
                    1.0 / winner_count as f64
                } else {
                    0.0
                }
            })
            .collect();
    }
    vec![1.0 / players.len() as f64; players.len()]
}

impl PersistedDeckAnalytics {
    fn backfill_legacy_rounds_played(&mut self) {
        for stratum in self.strata.values_mut() {
            if stratum.match_count > 0 && stratum.rounds_played_sum == 0 {
                // The v2 format originally inferred match duration from the number of
                // per-player snapshots. Preserve that historical approximation because
                // the exact final round was not persisted for those games.
                stratum.rounds_played_sum = stratum.total.turn_count;
            }
        }
    }

    fn ingest(&mut self, report: GameAnalyticsReport) {
        if report.players.is_empty() {
            return;
        }
        let keys = report
            .players
            .iter()
            .map(|player| stratum_key(player, report.game_mode, &report.context_id))
            .collect::<Vec<_>>();
        for (key, player) in keys.iter().zip(&report.players) {
            self.strata.entry(key.clone()).or_insert_with(|| {
                StratumAggregate::new(player, report.game_mode, &report.context_id)
            });
        }

        if report.status == GameStatus::Completed && report.players.len() > 1 {
            let mus = keys
                .iter()
                .map(|key| self.strata[key].plackett_luce_mu)
                .collect::<Vec<_>>();
            let expected_scores = plackett_luce_first_place_expected_scores(&mus);
            let actual_scores = multiplayer_actual_scores(&report.players);
            let deltas = actual_scores
                .iter()
                .zip(expected_scores)
                .map(|(actual, expected)| PLACKETT_LUCE_LEARNING_RATE * (actual - expected))
                .collect::<Vec<_>>();
            let mut delta_by_key = BTreeMap::<String, f64>::new();
            for (key, delta) in keys.iter().zip(deltas) {
                *delta_by_key.entry(key.clone()).or_default() += delta;
            }
            for (key, delta) in delta_by_key {
                if let Some(stratum) = self.strata.get_mut(&key) {
                    stratum.plackett_luce_mu += delta;
                    stratum.plackett_luce_sigma = (stratum.plackett_luce_sigma
                        * PLACKETT_LUCE_SIGMA_DECAY)
                        .max(MINIMUM_PLACKETT_LUCE_SIGMA);
                }
            }
            for key in &keys {
                self.strata
                    .get_mut(key)
                    .expect("stratum exists")
                    .rated_match_count += 1;
            }
        }

        for (key, player) in keys.iter().zip(report.players) {
            let stratum = self.strata.get_mut(key).expect("stratum exists");
            stratum.match_count += 1;
            stratum.rounds_played_sum += u64::from(player.rounds_played);
            stratum.wins += u64::from(player.won);
            stratum.losses += u64::from(player.lost);
            if let Some(set_winner) = report.set_winner_player_id.as_deref() {
                if set_winner == player.player_id {
                    stratum.set_wins += 1;
                } else {
                    stratum.set_losses += 1;
                }
            }
            if let Some(round) = player.elimination_round {
                stratum.elimination_turn_sum += f64::from(round);
                stratum.elimination_count += 1;
            }
            if let Some(round) = player.win_round {
                stratum.win_turn_sum += f64::from(round);
                stratum.win_count += 1;
            }
            for round in player.turns {
                stratum.total.add_turn(&round.metrics);
                stratum
                    .turns
                    .entry(round.round_number)
                    .or_default()
                    .add_turn(&round.metrics);
            }
        }
        self.recorded_games += 1;
        *self
            .recorded_games_by_context
            .entry(report.context_id.clone())
            .or_default() += 1;
        self.revision += 1;
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckAnalyticsQuery {
    #[serde(default)]
    pub analytics_context_id: Option<String>,
    #[serde(default)]
    pub pilot_id: Option<String>,
    #[serde(default)]
    pub pilot_ids: Option<Vec<String>>,
    #[serde(default)]
    pub creator_ids: Option<Vec<String>>,
    #[serde(default)]
    pub player_count: Option<usize>,
    #[serde(default)]
    pub game_mode: Option<GameMode>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsFilterOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsPilotOption {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub capabilities: PilotCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_id: Option<String>,
    pub pilot_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDefinitionView {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricValueView {
    pub sum: f64,
    pub per_round: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsRowView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_number: Option<u32>,
    pub round_count: u64,
    pub metrics: BTreeMap<String, MetricValueView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckAnalyticsView {
    pub row_id: String,
    pub rank: usize,
    pub deck_id: String,
    pub deck_name: String,
    pub creator: String,
    pub pilot_id: String,
    pub plackett_luce_mu: f64,
    pub plackett_luce_sigma: f64,
    pub plackett_luce_ordinal: f64,
    pub matches: u64,
    pub wins: u64,
    pub losses: u64,
    pub set_wins: u64,
    pub set_losses: u64,
    pub unresolved_matches: u64,
    pub average_rounds: f64,
    pub average_elimination_round: Option<f64>,
    pub average_win_round: Option<f64>,
    pub all_rounds: AnalyticsRowView,
    pub rounds: Vec<AnalyticsRowView>,
    pub profile_metric_order: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanLeaderboardView {
    pub rank: usize,
    pub pilot_id: String,
    pub label: String,
    pub plackett_luce_mu: f64,
    pub plackett_luce_sigma: f64,
    pub plackett_luce_ordinal: f64,
    pub matches: u64,
    pub wins: u64,
    pub losses: u64,
    pub set_wins: u64,
    pub set_losses: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckAnalyticsResponse {
    pub schema_version: String,
    pub revision: u64,
    pub recorded_games: u64,
    pub total_recorded_games: u64,
    pub selected_analytics_context_id: Option<String>,
    pub selected_pilot_id: Option<String>,
    pub selected_pilot_ids: Option<Vec<String>>,
    pub selected_creator_ids: Option<Vec<String>>,
    pub selected_player_count: Option<usize>,
    pub selected_game_mode: Option<GameMode>,
    pub pilots: Vec<AnalyticsPilotOption>,
    pub creators: Vec<AnalyticsFilterOption>,
    pub player_counts: Vec<usize>,
    pub game_modes: Vec<AnalyticsFilterOption>,
    pub analytics_contexts: Vec<AnalyticsFilterOption>,
    pub metric_definitions: Vec<MetricDefinitionView>,
    pub metric_order: Vec<String>,
    pub decks: Vec<DeckAnalyticsView>,
    pub rating_system: String,
    pub human_leaderboard: Vec<HumanLeaderboardView>,
}

#[derive(Default)]
struct CombinedDeck {
    deck_id: String,
    deck_name: String,
    pilot_id: String,
    plackett_luce_mu_weighted_sum: f64,
    plackett_luce_sigma_weighted_sum: f64,
    plackett_luce_ordinal_weighted_sum: f64,
    matches: u64,
    rounds_played_sum: u64,
    wins: u64,
    losses: u64,
    set_wins: u64,
    set_losses: u64,
    elimination_turn_sum: f64,
    elimination_count: u64,
    win_turn_sum: f64,
    win_count: u64,
    total: AggregateRow,
    turns: BTreeMap<u32, AggregateRow>,
}

impl CombinedDeck {
    fn merge_stratum(&mut self, stratum: &StratumAggregate) {
        self.deck_id = stratum.deck_id.clone();
        self.deck_name = stratum.deck_name.clone();
        self.pilot_id = stratum.pilot_id.clone();
        self.plackett_luce_mu_weighted_sum += stratum.plackett_luce_mu * stratum.match_count as f64;
        self.plackett_luce_sigma_weighted_sum +=
            stratum.plackett_luce_sigma * stratum.match_count as f64;
        self.plackett_luce_ordinal_weighted_sum +=
            stratum.plackett_luce_ordinal() * stratum.match_count as f64;
        self.matches += stratum.match_count;
        self.rounds_played_sum += stratum.rounds_played_sum;
        self.wins += stratum.wins;
        self.losses += stratum.losses;
        self.set_wins += stratum.set_wins;
        self.set_losses += stratum.set_losses;
        self.elimination_turn_sum += stratum.elimination_turn_sum;
        self.elimination_count += stratum.elimination_count;
        self.win_turn_sum += stratum.win_turn_sum;
        self.win_count += stratum.win_count;
        self.total.merge(&stratum.total);
        for (turn_number, turn) in &stratum.turns {
            self.turns.entry(*turn_number).or_default().merge(turn);
        }
    }

    fn plackett_luce_mu(&self) -> f64 {
        if self.matches == 0 {
            DEFAULT_PLACKETT_LUCE_MU
        } else {
            self.plackett_luce_mu_weighted_sum / self.matches as f64
        }
    }

    fn plackett_luce_sigma(&self) -> f64 {
        if self.matches == 0 {
            DEFAULT_PLACKETT_LUCE_SIGMA
        } else {
            self.plackett_luce_sigma_weighted_sum / self.matches as f64
        }
    }

    fn plackett_luce_ordinal(&self) -> f64 {
        if self.matches == 0 {
            DEFAULT_PLACKETT_LUCE_MU - 3.0 * DEFAULT_PLACKETT_LUCE_SIGMA
        } else {
            self.plackett_luce_ordinal_weighted_sum / self.matches as f64
        }
    }

    fn average_rounds(&self) -> f64 {
        if self.matches == 0 {
            0.0
        } else {
            self.rounds_played_sum as f64 / self.matches as f64
        }
    }
}

fn combine_strata<'a>(
    strata: impl Iterator<Item = &'a StratumAggregate>,
    separate_pilots: bool,
) -> BTreeMap<String, CombinedDeck> {
    let mut combined = BTreeMap::<String, CombinedDeck>::new();
    for stratum in strata {
        let key = if separate_pilots {
            format!("{}\u{1f}{}", stratum.deck_id, stratum.pilot_id)
        } else {
            stratum.deck_id.clone()
        };
        combined.entry(key).or_default().merge_stratum(stratum);
    }
    combined
}

fn row_view(round_number: Option<u32>, row: &AggregateRow) -> AnalyticsRowView {
    let metrics = METRIC_DEFINITIONS
        .iter()
        .map(|(key, _)| {
            let sum = row.metric_sums.get(*key).copied().unwrap_or_default();
            let per_round = if row.turn_count == 0 {
                0.0
            } else {
                sum / row.turn_count as f64
            };
            ((*key).to_string(), MetricValueView { sum, per_round })
        })
        .collect();
    AnalyticsRowView {
        round_number,
        round_count: row.turn_count,
        metrics,
    }
}

fn descending_metric_order(row: &AnalyticsRowView) -> Vec<String> {
    let mut keys = METRIC_DEFINITIONS
        .iter()
        .map(|(key, _)| (*key).to_string())
        .collect::<Vec<_>>();
    keys.sort_by(|first, second| {
        row.metrics[second]
            .per_round
            .partial_cmp(&row.metrics[first].per_round)
            .unwrap_or(Ordering::Equal)
            .then_with(|| first.cmp(second))
    });
    keys
}

fn creator_for_deck<'a>(deck_id: &str, deck_creators: &'a BTreeMap<String, String>) -> &'a str {
    deck_creators
        .get(deck_id)
        .map(String::as_str)
        .filter(|creator| !creator.trim().is_empty())
        .unwrap_or(UNKNOWN_DECK_CREATOR)
}

#[cfg(test)]
fn query_dataset(
    dataset: &PersistedDeckAnalytics,
    query: DeckAnalyticsQuery,
) -> DeckAnalyticsResponse {
    query_dataset_with_creators(dataset, query, &BTreeMap::new())
}

fn query_dataset_with_creators(
    dataset: &PersistedDeckAnalytics,
    query: DeckAnalyticsQuery,
    deck_creators: &BTreeMap<String, String>,
) -> DeckAnalyticsResponse {
    let selected_analytics_context_id = query
        .analytics_context_id
        .clone()
        .filter(|context_id| !context_id.trim().is_empty())
        .unwrap_or_else(|| PLAYER_MATCH_ANALYTICS_CONTEXT.to_string());
    let selected_pilot_ids = query.pilot_ids.as_ref().map(|pilots| {
        pilots
            .iter()
            .map(|pilot| pilot.trim().to_string())
            .filter(|pilot| !pilot.is_empty())
            .collect::<BTreeSet<_>>()
    });
    let selected_creator_ids = query.creator_ids.as_ref().map(|creators| {
        creators
            .iter()
            .map(|creator| creator.trim().to_string())
            .filter(|creator| !creator.is_empty())
            .collect::<BTreeSet<_>>()
    });
    let observed_pilots = dataset
        .strata
        .values()
        .map(|stratum| stratum.pilot_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut pilots = PILOT_DEFINITIONS
        .iter()
        .filter(|definition| definition.capabilities.deck_stats)
        .map(|definition| AnalyticsPilotOption {
            id: definition.id.to_string(),
            label: definition.label.to_string(),
            kind: definition.kind.to_string(),
            capabilities: definition.capabilities,
            controller_id: definition.controller_id.map(str::to_string),
            pilot_id: definition.pilot_id.to_string(),
        })
        .collect::<Vec<_>>();
    for pilot in observed_pilots {
        if pilot_definition(&pilot).is_some() {
            continue;
        }
        let (label, kind) = pilot
            .strip_prefix("human:")
            .map(|username| (username.to_string(), "human".to_string()))
            .unwrap_or_else(|| (pilot.clone(), "external".to_string()));
        pilots.push(AnalyticsPilotOption {
            id: pilot.clone(),
            label,
            kind,
            capabilities: PilotCapabilities {
                play: false,
                deck_stats: true,
                training: false,
            },
            controller_id: None,
            pilot_id: pilot,
        });
    }
    pilots.sort_by(|left, right| left.label.cmp(&right.label));
    let mut observed_creators = deck_creators
        .values()
        .map(|creator| creator.trim())
        .filter(|creator| !creator.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    observed_creators.extend(
        dataset
            .strata
            .values()
            .map(|stratum| creator_for_deck(&stratum.deck_id, deck_creators).to_string()),
    );
    let creators = observed_creators
        .into_iter()
        .map(|creator| AnalyticsFilterOption {
            id: creator.clone(),
            label: creator,
        })
        .collect();
    let player_counts = dataset
        .strata
        .values()
        .map(|stratum| stratum.player_count)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let game_modes = dataset
        .strata
        .values()
        .map(|stratum| stratum.game_mode)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|game_mode| AnalyticsFilterOption {
            id: game_mode_id(game_mode).to_string(),
            label: game_mode_label(game_mode).to_string(),
        })
        .collect();
    let mut observed_contexts = dataset
        .strata
        .values()
        .map(|stratum| stratum.context_id.clone())
        .collect::<BTreeSet<_>>();
    observed_contexts.insert(PLAYER_MATCH_ANALYTICS_CONTEXT.to_string());
    observed_contexts.insert("multi-model-evaluation".to_string());
    observed_contexts.extend(training_pilots().map(|pilot| format!("training:{}", pilot.pilot_id)));
    let analytics_contexts = observed_contexts
        .into_iter()
        .map(|context_id| AnalyticsFilterOption {
            label: analytics_context_label(&context_id),
            id: context_id,
        })
        .collect();
    let ranking_decks = combine_strata(
        dataset.strata.values().filter(|stratum| {
            query
                .player_count
                .is_none_or(|count| count == stratum.player_count)
                && query
                    .game_mode
                    .is_none_or(|game_mode| game_mode == stratum.game_mode)
                && stratum.context_id == selected_analytics_context_id
                && selected_creator_ids.as_ref().is_none_or(|creators| {
                    creators.contains(creator_for_deck(&stratum.deck_id, deck_creators))
                })
        }),
        true,
    );
    let mut ranked_decks = ranking_decks.into_values().collect::<Vec<_>>();
    ranked_decks.sort_by(|first, second| {
        second
            .plackett_luce_ordinal()
            .partial_cmp(&first.plackett_luce_ordinal())
            .unwrap_or(Ordering::Equal)
            .then_with(|| first.deck_name.cmp(&second.deck_name))
            .then_with(|| first.pilot_id.cmp(&second.pilot_id))
    });
    let rank_by_deck_pilot = ranked_decks
        .iter()
        .enumerate()
        .map(|(index, deck)| ((deck.deck_id.clone(), deck.pilot_id.clone()), index + 1))
        .collect::<BTreeMap<_, _>>();

    let combined = combine_strata(
        dataset.strata.values().filter(|stratum| {
            selected_pilot_ids.as_ref().map_or_else(
                || {
                    query
                        .pilot_id
                        .as_ref()
                        .is_none_or(|pilot| pilot == &stratum.pilot_id)
                },
                |pilots| pilots.contains(&stratum.pilot_id),
            ) && query
                .player_count
                .is_none_or(|count| count == stratum.player_count)
                && query
                    .game_mode
                    .is_none_or(|game_mode| game_mode == stratum.game_mode)
                && stratum.context_id == selected_analytics_context_id
                && selected_creator_ids.as_ref().is_none_or(|creators| {
                    creators.contains(creator_for_deck(&stratum.deck_id, deck_creators))
                })
        }),
        true,
    );

    let mut decks = combined
        .into_values()
        .map(|deck| {
            let creator = creator_for_deck(&deck.deck_id, deck_creators).to_string();
            let all_turns = row_view(None, &deck.total);
            let profile_metric_order = descending_metric_order(&all_turns);
            let rank = rank_by_deck_pilot
                .get(&(deck.deck_id.clone(), deck.pilot_id.clone()))
                .copied()
                .unwrap_or_default();
            let plackett_luce_mu = deck.plackett_luce_mu();
            let plackett_luce_sigma = deck.plackett_luce_sigma();
            let plackett_luce_ordinal = deck.plackett_luce_ordinal();
            let average_rounds = deck.average_rounds();
            DeckAnalyticsView {
                row_id: format!("{}::pilot::{}", deck.deck_id, deck.pilot_id),
                rank,
                deck_id: deck.deck_id,
                deck_name: deck.deck_name,
                creator,
                pilot_id: deck.pilot_id,
                plackett_luce_mu,
                plackett_luce_sigma,
                plackett_luce_ordinal,
                matches: deck.matches,
                wins: deck.wins,
                losses: deck.losses,
                set_wins: deck.set_wins,
                set_losses: deck.set_losses,
                unresolved_matches: deck
                    .matches
                    .saturating_sub(deck.wins.saturating_add(deck.losses)),
                average_rounds,
                average_elimination_round: (deck.elimination_count > 0)
                    .then(|| deck.elimination_turn_sum / deck.elimination_count as f64),
                average_win_round: (deck.win_count > 0)
                    .then(|| deck.win_turn_sum / deck.win_count as f64),
                all_rounds: all_turns,
                rounds: deck
                    .turns
                    .iter()
                    .map(|(turn_number, row)| row_view(Some(*turn_number), row))
                    .collect(),
                profile_metric_order,
            }
        })
        .collect::<Vec<_>>();
    decks.sort_by(|first, second| {
        first
            .rank
            .cmp(&second.rank)
            .then_with(|| first.deck_name.cmp(&second.deck_name))
            .then_with(|| first.pilot_id.cmp(&second.pilot_id))
    });

    let mut global_row = AggregateRow::default();
    for deck in &decks {
        global_row.turn_count += deck.all_rounds.round_count;
        for (key, value) in &deck.all_rounds.metrics {
            add_metric(&mut global_row.metric_sums, key, value.sum);
        }
    }
    let metric_order = descending_metric_order(&row_view(None, &global_row));

    let mut human_totals = BTreeMap::<String, (f64, f64, f64, u64, u64, u64, u64, u64)>::new();
    for stratum in dataset.strata.values().filter(|stratum| {
        stratum.context_id == PLAYER_MATCH_ANALYTICS_CONTEXT
            && (stratum.pilot_id == "human"
                || stratum.pilot_id == "network-human"
                || stratum.pilot_id.starts_with("human:"))
    }) {
        let entry = human_totals.entry(stratum.pilot_id.clone()).or_default();
        entry.0 += stratum.plackett_luce_mu * stratum.match_count as f64;
        entry.1 += stratum.plackett_luce_sigma * stratum.match_count as f64;
        entry.2 += stratum.plackett_luce_ordinal() * stratum.match_count as f64;
        entry.3 += stratum.match_count;
        entry.4 += stratum.wins;
        entry.5 += stratum.losses;
        entry.6 += stratum.set_wins;
        entry.7 += stratum.set_losses;
    }
    let mut human_leaderboard = human_totals
        .into_iter()
        .map(
            |(
                pilot_id,
                (mu_sum, sigma_sum, ordinal_sum, matches, wins, losses, set_wins, set_losses),
            )| {
                HumanLeaderboardView {
                    rank: 0,
                    label: pilot_id
                        .strip_prefix("human:")
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            if pilot_id == "network-human" {
                                "Réseau (historique)".to_string()
                            } else {
                                "Humain local".to_string()
                            }
                        }),
                    plackett_luce_mu: if matches == 0 {
                        DEFAULT_PLACKETT_LUCE_MU
                    } else {
                        mu_sum / matches as f64
                    },
                    plackett_luce_sigma: if matches == 0 {
                        DEFAULT_PLACKETT_LUCE_SIGMA
                    } else {
                        sigma_sum / matches as f64
                    },
                    plackett_luce_ordinal: if matches == 0 {
                        DEFAULT_PLACKETT_LUCE_MU - 3.0 * DEFAULT_PLACKETT_LUCE_SIGMA
                    } else {
                        ordinal_sum / matches as f64
                    },
                    pilot_id,
                    matches,
                    wins,
                    losses,
                    set_wins,
                    set_losses,
                }
            },
        )
        .collect::<Vec<_>>();
    human_leaderboard.sort_by(|left, right| {
        right
            .plackett_luce_ordinal
            .partial_cmp(&left.plackett_luce_ordinal)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.label.cmp(&right.label))
    });
    for (index, player) in human_leaderboard.iter_mut().enumerate() {
        player.rank = index + 1;
    }

    DeckAnalyticsResponse {
        schema_version: ANALYTICS_SCHEMA_VERSION.to_string(),
        revision: dataset.revision,
        recorded_games: dataset
            .recorded_games_by_context
            .get(&selected_analytics_context_id)
            .copied()
            .unwrap_or(0),
        total_recorded_games: dataset.recorded_games,
        selected_analytics_context_id: Some(selected_analytics_context_id),
        selected_pilot_id: query.pilot_id,
        selected_pilot_ids: query.pilot_ids,
        selected_creator_ids: query.creator_ids,
        selected_player_count: query.player_count,
        selected_game_mode: query.game_mode,
        pilots,
        creators,
        player_counts,
        game_modes,
        analytics_contexts,
        metric_definitions: METRIC_DEFINITIONS
            .iter()
            .map(|(key, label)| MetricDefinitionView {
                key: (*key).to_string(),
                label: (*label).to_string(),
            })
            .collect(),
        metric_order,
        decks,
        rating_system: "plackett-luce".to_string(),
        human_leaderboard,
    }
}

fn load_dataset(path: &Path) -> PersistedDeckAnalytics {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .filter(|dataset: &PersistedDeckAnalytics| {
            dataset.schema_version == ANALYTICS_SCHEMA_VERSION
        })
        .map(|mut dataset| {
            dataset.backfill_legacy_rounds_played();
            for stratum in dataset.strata.values_mut() {
                stratum.migrate_legacy_elo();
            }
            if dataset.recorded_games_by_context.is_empty() && dataset.recorded_games > 0 {
                dataset
                    .recorded_games_by_context
                    .insert(LEGACY_ANALYTICS_CONTEXT.to_string(), dataset.recorded_games);
            }
            dataset.strata = std::mem::take(&mut dataset.strata)
                .into_values()
                .map(|stratum| (persisted_stratum_key(&stratum), stratum))
                .collect();
            dataset
        })
        .unwrap_or_default()
}

fn persist_dataset(path: &Path, dataset: &PersistedDeckAnalytics) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = fs::File::create(&temporary)?;
    serde_json::to_writer(&mut file, dataset)?;
    file.flush()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)
}

#[derive(Clone)]
pub struct DeckAnalyticsService {
    dataset: Arc<Mutex<PersistedDeckAnalytics>>,
    sender: mpsc::Sender<GameAnalyticsReport>,
}

impl DeckAnalyticsService {
    pub fn new(path: PathBuf) -> Self {
        let dataset = Arc::new(Mutex::new(load_dataset(&path)));
        let (sender, receiver) = mpsc::channel::<GameAnalyticsReport>();
        let worker_dataset = Arc::clone(&dataset);
        thread::Builder::new()
            .name("mtg-deck-analytics".to_string())
            .spawn(move || {
                while let Ok(report) = receiver.recv() {
                    let mut dataset = worker_dataset.lock().expect("analytics dataset lock");
                    dataset.ingest(report);
                    if let Err(error) = persist_dataset(&path, &dataset) {
                        eprintln!("failed to persist deck analytics: {error}");
                    }
                }
            })
            .expect("deck analytics worker starts");
        Self { dataset, sender }
    }

    pub fn from_env() -> Self {
        let path = std::env::var("MTG_DECK_ANALYTICS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("runs/deck-analytics.json"));
        Self::new(path)
    }

    pub fn submit(&self, report: GameAnalyticsReport) {
        if self.sender.send(report).is_err() {
            eprintln!("deck analytics worker is unavailable");
        }
    }

    pub fn query(&self, query: DeckAnalyticsQuery) -> DeckAnalyticsResponse {
        let deck_creators = crate::local_app::local_deck_creators().unwrap_or_default();
        query_dataset_with_creators(
            &self.dataset.lock().expect("analytics dataset lock"),
            query,
            &deck_creators,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{CardDefinition, CardInstance, CombatState, EnginePlayer, GameMode};

    fn setup() -> GameSetup {
        GameSetup {
            players: vec![
                crate::engine::PlayerDeck {
                    id: "player-1".to_string(),
                    name: "First".to_string(),
                    starting_life: 20,
                    cards: Vec::new(),
                },
                crate::engine::PlayerDeck {
                    id: "player-2".to_string(),
                    name: "Second".to_string(),
                    starting_life: 20,
                    cards: Vec::new(),
                },
            ],
            opening_hand_size: 7,
            starting_player: 0,
        }
    }

    #[test]
    fn analytics_use_the_meta_session_id_as_the_deck_identity() {
        let sessions = BTreeMap::from([("player-1".to_string(), "session-2f9a".to_string())]);

        assert_eq!(
            analytics_deck_id("player-1", "Renamed Deck", &sessions),
            "session-2f9a"
        );
        assert_eq!(
            analytics_deck_id("player-2", "Fallback Deck", &sessions),
            "fallback-deck"
        );
    }

    #[test]
    fn rounds_are_comparable_across_player_counts() {
        for player_count in 2..=4 {
            let setup = GameSetup {
                players: (0..player_count)
                    .map(|index| crate::engine::PlayerDeck {
                        id: format!("player-{index}"),
                        name: format!("Player {index}"),
                        starting_life: 20,
                        cards: Vec::new(),
                    })
                    .collect(),
                opening_hand_size: 7,
                starting_player: 1 % player_count,
            };
            assert_eq!(round_number(&setup, 1 + 4 * player_count as u32), 5);
            assert_eq!(round_number(&setup, 5 * player_count as u32), 5);
        }
    }

    #[test]
    fn report_converts_win_and_elimination_to_rounds() {
        let setup = GameSetup {
            players: (0..4)
                .map(|index| crate::engine::PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: Vec::new(),
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        };
        let state = GameState {
            schema_version: "mtg-game/v1".to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            turn_number: 17,
            active_player: 0,
            priority_player: None,
            step: GameStep::Cleanup,
            players: (0..4)
                .map(|index| engine_player(&format!("player-{index}")))
                .collect(),
            game_pieces: Vec::new(),
            commanders: Vec::new(),
            stack: Vec::new(),
            combat: CombatState::default(),
            permissions: Vec::new(),
            rule_modifiers: Vec::new(),
            events: vec![crate::engine::GameEvent {
                sequence: 1,
                turn_number: 17,
                step: GameStep::Cleanup,
                kind: "playerLost".to_string(),
                player_id: Some("player-1".to_string()),
                card_instance_id: None,
                detail: json!({}),
            }],
            unsupported_rules: Vec::new(),
            outcome: Some(crate::engine::GameOutcome {
                winner: Some("player-0".to_string()),
                losers: vec!["player-1".to_string()],
                reason: crate::engine::GameEndReason::SpellOrAbility,
                turn_number: 17,
            }),
        };

        let report = build_game_analytics_report(
            &setup,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &[],
            &[],
            &state,
        );
        let winner = report
            .players
            .iter()
            .find(|player| player.player_id == "player-0")
            .unwrap();
        let loser = report
            .players
            .iter()
            .find(|player| player.player_id == "player-1")
            .unwrap();
        assert!(
            report
                .players
                .iter()
                .all(|player| player.rounds_played == 5)
        );
        assert_eq!(winner.win_round, Some(5));
        assert_eq!(loser.elimination_round, Some(5));

        let mut turn_limited_state = state;
        turn_limited_state.status = GameStatus::TurnLimitReached;
        turn_limited_state.turn_number = 5;
        turn_limited_state.outcome = None;
        assert_eq!(rounds_played(&setup, &[], &turn_limited_state), 1);
    }

    fn engine_player(id: &str) -> EnginePlayer {
        EnginePlayer {
            id: id.to_string(),
            name: id.to_string(),
            life: 20,
            has_lost: false,
            library: Vec::new(),
            hand: Vec::new(),
            battlefield: Vec::new(),
            graveyard: Vec::new(),
            exile: Vec::new(),
            sideboard: Vec::new(),
            command_zone: Vec::new(),
            commander_damage: Vec::new(),
            mana_pool: Vec::new(),
            counters: BTreeMap::new(),
            land_plays_remaining: 1,
            max_hand_size: 7,
        }
    }

    fn permanent_with_counters(counters: BTreeMap<String, i32>) -> CardInstance {
        CardInstance {
            instance_id: "permanent-1".to_string(),
            definition: CardDefinition {
                id: "counter-card".to_string(),
                name: "Counter Card".to_string(),
                type_line: "Artifact Creature".to_string(),
                is_commander: false,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                mana_cost: String::new(),
                power: Some("1".to_string()),
                toughness: Some("1".to_string()),
                rules: Vec::new(),
            },
            printed_definition: None,
            owner: "player-1".to_string(),
            controller: "player-1".to_string(),
            tapped: false,
            summoning_sick: false,
            damage_marked: 0,
            power_modifier: 0,
            toughness_modifier: 0,
            counters,
            flags: BTreeMap::new(),
            battle_protector: None,
            attached_to: None,
        }
    }

    fn player(
        deck: &str,
        pilot: &str,
        won: bool,
        lost: bool,
        turn_count: u32,
        damage_per_turn: f64,
    ) -> PlayerGameAnalytics {
        PlayerGameAnalytics {
            player_id: format!("{deck}-player"),
            deck_id: slug(deck),
            deck_name: deck.to_string(),
            pilot_id: pilot.to_string(),
            player_count: 2,
            rounds_played: turn_count,
            turns: (1..=turn_count)
                .map(|round_number| {
                    let mut metrics = empty_metric_sums();
                    add_metric(&mut metrics, "damage", damage_per_turn);
                    PlayerTurnAnalytics {
                        round_number,
                        metrics,
                    }
                })
                .collect(),
            won,
            lost,
            elimination_round: lost.then_some(turn_count),
            win_round: won.then_some(turn_count),
        }
    }

    #[test]
    fn metrics_are_summed_and_divided_only_by_observed_rounds() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "ia-v6", true, false, 2, 4.0),
                player("Control", "ia-v6", false, true, 2, 1.0),
            ],
        });

        let response = query_dataset(&dataset, DeckAnalyticsQuery::default());
        let aggro = response
            .decks
            .iter()
            .find(|deck| deck.deck_name == "Aggro")
            .expect("aggro deck");
        assert_eq!(aggro.all_rounds.round_count, 2);
        assert_eq!(aggro.all_rounds.metrics["damage"].sum, 8.0);
        assert_eq!(aggro.all_rounds.metrics["damage"].per_round, 4.0);
    }

    #[test]
    fn average_rounds_divides_rounds_played_by_matches_not_metric_snapshots() {
        let mut dataset = PersistedDeckAnalytics::default();
        for rounds_played in [2, 4] {
            let mut aggro = player("Aggro", "ia-v6", true, false, 1, 1.0);
            aggro.rounds_played = rounds_played;
            let mut control = player("Control", "ia-v6", false, true, 1, 1.0);
            control.rounds_played = rounds_played;
            dataset.ingest(GameAnalyticsReport {
                context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
                game_mode: GameMode::Free,
                status: GameStatus::Completed,
                set_winner_player_id: None,
                players: vec![aggro, control],
            });
        }

        let response = query_dataset(&dataset, DeckAnalyticsQuery::default());
        let aggro = response
            .decks
            .iter()
            .find(|deck| deck.deck_name == "Aggro")
            .expect("aggro deck");
        assert_eq!(aggro.matches, 2);
        assert_eq!(aggro.all_rounds.round_count, 2);
        assert_eq!(aggro.average_rounds, 3.0);
    }

    #[test]
    fn completed_games_update_plackett_luce_rating_from_field_expected_score() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Winner", "ia-v6", true, false, 1, 0.0),
                player("Loser A", "ia-v6", false, true, 1, 0.0),
                player("Loser B", "ia-v6", false, true, 1, 0.0),
                player("Loser C", "ia-v6", false, true, 1, 0.0),
            ],
        });

        let response = query_dataset(&dataset, DeckAnalyticsQuery::default());
        assert_eq!(response.decks[0].deck_name, "Winner");
        let winner_delta = response.decks[0].plackett_luce_mu - DEFAULT_PLACKETT_LUCE_MU;
        let loser_delta = DEFAULT_PLACKETT_LUCE_MU - response.decks[1].plackett_luce_mu;
        assert!((winner_delta - loser_delta * 3.0).abs() < 0.001);
        assert!((response.decks[0].plackett_luce_ordinal - 0.8125).abs() < 0.000_001);
        assert!((response.decks[0].plackett_luce_sigma - 8.3125).abs() < 0.000_001);
    }

    #[test]
    fn legacy_elo_is_migrated_to_plackett_luce_mu_sigma_and_ordinal() {
        let player = player("Legacy", "human", true, false, 1, 0.0);
        let mut stratum =
            StratumAggregate::new(&player, GameMode::Free, PLAYER_MATCH_ANALYTICS_CONTEXT);
        stratum.rated_match_count = 8;
        stratum.legacy_elo_rating = Some(1538.4);

        stratum.migrate_legacy_elo();

        assert!((stratum.plackett_luce_mu - 26.6).abs() < 0.000_001);
        assert!(stratum.plackett_luce_sigma < DEFAULT_PLACKETT_LUCE_SIGMA);
        assert!(stratum.plackett_luce_ordinal() > 1.6);
        let persisted = serde_json::to_value(&stratum).expect("stratum serializes");
        assert!(persisted.get("plackettLuceRating").is_none());
        assert_eq!(persisted["plackettLuceMu"], stratum.plackett_luce_mu);
    }

    #[test]
    fn analytics_service_publishes_completed_wins_to_queries() {
        let directory =
            std::env::temp_dir().join(format!("mtg-deck-analytics-wins-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("analytics test directory");
        let service = DeckAnalyticsService::new(directory.join("analytics.json"));
        service.submit(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Commander,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Winner", "ia-v6-in-training", true, false, 1, 0.0),
                player("Loser", "ia-v6-in-training", false, true, 1, 0.0),
            ],
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let response = loop {
            let response = service.query(DeckAnalyticsQuery::default());
            if response.revision == 1 {
                break response;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "analytics worker did not publish the game"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let winner = response
            .decks
            .iter()
            .find(|deck| deck.deck_name == "Winner")
            .expect("winner deck");
        assert_eq!(response.recorded_games, 1);
        assert_eq!(winner.matches, 1);
        assert_eq!(winner.wins, 1);
        assert_eq!(winner.losses, 0);
        assert_eq!(winner.unresolved_matches, 0);

        drop(service);
        fs::remove_dir_all(directory).expect("analytics test cleanup");
    }

    #[test]
    fn filters_keep_pilot_and_player_count_strata_independent() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "ia-v6", true, false, 1, 6.0),
                player("Control", "ia-v6", false, true, 1, 0.0),
            ],
        });
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "human", false, true, 1, 1.0),
                player("Control", "human", true, false, 1, 0.0),
            ],
        });

        let response = query_dataset(
            &dataset,
            DeckAnalyticsQuery {
                analytics_context_id: None,
                game_mode: None,
                pilot_id: Some("ia-v6".to_string()),
                pilot_ids: None,
                creator_ids: None,
                player_count: Some(2),
            },
        );
        let aggro = response
            .decks
            .iter()
            .find(|deck| deck.deck_name == "Aggro")
            .expect("aggro deck");
        assert_eq!(aggro.matches, 1);
        assert_eq!(aggro.wins, 1);
        assert_eq!(aggro.all_rounds.metrics["damage"].per_round, 6.0);
    }

    #[test]
    fn filters_accept_multiple_pilots_and_preserve_an_explicit_empty_selection() {
        let mut dataset = PersistedDeckAnalytics::default();
        for pilot in ["human", "ai-random", "ia-v10-in-training"] {
            dataset.ingest(GameAnalyticsReport {
                context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
                game_mode: GameMode::Free,
                status: GameStatus::Completed,
                set_winner_player_id: None,
                players: vec![
                    player("Aggro", pilot, true, false, 1, 4.0),
                    player("Control", pilot, false, true, 1, 0.0),
                ],
            });
        }

        let subset = query_dataset(
            &dataset,
            DeckAnalyticsQuery {
                analytics_context_id: None,
                game_mode: None,
                pilot_id: None,
                pilot_ids: Some(vec!["human".to_string(), "ia-v10-in-training".to_string()]),
                creator_ids: None,
                player_count: None,
            },
        );
        assert_eq!(
            subset
                .decks
                .iter()
                .map(|deck| deck.pilot_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["human", "ia-v10-in-training"])
        );

        let empty = query_dataset(
            &dataset,
            DeckAnalyticsQuery {
                analytics_context_id: None,
                game_mode: None,
                pilot_id: None,
                pilot_ids: Some(Vec::new()),
                creator_ids: None,
                player_count: None,
            },
        );
        assert!(empty.decks.is_empty());
    }

    #[test]
    fn ranking_assigns_a_unique_global_rank_to_each_deck_pilot_pair() {
        let mut dataset = PersistedDeckAnalytics::default();
        for _ in 0..2 {
            dataset.ingest(GameAnalyticsReport {
                context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
                game_mode: GameMode::Free,
                status: GameStatus::Completed,
                set_winner_player_id: None,
                players: vec![
                    player("Aggro", "ia-v8", true, false, 1, 6.0),
                    player("Control", "ia-v8", false, true, 1, 0.0),
                ],
            });
        }
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "human", false, true, 1, 1.0),
                player("Control", "human", true, false, 1, 0.0),
            ],
        });

        let all_pilots = query_dataset(&dataset, DeckAnalyticsQuery::default());
        assert_eq!(all_pilots.decks.len(), 4);
        assert_eq!(
            all_pilots
                .decks
                .iter()
                .filter(|row| row.deck_name == "Aggro")
                .map(|row| row.pilot_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["human", "ia-v8"])
        );
        assert_eq!(
            all_pilots
                .decks
                .iter()
                .map(|row| row.rank)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([1, 2, 3, 4])
        );

        let human = query_dataset(
            &dataset,
            DeckAnalyticsQuery {
                analytics_context_id: None,
                game_mode: None,
                pilot_id: Some("human".to_string()),
                pilot_ids: None,
                creator_ids: None,
                player_count: None,
            },
        );
        let aggro = human
            .decks
            .iter()
            .find(|row| row.deck_name == "Aggro")
            .expect("human Aggro row");
        let control = human
            .decks
            .iter()
            .find(|row| row.deck_name == "Control")
            .expect("human Control row");
        assert!(aggro.plackett_luce_ordinal < control.plackett_luce_ordinal);
        assert_eq!(aggro.rank, 3);
        assert_eq!(control.rank, 2);
    }

    #[test]
    fn filters_keep_free_and_commander_strata_independent() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "ia-v6", true, false, 1, 6.0),
                player("Control", "ia-v6", false, true, 1, 0.0),
            ],
        });
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Commander,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "ia-v6", false, true, 1, 2.0),
                player("Control", "ia-v6", true, false, 1, 0.0),
            ],
        });

        let response = query_dataset(
            &dataset,
            DeckAnalyticsQuery {
                analytics_context_id: None,
                game_mode: Some(GameMode::Commander),
                pilot_id: None,
                pilot_ids: None,
                creator_ids: None,
                player_count: None,
            },
        );
        let aggro = response
            .decks
            .iter()
            .find(|deck| deck.deck_name == "Aggro")
            .expect("aggro deck");
        assert_eq!(aggro.matches, 1);
        assert_eq!(aggro.wins, 0);
        assert_eq!(aggro.losses, 1);
        assert_eq!(aggro.all_rounds.metrics["damage"].per_round, 2.0);
        assert_eq!(response.selected_game_mode, Some(GameMode::Commander));
        assert_eq!(
            response
                .game_modes
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            vec!["free", "commander"]
        );
    }

    #[test]
    fn defaults_to_player_matches_and_keeps_training_models_in_separate_contexts() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aang", "human", true, false, 2, 2.0),
                player("Yenna", "ia-v10-in-training", false, true, 2, 1.0),
            ],
        });
        dataset.ingest(GameAnalyticsReport {
            context_id: "training:ia-v10-in-training".to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aang", "ia-v10-in-training", true, false, 2, 4.0),
                player("Yenna", "ia-v10-in-training", false, true, 2, 3.0),
            ],
        });

        let player_matches = query_dataset(&dataset, DeckAnalyticsQuery::default());
        assert_eq!(
            player_matches.selected_analytics_context_id.as_deref(),
            Some("player-match")
        );
        assert_eq!(player_matches.decks.len(), 2);
        assert!(
            player_matches
                .decks
                .iter()
                .any(|deck| deck.pilot_id == "human")
        );

        let training = query_dataset(
            &dataset,
            DeckAnalyticsQuery {
                analytics_context_id: Some("training:ia-v10-in-training".to_string()),
                ..DeckAnalyticsQuery::default()
            },
        );
        assert_eq!(training.decks.len(), 2);
        assert!(
            training
                .decks
                .iter()
                .all(|deck| deck.pilot_id == "ia-v10-in-training")
        );
    }

    #[test]
    fn creator_metadata_joins_by_deck_id_and_filters_existing_analytics() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::Completed,
            set_winner_player_id: None,
            players: vec![
                player("Aang", "human", true, false, 2, 3.0),
                player("4c Control", "human", false, true, 2, 1.0),
            ],
        });
        let creators = BTreeMap::from([
            ("aang".to_string(), "dd-the-dd".to_string()),
            ("4c-control".to_string(), "meta-standard".to_string()),
        ]);

        let response = query_dataset_with_creators(
            &dataset,
            DeckAnalyticsQuery {
                creator_ids: Some(vec!["meta-standard".to_string()]),
                ..DeckAnalyticsQuery::default()
            },
            &creators,
        );

        assert_eq!(response.decks.len(), 1);
        assert_eq!(response.decks[0].deck_name, "4c Control");
        assert_eq!(response.decks[0].creator, "meta-standard");
        assert_eq!(
            response
                .creators
                .iter()
                .map(|creator| creator.id.as_str())
                .collect::<Vec<_>>(),
            vec!["dd-the-dd", "meta-standard"]
        );
    }

    #[test]
    fn unfinished_games_are_reported_without_fabricating_results() {
        let mut dataset = PersistedDeckAnalytics::default();
        dataset.ingest(GameAnalyticsReport {
            context_id: PLAYER_MATCH_ANALYTICS_CONTEXT.to_string(),
            game_mode: GameMode::Commander,
            status: GameStatus::TurnLimitReached,
            set_winner_player_id: None,
            players: vec![
                player("Aggro", "ia-v6-in-training", false, false, 2, 2.0),
                player("Control", "ia-v6-in-training", false, false, 2, 1.0),
            ],
        });

        let response = query_dataset(&dataset, DeckAnalyticsQuery::default());
        let aggro = response
            .decks
            .iter()
            .find(|deck| deck.deck_name == "Aggro")
            .expect("aggro deck");
        assert_eq!(aggro.matches, 1);
        assert_eq!(aggro.wins, 0);
        assert_eq!(aggro.losses, 0);
        assert_eq!(aggro.unresolved_matches, 1);
    }

    #[test]
    fn instant_speed_options_are_deduplicated_within_the_round() {
        let setup = setup();
        let mut players = setup
            .players
            .iter()
            .map(|player| (player.id.clone(), PlayerGameBuilder::default()))
            .collect::<BTreeMap<_, _>>();
        let observations = vec![
            DecisionObservation {
                turn_number: 1,
                player_id: "player-1".to_string(),
                own_turn_option_count: 2,
                instant_speed_action_signatures: Vec::new(),
            },
            DecisionObservation {
                turn_number: 1,
                player_id: "player-2".to_string(),
                own_turn_option_count: 0,
                instant_speed_action_signatures: vec!["cast-a".to_string(), "cast-b".to_string()],
            },
            DecisionObservation {
                turn_number: 1,
                player_id: "player-2".to_string(),
                own_turn_option_count: 0,
                instant_speed_action_signatures: vec!["cast-a".to_string(), "cast-c".to_string()],
            },
        ];

        apply_decision_observations(&mut players, &setup, &observations);

        assert_eq!(players["player-1"].turns[&1].metrics["ownTurnOptions"], 2.0);
        assert_eq!(
            players["player-2"].turns[&1].metrics["instantSpeedOptions"],
            3.0
        );
    }

    #[test]
    fn snapshots_separate_plus_minus_and_other_counters() {
        let mut first = engine_player("player-1");
        first.counters.insert("poison".to_string(), 2);
        first
            .battlefield
            .push(permanent_with_counters(BTreeMap::from([
                ("+1/+1".to_string(), 3),
                ("-1/-1".to_string(), 1),
                ("charge".to_string(), 4),
            ])));
        let state = GameState {
            schema_version: "mtg-game/v1".to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::InProgress,
            turn_number: 1,
            active_player: 0,
            priority_player: Some(0),
            step: GameStep::PrecombatMain,
            players: vec![first, engine_player("player-2")],
            game_pieces: Vec::new(),
            commanders: Vec::new(),
            stack: Vec::new(),
            combat: CombatState::default(),
            permissions: Vec::new(),
            rule_modifiers: Vec::new(),
            events: Vec::new(),
            unsupported_rules: Vec::new(),
            outcome: None,
        };

        let snapshot = player_turn_snapshot(&state, "player-1").expect("player snapshot");

        assert_eq!(snapshot.metrics["plusOnePlusOneCounters"], 3.0);
        assert_eq!(snapshot.metrics["minusOneMinusOneCounters"], 1.0);
        assert_eq!(snapshot.metrics["otherCounters"], 6.0);
        assert_eq!(snapshot.metrics["creatures"], 1.0);
        assert_eq!(snapshot.metrics["artifacts"], 1.0);
    }

    #[test]
    fn snapshots_count_permanents_for_their_controller_and_include_tokens() {
        let mut first = engine_player("player-1");
        let mut controlled_token = permanent_with_counters(BTreeMap::new());
        controlled_token.controller = "player-2".to_string();
        controlled_token.definition.is_token = true;
        first.battlefield.push(controlled_token);
        let state = GameState {
            schema_version: "mtg-game/v1".to_string(),
            game_mode: GameMode::Free,
            status: GameStatus::InProgress,
            turn_number: 1,
            active_player: 0,
            priority_player: Some(0),
            step: GameStep::PrecombatMain,
            players: vec![first, engine_player("player-2")],
            game_pieces: Vec::new(),
            commanders: Vec::new(),
            stack: Vec::new(),
            combat: CombatState::default(),
            permissions: Vec::new(),
            rule_modifiers: Vec::new(),
            events: Vec::new(),
            unsupported_rules: Vec::new(),
            outcome: None,
        };

        let owner_snapshot = player_turn_snapshot(&state, "player-1").expect("owner snapshot");
        let controller_snapshot =
            player_turn_snapshot(&state, "player-2").expect("controller snapshot");

        assert_eq!(owner_snapshot.metrics["creatures"], 0.0);
        assert_eq!(owner_snapshot.metrics["artifacts"], 0.0);
        assert_eq!(owner_snapshot.metrics["tokens"], 0.0);
        assert_eq!(controller_snapshot.metrics["creatures"], 1.0);
        assert_eq!(controller_snapshot.metrics["artifacts"], 1.0);
        assert_eq!(controller_snapshot.metrics["tokens"], 1.0);
    }
}
