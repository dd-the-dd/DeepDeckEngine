use crate::agent_protocol::{
    WebSocketAgent, is_registered_agent_controller, validate_registered_agent_assignment,
};
use crate::analytics::{
    DeckAnalyticsService, ObservedDecisionProvider, PLAYER_MATCH_ANALYTICS_CONTEXT,
    build_game_analytics_report,
};
use crate::engine::{
    ActionKind, DecisionChoice, DecisionKind, DecisionProvider, EffectivePowerToughness,
    EngineDecisionRequest, EngineError, GameEngine, GameMode, GameSetup, GameState, LegalAction,
    RandomAi, decision_number_bounds, validate_decision_card_instance_ids,
};
use crate::game_rules::{
    LEGACY_MAXIMUM_SIDEBOARD_SIZE, LEGACY_MINIMUM_DECK_SIZE, TRAINING2_FREE_MULLIGANS,
    TRAINING2_MAX_MULLIGANS, TRAINING2_OPENING_HAND_SIZE, TRAINING2_STARTING_LIFE,
};
use crate::history_queue::{HistoryQueue, HistoryQueueStatus, HistoryStream};
use crate::http::project_ai_state;
use crate::pilot_catalog::{is_playable_model_controller, pilot_definition};
use crate::remote_ai::RemoteAi;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const SESSION_SCHEMA_VERSION: &str = "mtg-game-session/v1";
const SESSION_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

fn default_session_wait_timeout_ms() -> u64 {
    SESSION_WAIT_TIMEOUT.as_millis() as u64
}

fn default_max_turns() -> u32 {
    200
}

fn analytics_pilot_for_ai_controller(controller_id: &str) -> String {
    if controller_id == "ia-gt-0" {
        return "ia-v8-s0".to_string();
    }
    pilot_definition(controller_id)
        .map(|definition| definition.pilot_id.to_string())
        .unwrap_or_else(|| controller_id.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFormatDefaults {
    pub opening_hand_size: usize,
    pub starting_life: i32,
    pub max_turns: u32,
    pub mulligan_enabled: bool,
    pub free_mulligans: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_mulligans: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFormatDescription {
    pub id: &'static str,
    pub label: &'static str,
    pub defaults: GameFormatDefaults,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFormatCatalog {
    pub schema_version: &'static str,
    pub default_format_id: &'static str,
    pub formats: Vec<GameFormatDescription>,
}

pub fn game_format_catalog() -> GameFormatCatalog {
    GameFormatCatalog {
        schema_version: "mtg-game-format-catalog/v1",
        default_format_id: "free",
        formats: vec![
            GameFormatDescription {
                id: "free",
                label: "Partie personnalisée",
                defaults: GameFormatDefaults {
                    opening_hand_size: 7,
                    starting_life: 20,
                    max_turns: default_max_turns(),
                    mulligan_enabled: true,
                    free_mulligans: 0,
                    max_mulligans: None,
                },
            },
            GameFormatDescription {
                id: "legacy",
                label: "Legacy · meilleur de trois",
                defaults: GameFormatDefaults {
                    opening_hand_size: 7,
                    starting_life: 20,
                    max_turns: default_max_turns(),
                    mulligan_enabled: true,
                    free_mulligans: 0,
                    max_mulligans: None,
                },
            },
            GameFormatDescription {
                id: "commander",
                label: "Commander",
                defaults: GameFormatDefaults {
                    opening_hand_size: 7,
                    starting_life: 40,
                    max_turns: default_max_turns(),
                    mulligan_enabled: true,
                    free_mulligans: 1,
                    max_mulligans: None,
                },
            },
            GameFormatDescription {
                id: "duelCommander",
                label: "Duel Commander",
                defaults: GameFormatDefaults {
                    opening_hand_size: 7,
                    starting_life: 25,
                    max_turns: default_max_turns(),
                    mulligan_enabled: true,
                    free_mulligans: 1,
                    max_mulligans: None,
                },
            },
            GameFormatDescription {
                id: "training",
                label: "Training",
                defaults: GameFormatDefaults {
                    opening_hand_size: 5,
                    starting_life: 5,
                    max_turns: 40,
                    mulligan_enabled: true,
                    free_mulligans: 3,
                    max_mulligans: Some(3),
                },
            },
            GameFormatDescription {
                id: "training2",
                label: "Training 2",
                defaults: GameFormatDefaults {
                    opening_hand_size: TRAINING2_OPENING_HAND_SIZE,
                    starting_life: TRAINING2_STARTING_LIFE,
                    max_turns: 80,
                    mulligan_enabled: true,
                    free_mulligans: TRAINING2_FREE_MULLIGANS,
                    max_mulligans: Some(TRAINING2_MAX_MULLIGANS),
                },
            },
        ],
    }
}

fn session_run_error<T>(run: impl FnOnce() -> Result<T, EngineError>) -> Option<String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(result) => result.err().map(|error| error.to_string()),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            Some(format!("Rust game session panicked: {message}"))
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGameSessionRequest {
    pub setup: GameSetup,
    pub seed: u64,
    #[serde(default)]
    pub game_mode: GameMode,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default)]
    pub human_player_ids: Vec<String>,
    #[serde(default)]
    pub combat_declaration_revision_player_ids: Option<Vec<String>>,
    #[serde(default)]
    pub ai_controller_by_player_id: BTreeMap<String, String>,
    #[serde(default)]
    pub analytics_pilot_by_player_id: BTreeMap<String, String>,
    #[serde(default)]
    pub analytics_context_id: Option<String>,
    #[serde(default)]
    pub analytics_deck_session_by_player_id: BTreeMap<String, String>,
    #[serde(default)]
    pub punching_bag_player_ids: Vec<String>,
    #[serde(default)]
    pub opening_hand_selection_pool_size_by_player_id: BTreeMap<String, usize>,
    #[serde(default)]
    pub training_anchor_deadline_round_by_player_id: BTreeMap<String, u32>,
    #[serde(default)]
    pub hold_priority_player_ids: Vec<String>,
    #[serde(default)]
    pub mulligan_enabled: bool,
    #[serde(default)]
    pub free_mulligans: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_mulligans: Option<usize>,
    #[serde(default = "default_session_wait_timeout_ms")]
    pub wait_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_decision_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitGameSessionAction {
    pub revision: u64,
    pub decision_id: String,
    pub action_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_value: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_instance_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGameSessionSettings {
    #[serde(default)]
    pub hold_priority_player_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSessionView {
    pub schema_version: String,
    pub session_id: String,
    pub revision: u64,
    pub state: GameState,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calculated_stats: BTreeMap<String, EffectivePowerToughness>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<EngineDecisionRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_state: Option<GameMatchState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMatchState {
    pub game_number: u32,
    pub games_to_win: u32,
    pub phase: String,
    pub wins_by_player_id: BTreeMap<String, u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner_player_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSessionError {
    message: String,
}

impl GameSessionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GameSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GameSessionError {}

struct SessionChoice {
    action_id: String,
    card_instance_ids: Option<Vec<String>>,
    decision_id: String,
    number_value: Option<i32>,
}

struct SharedSession {
    view: Mutex<GameSessionView>,
    changed: Condvar,
    history: Option<HistoryStream>,
    pending_recovery_error: Mutex<Option<String>>,
}

impl SharedSession {
    fn publish_match_state(&self, match_state: GameMatchState) {
        if let Some(history) = &self.history {
            history.publish_authoritative(
                "match.state_changed",
                serde_json::to_value(&match_state).expect("match state serializes"),
            );
        }
        self.view.lock().expect("session view lock").match_state = Some(match_state);
    }

    fn publish_decision(&self, state: &GameState, decision: &EngineDecisionRequest) {
        let recovery_error = self
            .pending_recovery_error
            .lock()
            .expect("session recovery error lock")
            .take();
        let mut view = self.view.lock().expect("session view lock");
        view.revision += 1;
        view.state = state.clone();
        view.calculated_stats = GameEngine::effective_power_toughness(state);
        view.decision = Some(decision.clone());
        view.error = recovery_error;
        self.changed.notify_all();
    }

    fn publish_completion(&self, state: &GameState, error: Option<String>) {
        let recovery_error = self
            .pending_recovery_error
            .lock()
            .expect("session recovery error lock")
            .take();
        if let Some(history) = &self.history {
            history.publish_authoritative(
                "game.completed",
                serde_json::json!({
                    "state": state,
                    "error": error,
                }),
            );
        }
        let mut view = self.view.lock().expect("session view lock");
        view.revision += 1;
        view.state = state.clone();
        view.calculated_stats = GameEngine::effective_power_toughness(state);
        view.decision = None;
        view.error = error.or(recovery_error);
        self.changed.notify_all();
    }

    fn stage_recovery_error(&self, error: String) {
        *self
            .pending_recovery_error
            .lock()
            .expect("session recovery error lock") = Some(error);
    }

    fn snapshot(&self) -> GameSessionView {
        self.view.lock().expect("session view lock").clone()
    }

    fn wait_after(
        &self,
        revision: u64,
        timeout: Duration,
    ) -> Result<GameSessionView, GameSessionError> {
        let deadline = Instant::now() + timeout;
        let mut view = self.view.lock().expect("session view lock");
        while view.revision <= revision {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(GameSessionError::new(
                    "timed out waiting for the Rust game session",
                ));
            }
            let (next_view, timeout) = self
                .changed
                .wait_timeout(view, remaining)
                .expect("session condition lock");
            view = next_view;
            if timeout.timed_out() && view.revision <= revision {
                return Err(GameSessionError::new(
                    "timed out waiting for the Rust game session",
                ));
            }
        }
        Ok(view.clone())
    }
}

struct HistoryDecisionProvider<P> {
    inner: P,
    history: Option<HistoryStream>,
}

impl<P> HistoryDecisionProvider<P> {
    fn new(inner: P, history: Option<HistoryStream>) -> Self {
        Self { inner, history }
    }

    fn publish_decision(
        &self,
        state: &GameState,
        request: &EngineDecisionRequest,
        selected: serde_json::Value,
        response_time: Duration,
        error: Option<String>,
    ) {
        let Some(history) = &self.history else {
            return;
        };
        let observation = serde_json::to_value(project_ai_state(state.clone(), &request.player_id))
            .expect("player observation serializes");
        let mut payload = serde_json::Map::new();
        payload.insert(
            "request".to_string(),
            serde_json::to_value(request).expect("decision request serializes"),
        );
        payload.insert("selected".to_string(), selected);
        payload.insert(
            "responseTimeMs".to_string(),
            serde_json::json!(response_time.as_millis().min(u128::from(u64::MAX)) as u64),
        );
        if let Some(error) = error {
            payload.insert("error".to_string(), serde_json::Value::String(error));
        }
        history.publish_player_observation(
            "decision.resolved",
            &request.player_id,
            observation,
            payload,
        );
    }
}

impl<P: DecisionProvider> DecisionProvider for HistoryDecisionProvider<P> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let started = Instant::now();
        let result = self.inner.choose(state, request);
        let (selected, error) = match &result {
            Ok(index) => (
                request
                    .options
                    .get(*index)
                    .map(|action| serde_json::to_value(action).expect("legal action serializes"))
                    .unwrap_or_else(|| serde_json::json!({ "optionIndex": index })),
                None,
            ),
            Err(error) => (serde_json::Value::Null, Some(error.to_string())),
        };
        self.publish_decision(state, request, selected, started.elapsed(), error);
        result
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let started = Instant::now();
        let result = self.inner.choose_number(state, request);
        let (selected, error) = match &result {
            Ok(number) => (serde_json::json!({ "number": number }), None),
            Err(error) => (serde_json::Value::Null, Some(error.to_string())),
        };
        self.publish_decision(state, request, selected, started.elapsed(), error);
        result
    }

    fn choose_card_instance_ids(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        let started = Instant::now();
        let result = self.inner.choose_card_instance_ids(state, request);
        let (selected, error) = match &result {
            Ok(card_instance_ids) => (
                serde_json::json!({ "cardInstanceIds": card_instance_ids }),
                None,
            ),
            Err(error) => (serde_json::Value::Null, Some(error.to_string())),
        };
        self.publish_decision(state, request, selected, started.elapsed(), error);
        result
    }

    fn requests_explicit_priority_pass(&self, player_id: &str) -> bool {
        self.inner.requests_explicit_priority_pass(player_id)
    }

    fn allows_combat_declaration_revisions(&self, player_id: &str) -> bool {
        self.inner.allows_combat_declaration_revisions(player_id)
    }

    fn observe_turn_completed(&mut self, state: &GameState) {
        self.inner.observe_turn_completed(state);
        if let Some(history) = &self.history {
            history.publish_authoritative("turn.completed", serde_json::json!({ "state": state }));
        }
    }

    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}

struct GameSessionHandle {
    cancelled: Arc<AtomicBool>,
    choices: mpsc::Sender<SessionChoice>,
    hold_priority_player_ids: Arc<Mutex<BTreeSet<String>>>,
    human_player_ids: BTreeSet<String>,
    shared: Arc<SharedSession>,
    submit_guard: Mutex<()>,
    wait_timeout: Duration,
}

struct InteractiveDecisionProvider {
    agent_clients: BTreeMap<String, WebSocketAgent>,
    ai_clients: BTreeMap<String, RandomAi>,
    ai_seed: u64,
    choices: mpsc::Receiver<SessionChoice>,
    hold_priority_player_ids: Arc<Mutex<BTreeSet<String>>>,
    human_player_ids: BTreeSet<String>,
    combat_declaration_revision_player_ids: BTreeSet<String>,
    remote_ai_clients: BTreeMap<String, RemoteAi>,
    shared: Arc<SharedSession>,
    cancelled: Arc<AtomicBool>,
    human_decision_timeout: Option<Duration>,
}

impl InteractiveDecisionProvider {
    fn player_seed(&self, player_id: &str) -> u64 {
        player_id
            .as_bytes()
            .iter()
            .fold(self.ai_seed ^ 0xA076_1D64_78BD_642F, |seed, byte| {
                seed.rotate_left(7)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(u64::from(*byte))
            })
    }

    fn receive_human_choice(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<SessionChoice, EngineError> {
        self.shared.publish_decision(state, request);
        let choice = if let Some(timeout) = self.human_decision_timeout {
            self.choices
                .recv_timeout(timeout)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => EngineError::new(format!(
                        "human decision {} timed out after {} seconds",
                        request.id,
                        timeout.as_secs()
                    )),
                    mpsc::RecvTimeoutError::Disconnected => {
                        EngineError::new("interactive session action channel closed")
                    }
                })?
        } else {
            self.choices
                .recv()
                .map_err(|_| EngineError::new("interactive session action channel closed"))?
        };
        if choice.decision_id != request.id {
            return Err(EngineError::new(format!(
                "session answered decision {} while {} was pending",
                choice.decision_id, request.id
            )));
        }
        Ok(choice)
    }
}

impl DecisionProvider for InteractiveDecisionProvider {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(agent) = self.agent_clients.get_mut(&request.player_id) {
            let observation = project_ai_state(state.clone(), &request.player_id);
            match agent.choose(&observation, request) {
                Ok(choice) => return Ok(choice),
                Err(error) => eprintln!(
                    "external agent failed decision {}; using deterministic random fallback: {}",
                    request.id, error
                ),
            }
            let seed = self.player_seed(&request.player_id);
            return self
                .ai_clients
                .entry(request.player_id.clone())
                .or_insert_with(|| RandomAi::seeded(seed))
                .choose(state, request);
        }
        if let Some(remote_ai) = self.remote_ai_clients.get_mut(&request.player_id) {
            let observation = project_ai_state(state.clone(), &request.player_id);
            return remote_ai.choose(&observation, request);
        }
        if !self.human_player_ids.contains(&request.player_id) {
            let seed = self.player_seed(&request.player_id);
            return self
                .ai_clients
                .entry(request.player_id.clone())
                .or_insert_with(|| RandomAi::seeded(seed))
                .choose(state, request);
        }

        let choice = self.receive_human_choice(state, request)?;
        request
            .options
            .iter()
            .position(|action| action.id == choice.action_id)
            .ok_or_else(|| {
                EngineError::new(format!(
                    "session action {} is not legal for {}",
                    choice.action_id, request.id
                ))
            })
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        if let Some(agent) = self.agent_clients.get_mut(&request.player_id) {
            let observation = project_ai_state(state.clone(), &request.player_id);
            match agent.choose_number(&observation, request) {
                Ok(choice) => return Ok(choice),
                Err(error) => eprintln!(
                    "external agent failed numeric decision {}; using deterministic random fallback: {}",
                    request.id, error
                ),
            }
            let seed = self.player_seed(&request.player_id);
            return self
                .ai_clients
                .entry(request.player_id.clone())
                .or_insert_with(|| RandomAi::seeded(seed))
                .choose_number(state, request);
        }
        if let Some(remote_ai) = self.remote_ai_clients.get_mut(&request.player_id) {
            let observation = project_ai_state(state.clone(), &request.player_id);
            return remote_ai.choose_number(&observation, request);
        }
        if !self.human_player_ids.contains(&request.player_id) {
            let seed = self.player_seed(&request.player_id);
            return self
                .ai_clients
                .entry(request.player_id.clone())
                .or_insert_with(|| RandomAi::seeded(seed))
                .choose_number(state, request);
        }
        let choice = self.receive_human_choice(state, request)?;
        let selected = choice.number_value.ok_or_else(|| {
            EngineError::new(format!(
                "session did not provide a number for {}",
                request.id
            ))
        })?;
        let Some((minimum, maximum)) = decision_number_bounds(request) else {
            return Err(EngineError::new(format!(
                "decision {} is not a number selection",
                request.id
            )));
        };
        if !(minimum..=maximum).contains(&selected) {
            return Err(EngineError::new(format!(
                "session number {selected} is outside {minimum}..={maximum} for {}",
                request.id
            )));
        }
        Ok(selected)
    }

    fn choose_card_instance_ids(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        if let Some(agent) = self.agent_clients.get_mut(&request.player_id) {
            let observation = project_ai_state(state.clone(), &request.player_id);
            match agent.choose_card_instance_ids(&observation, request) {
                Ok(choice) => return Ok(choice),
                Err(error) => eprintln!(
                    "external agent failed card selection {}; using deterministic random fallback: {}",
                    request.id, error
                ),
            }
            if request.kind == DecisionKind::Sideboarding {
                return Ok(Vec::new());
            }
            let seed = self.player_seed(&request.player_id);
            return self
                .ai_clients
                .entry(request.player_id.clone())
                .or_insert_with(|| RandomAi::seeded(seed))
                .choose_card_instance_ids(state, request);
        }
        if let Some(remote_ai) = self.remote_ai_clients.get_mut(&request.player_id) {
            let observation = project_ai_state(state.clone(), &request.player_id);
            return remote_ai.choose_card_instance_ids(&observation, request);
        }
        if request.kind == DecisionKind::Sideboarding
            && !self.human_player_ids.contains(&request.player_id)
        {
            return Ok(Vec::new());
        }
        if !self.human_player_ids.contains(&request.player_id) {
            let seed = self.player_seed(&request.player_id);
            return self
                .ai_clients
                .entry(request.player_id.clone())
                .or_insert_with(|| RandomAi::seeded(seed))
                .choose_card_instance_ids(state, request);
        }
        let choice = self.receive_human_choice(state, request)?;
        let selected = if let Some(selected) = choice.card_instance_ids {
            selected
        } else {
            let action = request
                .options
                .iter()
                .find(|action| action.id == choice.action_id)
                .ok_or_else(|| {
                    EngineError::new(format!(
                        "session action {} is not legal for {}",
                        choice.action_id, request.id
                    ))
                })?;
            let decision_id = match request.choice.as_ref() {
                Some(crate::engine::DecisionChoice::CardSelection { decision_id, .. })
                | Some(crate::engine::DecisionChoice::CardOrder { decision_id, .. }) => decision_id,
                _ => {
                    return Err(EngineError::new(format!(
                        "decision {} is not a card selection",
                        request.id
                    )));
                }
            };
            action
                .decisions
                .get(decision_id)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        };
        validate_decision_card_instance_ids(request, &selected)?;
        Ok(selected)
    }

    fn requests_explicit_priority_pass(&self, player_id: &str) -> bool {
        self.human_player_ids.contains(player_id)
            && self
                .hold_priority_player_ids
                .lock()
                .expect("hold-priority settings lock")
                .contains(player_id)
    }

    fn allows_combat_declaration_revisions(&self, player_id: &str) -> bool {
        self.combat_declaration_revision_player_ids
            .contains(player_id)
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

fn sideboard_decision(
    player_id: &str,
    game_number: u32,
    stage: &str,
    candidates: Vec<String>,
    initial_main_deck_ids: &BTreeSet<String>,
    minimum: usize,
    maximum: usize,
    prompt: &str,
) -> EngineDecisionRequest {
    let decision_id = format!("sideboard:{game_number}:{player_id}:{stage}");
    let choice_id = format!("{decision_id}:cards");
    let mut decisions = BTreeMap::new();
    decisions.insert(choice_id.clone(), serde_json::Value::Array(Vec::new()));
    decisions.insert(
        "initialMainDeckIds".to_string(),
        serde_json::json!(initial_main_deck_ids),
    );
    EngineDecisionRequest {
        id: decision_id.clone(),
        kind: DecisionKind::Sideboarding,
        player_id: player_id.to_string(),
        source_card: None,
        source_card_instance_id: None,
        choice: Some(DecisionChoice::CardSelection {
            decision_id: choice_id,
            candidate_card_instance_ids: candidates,
            minimum,
            maximum,
            prompt: prompt.to_string(),
        }),
        options: vec![LegalAction {
            id: format!("{decision_id}:confirm"),
            kind: ActionKind::ChooseResolution,
            player_id: player_id.to_string(),
            label: "Confirmer le sideboard".to_string(),
            card_instance_id: None,
            payment_sources: Vec::new(),
            decisions,
            targets: BTreeMap::new(),
            target_order: Vec::new(),
            attacker_id: None,
            blocker_id: None,
        }],
    }
}

fn apply_sideboarding(
    setup: &mut GameSetup,
    state: &GameState,
    provider: &mut impl DecisionProvider,
    game_number: u32,
) -> Result<(), EngineError> {
    for player in &mut setup.players {
        let indexed = player
            .cards
            .iter()
            .enumerate()
            .filter(|(_, card)| !card.is_token && !card.is_game_piece && !card.is_commander)
            .map(|(index, card)| {
                (
                    index,
                    format!("{}:{}:{index}", player.id, card.id),
                    card.is_sideboard,
                )
            })
            .collect::<Vec<_>>();
        let sideboard = indexed
            .iter()
            .filter(|(_, _, sideboard)| *sideboard)
            .map(|(_, instance_id, _)| instance_id.clone())
            .collect::<Vec<_>>();
        if sideboard.is_empty() {
            continue;
        }
        let candidates = indexed
            .iter()
            .map(|(_, instance_id, _)| instance_id.clone())
            .collect::<Vec<_>>();
        let minimum_main_deck_size = LEGACY_MINIMUM_DECK_SIZE.max(
            candidates
                .len()
                .saturating_sub(LEGACY_MAXIMUM_SIDEBOARD_SIZE),
        );
        let current_main_deck = indexed
            .iter()
            .filter(|(_, _, is_sideboard)| !*is_sideboard)
            .map(|(_, instance_id, _)| instance_id.clone())
            .collect::<BTreeSet<_>>();
        let request = sideboard_decision(
            &player.id,
            game_number,
            "configure",
            candidates.clone(),
            &current_main_deck,
            minimum_main_deck_size,
            candidates.len(),
            "Compose le deck principal final. Il doit contenir au moins 60 cartes.",
        );
        // An AI can time out or return the request's empty default.  Sideboarding
        // must never abort a match in that case: keeping the already legal main
        // deck is the neutral (and deterministic) fallback.
        let final_main_deck = match provider.choose_card_instance_ids(state, &request) {
            Ok(selection) if validate_decision_card_instance_ids(&request, &selection).is_ok() => {
                selection.into_iter().collect::<BTreeSet<_>>()
            }
            Ok(_) | Err(_) => current_main_deck,
        };
        for (index, instance_id, _) in indexed {
            player.cards[index].is_sideboard = !final_main_deck.contains(&instance_id);
        }
    }
    Ok(())
}

pub struct GameSessionManager {
    analytics: Option<DeckAnalyticsService>,
    history: Option<HistoryQueue>,
    next_id: AtomicU64,
    sessions: Mutex<BTreeMap<String, Arc<GameSessionHandle>>>,
}

impl Default for GameSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GameSessionManager {
    pub fn new() -> Self {
        Self {
            analytics: None,
            history: None,
            next_id: AtomicU64::new(1),
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn history_status(&self) -> Result<Option<HistoryQueueStatus>, String> {
        self.history.as_ref().map(HistoryQueue::status).transpose()
    }

    pub fn with_analytics(analytics: DeckAnalyticsService) -> Self {
        Self {
            analytics: Some(analytics),
            history: HistoryQueue::from_env(),
            next_id: AtomicU64::new(1),
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn create(
        &self,
        request: CreateGameSessionRequest,
    ) -> Result<GameSessionView, GameSessionError> {
        if let Some(history) = &self.history {
            let status = history.status().map_err(GameSessionError::new)?;
            let stored_bytes = status
                .pending_bytes
                .saturating_add(status.quarantined_bytes);
            let high_watermark = status.max_bytes.saturating_mul(9) / 10;
            if stored_bytes >= high_watermark {
                return Err(GameSessionError::new(format!(
                    "official history spool is at its safety threshold ({stored_bytes}/{} bytes); new sessions are paused until delivery recovers",
                    status.max_bytes
                )));
            }
        }
        if request.max_turns == 0 {
            return Err(GameSessionError::new(
                "interactive session turn limit must be positive",
            ));
        }
        let player_ids = request
            .setup
            .players
            .iter()
            .map(|player| player.id.as_str())
            .collect::<BTreeSet<_>>();
        if let Some(unknown) = request
            .human_player_ids
            .iter()
            .find(|player_id| !player_ids.contains(player_id.as_str()))
        {
            return Err(GameSessionError::new(format!(
                "unknown human session player: {unknown}"
            )));
        }
        for (player_id, controller_id) in &request.ai_controller_by_player_id {
            if !player_ids.contains(player_id.as_str()) {
                return Err(GameSessionError::new(format!(
                    "unknown AI session player: {player_id}"
                )));
            }
            let ground_truth_version = controller_id
                .strip_prefix("ia-gt-")
                .and_then(|version| version.parse::<u64>().ok());
            if !is_playable_model_controller(controller_id)
                && ground_truth_version.is_none()
                && !is_registered_agent_controller(controller_id)
            {
                return Err(GameSessionError::new(format!(
                    "unsupported AI controller: {controller_id}"
                )));
            }
            if is_registered_agent_controller(controller_id) {
                let game_mode = serde_json::to_value(request.game_mode)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("{:?}", request.game_mode).to_ascii_lowercase());
                validate_registered_agent_assignment(
                    controller_id,
                    &game_mode,
                    request
                        .analytics_deck_session_by_player_id
                        .get(player_id)
                        .map(String::as_str),
                )
                .map_err(GameSessionError::new)?;
            }
            if request.human_player_ids.contains(player_id) {
                return Err(GameSessionError::new(format!(
                    "session player {player_id} has both human and {controller_id} controllers"
                )));
            }
        }
        for (player_id, pilot_id) in &request.analytics_pilot_by_player_id {
            if !player_ids.contains(player_id.as_str()) {
                return Err(GameSessionError::new(format!(
                    "unknown analytics pilot player: {player_id}"
                )));
            }
            if pilot_id.trim().is_empty() {
                return Err(GameSessionError::new(format!(
                    "analytics pilot is empty for {player_id}"
                )));
            }
        }
        for (player_id, session_id) in &request.analytics_deck_session_by_player_id {
            if !player_ids.contains(player_id.as_str()) {
                return Err(GameSessionError::new(format!(
                    "unknown analytics deck-session player: {player_id}"
                )));
            }
            if session_id.trim().is_empty() {
                return Err(GameSessionError::new(format!(
                    "analytics deck-session is empty for {player_id}"
                )));
            }
        }
        for player_id in request
            .opening_hand_selection_pool_size_by_player_id
            .keys()
            .chain(request.training_anchor_deadline_round_by_player_id.keys())
        {
            if !player_ids.contains(player_id.as_str()) {
                return Err(GameSessionError::new(format!(
                    "unknown training configuration player: {player_id}"
                )));
            }
        }
        if let Some(unknown) = request
            .hold_priority_player_ids
            .iter()
            .find(|player_id| !request.human_player_ids.contains(player_id))
        {
            return Err(GameSessionError::new(format!(
                "hold-priority player is not a human session player: {unknown}"
            )));
        }
        let combat_declaration_revision_player_ids = request
            .combat_declaration_revision_player_ids
            .clone()
            .unwrap_or_else(|| request.human_player_ids.clone());
        if let Some(unknown) = combat_declaration_revision_player_ids
            .iter()
            .find(|player_id| !request.human_player_ids.contains(player_id))
        {
            return Err(GameSessionError::new(format!(
                "combat declaration revision player is not human: {unknown}"
            )));
        }

        let analytics_setup = request.setup.clone();
        let mut match_setup = request.setup.clone();
        let analytics_context_id = request
            .analytics_context_id
            .as_deref()
            .map(str::trim)
            .filter(|context_id| !context_id.is_empty())
            .unwrap_or(PLAYER_MATCH_ANALYTICS_CONTEXT)
            .to_string();
        let analytics_pilot_by_player_id = request
            .setup
            .players
            .iter()
            .map(|player| {
                let pilot = request
                    .ai_controller_by_player_id
                    .get(&player.id)
                    .map(|controller_id| analytics_pilot_for_ai_controller(controller_id))
                    .or_else(|| {
                        request
                            .analytics_pilot_by_player_id
                            .get(&player.id)
                            .cloned()
                    })
                    .unwrap_or_else(|| {
                        if request.human_player_ids.contains(&player.id) {
                            "human".to_string()
                        } else {
                            "ai-random".to_string()
                        }
                    });
                (player.id.clone(), pilot)
            })
            .collect::<BTreeMap<_, _>>();
        let analytics = self.analytics.clone();
        let analytics_deck_session_by_player_id =
            request.analytics_deck_session_by_player_id.clone();
        let game_mode = request.game_mode;
        let wait_timeout = Duration::from_millis(request.wait_timeout_ms.clamp(1_000, 600_000));
        let human_decision_timeout = request
            .human_decision_timeout_ms
            .map(|milliseconds| Duration::from_millis(milliseconds.clamp(1_000, 600_000)));
        let mut engine = GameEngine::new_with_mode(match_setup.clone(), request.seed, game_mode)
            .map_err(|error| GameSessionError::new(error.to_string()))?;
        for player_id in &request.punching_bag_player_ids {
            engine
                .configure_punching_bag(player_id)
                .map_err(|error| GameSessionError::new(error.to_string()))?;
        }
        for (player_id, deadline_round) in &request.training_anchor_deadline_round_by_player_id {
            engine
                .configure_training_anchor(player_id, *deadline_round)
                .map_err(|error| GameSessionError::new(error.to_string()))?;
        }
        let session_id = format!(
            "game-session:{}",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let history = self
            .history
            .as_ref()
            .map(|queue| queue.stream(session_id.clone()));
        let initial_view = GameSessionView {
            calculated_stats: GameEngine::effective_power_toughness(engine.state()),
            decision: None,
            error: None,
            match_state: (game_mode == GameMode::Legacy).then(|| GameMatchState {
                game_number: 1,
                games_to_win: 2,
                phase: "game".to_string(),
                wins_by_player_id: match_setup
                    .players
                    .iter()
                    .map(|player| (player.id.clone(), 0))
                    .collect(),
                winner_player_id: None,
            }),
            revision: 0,
            schema_version: SESSION_SCHEMA_VERSION.to_string(),
            session_id: session_id.clone(),
            state: engine.state().clone(),
        };
        let shared = Arc::new(SharedSession {
            changed: Condvar::new(),
            history: history.clone(),
            pending_recovery_error: Mutex::new(None),
            view: Mutex::new(initial_view),
        });
        if let Some(history) = &history {
            history.publish_authoritative(
                "game.started",
                serde_json::json!({
                    "schemaVersion": crate::observation_delta::PIXI_REPLAY_SCHEMA_VERSION,
                    "sessionId": session_id,
                    "seed": request.seed,
                    "gameMode": game_mode,
                    "maxTurns": request.max_turns,
                    "humanPlayerIds": request.human_player_ids,
                    "combatDeclarationRevisionPlayerIds": combat_declaration_revision_player_ids,
                    "aiControllerByPlayerId": request.ai_controller_by_player_id,
                    "analyticsPilotByPlayerId": request.analytics_pilot_by_player_id,
                    "analyticsDeckSessionByPlayerId": request.analytics_deck_session_by_player_id,
                    "analyticsContextId": analytics_context_id,
                    "setup": match_setup,
                    "initialState": engine.state(),
                }),
            );
        }
        let (choice_sender, choice_receiver) = mpsc::channel();
        let human_player_ids = request
            .human_player_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let combat_declaration_revision_player_ids = combat_declaration_revision_player_ids
            .into_iter()
            .collect::<BTreeSet<_>>();
        let agent_clients = request
            .ai_controller_by_player_id
            .iter()
            .filter(|(_, controller_id)| controller_id.starts_with("agent:"))
            .map(|(player_id, controller_id)| {
                let known_deck = request
                    .setup
                    .players
                    .iter()
                    .find(|player| player.id == *player_id)
                    .map(|player| player.cards.clone())
                    .unwrap_or_default();
                WebSocketAgent::new(
                    controller_id.clone(),
                    format!("{session_id}:{player_id}"),
                    player_id.clone(),
                    known_deck,
                    history.clone(),
                )
                .map(|client| (player_id.clone(), client))
                .map_err(|error| GameSessionError::new(error.to_string()))
            })
            .collect::<Result<BTreeMap<_, _>, GameSessionError>>()?;
        let remote_ai_clients = request
            .ai_controller_by_player_id
            .iter()
            .filter(|(_, controller_id)| !controller_id.starts_with("agent:"))
            .map(|(player_id, controller_id)| {
                let client = match controller_id.as_str() {
                    "ia-in-training" => RemoteAi::training_from_env(),
                    "ia-v9-in-training" => RemoteAi::v9_training_from_env(),
                    "ia-v10-in-training" => RemoteAi::v10_training_from_env(),
                    "ia-v11-in-training" => RemoteAi::v11_training_from_env(),
                    "ia-v12-in-training" => RemoteAi::v12_training_from_env(),
                    _ => RemoteAi::ground_truth_from_env(),
                }
                .map(|client| client.with_controller_id(controller_id.clone()))
                .map(|client| client.with_context_id(format!("{session_id}:{player_id}")))
                .map_err(|error| GameSessionError::new(error.to_string()))?;
                Ok((player_id.clone(), client))
            })
            .collect::<Result<BTreeMap<_, _>, GameSessionError>>()?;
        let hold_priority_player_ids = Arc::new(Mutex::new(
            request
                .hold_priority_player_ids
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
        ));
        let cancelled = Arc::new(AtomicBool::new(false));
        let handle = Arc::new(GameSessionHandle {
            cancelled: Arc::clone(&cancelled),
            choices: choice_sender,
            hold_priority_player_ids: Arc::clone(&hold_priority_player_ids),
            human_player_ids: human_player_ids.clone(),
            shared: Arc::clone(&shared),
            submit_guard: Mutex::new(()),
            wait_timeout,
        });
        self.sessions
            .lock()
            .expect("session registry lock")
            .insert(session_id.clone(), handle);
        let waits_for_human_decision = !human_player_ids.is_empty();
        let initial_response = shared.snapshot();

        let max_turns = request.max_turns;
        let mulligan_enabled = matches!(
            game_mode,
            GameMode::Legacy
                | GameMode::Commander
                | GameMode::DuelCommander
                | GameMode::Training
                | GameMode::Training2
        ) || request.mulligan_enabled;
        let free_mulligans = match game_mode {
            GameMode::Commander | GameMode::DuelCommander => 1,
            GameMode::Training2 => TRAINING2_FREE_MULLIGANS,
            GameMode::Free | GameMode::Legacy | GameMode::Training => request.free_mulligans,
        };
        let max_mulligans = match game_mode {
            GameMode::Training => request.max_mulligans.or(Some(3)),
            GameMode::Training2 => Some(TRAINING2_MAX_MULLIGANS),
            GameMode::Free | GameMode::Legacy | GameMode::Commander | GameMode::DuelCommander => {
                request.max_mulligans
            }
        };
        let ai_seed = request.seed ^ 0xD1B5_4A32_D192_ED03;
        let opening_hand_selections = request.opening_hand_selection_pool_size_by_player_id;
        let wait_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("mtg-game-session".to_string())
            .spawn(move || {
                let provider = InteractiveDecisionProvider {
                    agent_clients,
                    ai_clients: BTreeMap::new(),
                    ai_seed,
                    choices: choice_receiver,
                    hold_priority_player_ids,
                    human_player_ids,
                    combat_declaration_revision_player_ids,
                    remote_ai_clients,
                    shared: Arc::clone(&shared),
                    cancelled,
                    human_decision_timeout,
                };
                let provider = HistoryDecisionProvider::new(provider, history);
                let mut provider = ObservedDecisionProvider::new(provider);
                const MAX_SESSION_RECOVERIES_PER_CHECKPOINT: usize = 3;
                let games_to_win = if game_mode == GameMode::Legacy { 2 } else { 1 };
                let mut game_number = 1_u32;
                let mut wins_by_player_id = match_setup
                    .players
                    .iter()
                    .map(|player| (player.id.clone(), 0_u32))
                    .collect::<BTreeMap<_, _>>();
                loop {
                    if game_mode == GameMode::Legacy {
                        shared.publish_match_state(GameMatchState {
                            game_number,
                            games_to_win,
                            phase: "game".to_string(),
                            wins_by_player_id: wins_by_player_id.clone(),
                            winner_player_id: None,
                        });
                    }
                    let mut error = None;
                    let mut setup_failures = 0;
                    loop {
                        let checkpoint = engine.clone();
                        let analytics_checkpoint = provider.analytics_checkpoint();
                        let attempt_error = session_run_error(|| {
                            for (player_id, pool_size) in &opening_hand_selections {
                                engine.run_opening_hand_selection(
                                    &mut provider,
                                    player_id,
                                    *pool_size,
                                )?;
                            }
                            if mulligan_enabled {
                                engine.run_mulligans(
                                    &mut provider,
                                    free_mulligans,
                                    max_mulligans,
                                )?;
                            } else {
                                engine.record_opening_hands();
                            }
                            Ok(())
                        });
                        let Some(attempt_error) = attempt_error else {
                            break;
                        };
                        engine = checkpoint;
                        provider.restore_analytics_checkpoint(analytics_checkpoint);
                        if provider.is_cancelled() {
                            error = Some(attempt_error);
                            break;
                        }
                        setup_failures += 1;
                        if setup_failures >= MAX_SESSION_RECOVERIES_PER_CHECKPOINT {
                            error = Some(format!(
                                "{attempt_error}; the opening sequence failed after {setup_failures} recovery attempts"
                            ));
                            break;
                        }
                        shared.stage_recovery_error(format!(
                            "{attempt_error}; the game was restored before opening-hand decisions"
                        ));
                    }

                    while error.is_none()
                        && engine.state().outcome.is_none()
                        && engine.state().turn_number <= max_turns
                    {
                        let checkpoint = engine.clone();
                        let checkpoint_turn = checkpoint.state().turn_number;
                        let analytics_checkpoint = provider.analytics_checkpoint();
                        let mut turn_failures = 0;
                        loop {
                            let attempt_error = session_run_error(|| {
                                engine
                                    .run_next_turn(&mut provider, max_turns)
                                    .map(|_| ())
                            });
                            let Some(attempt_error) = attempt_error else {
                                break;
                            };
                            engine = checkpoint.clone();
                            provider.restore_analytics_checkpoint(analytics_checkpoint);
                            if provider.is_cancelled() {
                                error = Some(attempt_error);
                                break;
                            }
                            turn_failures += 1;
                            if turn_failures >= MAX_SESSION_RECOVERIES_PER_CHECKPOINT {
                                error = Some(format!(
                                    "{attempt_error}; turn {checkpoint_turn} failed after {turn_failures} recovery attempts"
                                ));
                                break;
                            }
                            shared.stage_recovery_error(format!(
                                "{attempt_error}; turn {checkpoint_turn} was restored to its checkpoint"
                            ));
                        }
                    }

                    let completed_without_error = error.is_none();
                    let (observations, snapshots) = provider.take_analytics();
                    let mut analytics_report = if completed_without_error && analytics.is_some() {
                        let mut report = build_game_analytics_report(
                            &analytics_setup,
                            &analytics_pilot_by_player_id,
                            &analytics_deck_session_by_player_id,
                            &observations,
                            &snapshots,
                            engine.state(),
                        );
                        report.context_id = analytics_context_id.clone();
                        Some(report)
                    } else {
                        None
                    };
                    if let Some(error) = error {
                        shared.publish_completion(engine.state(), Some(error));
                        break;
                    }

                    let winner = engine
                        .state()
                        .outcome
                        .as_ref()
                        .and_then(|outcome| outcome.winner.clone());
                    let Some(winner) = winner else {
                        if let (Some(analytics), Some(report)) = (&analytics, analytics_report.take()) {
                            analytics.submit(report);
                        }
                        shared.publish_completion(engine.state(), None);
                        break;
                    };
                    *wins_by_player_id.entry(winner.clone()).or_default() += 1;
                    let set_complete = wins_by_player_id[&winner] >= games_to_win;
                    if let Some(report) = analytics_report.as_mut() {
                        report.set_winner_player_id = set_complete.then_some(winner.clone());
                    }
                    if let (Some(analytics), Some(report)) = (&analytics, analytics_report.take()) {
                        analytics.submit(report);
                    }
                    if set_complete {
                        if game_mode == GameMode::Legacy {
                            shared.publish_match_state(GameMatchState {
                                game_number,
                                games_to_win,
                                phase: "complete".to_string(),
                                wins_by_player_id: wins_by_player_id.clone(),
                                winner_player_id: Some(winner),
                            });
                        }
                        shared.publish_completion(engine.state(), None);
                        break;
                    }

                    shared.publish_match_state(GameMatchState {
                        game_number,
                        games_to_win,
                        phase: "sideboarding".to_string(),
                        wins_by_player_id: wins_by_player_id.clone(),
                        winner_player_id: None,
                    });
                    if let Err(error) = apply_sideboarding(
                        &mut match_setup,
                        engine.state(),
                        &mut provider,
                        game_number,
                    ) {
                        shared.publish_completion(engine.state(), Some(error.to_string()));
                        break;
                    }
                    if let Some(loser_index) = match_setup
                        .players
                        .iter()
                        .position(|player| player.id != winner)
                    {
                        match_setup.starting_player = loser_index;
                    }
                    game_number += 1;
                    match GameEngine::new_with_mode(
                        match_setup.clone(),
                        request.seed.wrapping_add(u64::from(game_number - 1)),
                        game_mode,
                    ) {
                        Ok(next_engine) => engine = next_engine,
                        Err(error) => {
                            shared.publish_completion(engine.state(), Some(error.to_string()));
                            break;
                        }
                    }
                }
            })
            .map_err(|error| {
                GameSessionError::new(format!("failed to start game session: {error}"))
            })?;

        // Automated sessions have no interactive decision to publish. Returning the
        // initial view lets an orchestrator obtain the session id immediately and
        // poll it while the game runs in the background.
        if !waits_for_human_decision {
            return Ok(initial_response);
        }

        let view = match wait_shared.wait_after(0, wait_timeout) {
            Ok(view) => view,
            Err(error) => {
                if let Some(handle) = self
                    .sessions
                    .lock()
                    .expect("session registry lock")
                    .remove(&session_id)
                {
                    handle.cancelled.store(true, Ordering::Relaxed);
                }
                return Err(error);
            }
        };
        if let Some(error) = &view.error {
            return Err(GameSessionError::new(error.clone()));
        }
        Ok(view)
    }

    pub fn view(&self, session_id: &str) -> Result<GameSessionView, GameSessionError> {
        let handle = self
            .sessions
            .lock()
            .expect("session registry lock")
            .get(session_id)
            .cloned()
            .ok_or_else(|| GameSessionError::new(format!("unknown game session: {session_id}")))?;
        Ok(handle.shared.snapshot())
    }

    pub fn remove(&self, session_id: &str) -> Result<(), GameSessionError> {
        self.sessions
            .lock()
            .expect("session registry lock")
            .remove(session_id)
            .map(|handle| handle.cancelled.store(true, Ordering::Relaxed))
            .ok_or_else(|| GameSessionError::new(format!("unknown game session: {session_id}")))
    }

    pub fn update_settings(
        &self,
        session_id: &str,
        settings: UpdateGameSessionSettings,
    ) -> Result<GameSessionView, GameSessionError> {
        let handle = self
            .sessions
            .lock()
            .expect("session registry lock")
            .get(session_id)
            .cloned()
            .ok_or_else(|| GameSessionError::new(format!("unknown game session: {session_id}")))?;
        if let Some(unknown) = settings
            .hold_priority_player_ids
            .iter()
            .find(|player_id| !handle.human_player_ids.contains(player_id.as_str()))
        {
            return Err(GameSessionError::new(format!(
                "hold-priority player is not a human session player: {unknown}"
            )));
        }
        *handle
            .hold_priority_player_ids
            .lock()
            .expect("hold-priority settings lock") =
            settings.hold_priority_player_ids.into_iter().collect();
        Ok(handle.shared.snapshot())
    }

    pub fn submit(
        &self,
        session_id: &str,
        submission: SubmitGameSessionAction,
    ) -> Result<GameSessionView, GameSessionError> {
        let handle = self
            .sessions
            .lock()
            .expect("session registry lock")
            .get(session_id)
            .cloned()
            .ok_or_else(|| GameSessionError::new(format!("unknown game session: {session_id}")))?;
        let _submit = handle.submit_guard.lock().expect("session submit lock");
        let current = handle.shared.snapshot();
        if current.revision != submission.revision {
            return Err(GameSessionError::new(format!(
                "stale game session revision: expected {}, received {}",
                current.revision, submission.revision
            )));
        }
        let decision = current
            .decision
            .as_ref()
            .ok_or_else(|| GameSessionError::new("game session is not awaiting a decision"))?;
        if decision.id != submission.decision_id {
            return Err(GameSessionError::new(format!(
                "game session is awaiting {}, not {}",
                decision.id, submission.decision_id
            )));
        }
        if !decision
            .options
            .iter()
            .any(|action| action.id == submission.action_id)
        {
            return Err(GameSessionError::new(format!(
                "action {} is not offered by {}",
                submission.action_id, decision.id
            )));
        }
        match decision_number_bounds(decision) {
            Some((minimum, maximum)) => {
                let number_value = submission.number_value.ok_or_else(|| {
                    GameSessionError::new(format!(
                        "number selection {} requires numberValue",
                        decision.id
                    ))
                })?;
                if !(minimum..=maximum).contains(&number_value) {
                    return Err(GameSessionError::new(format!(
                        "numberValue {number_value} is outside {minimum}..={maximum} for {}",
                        decision.id
                    )));
                }
            }
            None if submission.number_value.is_some() => {
                return Err(GameSessionError::new(format!(
                    "decision {} does not accept numberValue",
                    decision.id
                )));
            }
            None => {}
        }
        if let Some(card_instance_ids) = submission.card_instance_ids.as_ref() {
            if submission.number_value.is_some() {
                return Err(GameSessionError::new(
                    "a session choice cannot contain both numberValue and cardInstanceIds",
                ));
            }
            validate_decision_card_instance_ids(decision, card_instance_ids)
                .map_err(|error| GameSessionError::new(error.to_string()))?;
        }
        handle
            .choices
            .send(SessionChoice {
                action_id: submission.action_id,
                card_instance_ids: submission.card_instance_ids,
                decision_id: submission.decision_id,
                number_value: submission.number_value,
            })
            .map_err(|_| GameSessionError::new("game session is no longer running"))?;
        match handle
            .shared
            .wait_after(current.revision, handle.wait_timeout)
        {
            Ok(view) => Ok(view),
            Err(error) => {
                if let Some(handle) = self
                    .sessions
                    .lock()
                    .expect("session registry lock")
                    .remove(session_id)
                {
                    handle.cancelled.store(true, Ordering::Relaxed);
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CreateGameSessionRequest, GameSessionManager, analytics_pilot_for_ai_controller,
        apply_sideboarding, session_run_error,
    };
    use crate::engine::{
        CardDefinition, DecisionChoice, DecisionProvider, EngineDecisionRequest, EngineError,
        GameEngine, GameMode, GameSetup, GameState, PlayerDeck,
    };

    struct SideboardProvider {
        selections: Vec<Vec<String>>,
    }

    impl DecisionProvider for SideboardProvider {
        fn choose(
            &mut self,
            _state: &GameState,
            _request: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Ok(0)
        }

        fn choose_card_instance_ids(
            &mut self,
            _state: &GameState,
            request: &EngineDecisionRequest,
        ) -> Result<Vec<String>, EngineError> {
            let selection = self.selections.remove(0);
            let DecisionChoice::CardSelection {
                candidate_card_instance_ids,
                minimum,
                maximum,
                ..
            } = request.choice.as_ref().expect("sideboard choice")
            else {
                panic!("expected a card selection")
            };
            assert!((*minimum..=*maximum).contains(&selection.len()));
            assert!(
                selection
                    .iter()
                    .all(|id| candidate_card_instance_ids.contains(id))
            );
            Ok(selection)
        }
    }

    fn sideboard_card(id: &str, is_sideboard: bool) -> CardDefinition {
        CardDefinition {
            id: id.to_string(),
            name: id.to_string(),
            type_line: "Artifact".to_string(),
            is_commander: false,
            is_token: false,
            is_game_piece: false,
            is_sideboard,
            mana_cost: "{1}".to_string(),
            power: None,
            toughness: None,
            rules: Vec::new(),
        }
    }

    #[test]
    fn session_worker_converts_a_panic_to_an_explicit_error() {
        let error =
            session_run_error(|| -> Result<(), EngineError> { panic!("broken game instruction") })
                .expect("the panic becomes a session error");

        assert_eq!(error, "Rust game session panicked: broken game instruction");
    }

    #[test]
    fn automated_session_returns_an_initial_view_for_polling() {
        let cards = |owner: &str| {
            (0..14)
                .map(|index| CardDefinition {
                    id: format!("{owner}-land-{index}"),
                    name: "Wastes".to_string(),
                    type_line: "Basic Land".to_string(),
                    is_commander: false,
                    is_token: false,
                    is_game_piece: false,
                    is_sideboard: false,
                    mana_cost: String::new(),
                    power: None,
                    toughness: None,
                    rules: Vec::new(),
                })
                .collect()
        };
        let manager = GameSessionManager::new();
        let view = manager
            .create(CreateGameSessionRequest {
                setup: GameSetup {
                    players: vec![
                        PlayerDeck {
                            id: "a".to_string(),
                            name: "A".to_string(),
                            starting_life: 20,
                            cards: cards("a"),
                        },
                        PlayerDeck {
                            id: "b".to_string(),
                            name: "B".to_string(),
                            starting_life: 20,
                            cards: cards("b"),
                        },
                    ],
                    opening_hand_size: 7,
                    starting_player: 0,
                },
                seed: 1,
                game_mode: GameMode::Free,
                max_turns: 1,
                human_player_ids: Vec::new(),
                combat_declaration_revision_player_ids: None,
                ai_controller_by_player_id: Default::default(),
                analytics_pilot_by_player_id: Default::default(),
                analytics_context_id: None,
                analytics_deck_session_by_player_id: Default::default(),
                punching_bag_player_ids: Vec::new(),
                opening_hand_selection_pool_size_by_player_id: Default::default(),
                training_anchor_deadline_round_by_player_id: Default::default(),
                hold_priority_player_ids: Vec::new(),
                mulligan_enabled: false,
                free_mulligans: 0,
                max_mulligans: None,
                wait_timeout_ms: 10_000,
                human_decision_timeout_ms: None,
            })
            .expect("automated session starts in the background");

        assert_eq!(view.revision, 0);
        assert!(view.decision.is_none());
        assert!(view.session_id.starts_with("game-session:"));
    }

    #[test]
    fn ai_controller_analytics_distinguishes_v8_step_zero_from_historical_gt_zero() {
        assert_eq!(analytics_pilot_for_ai_controller("ia-gt-0"), "ia-v8-s0");
        assert_eq!(
            analytics_pilot_for_ai_controller("ia-in-training"),
            "ia-v8-in-training"
        );
        assert_eq!(analytics_pilot_for_ai_controller("ia-gt-1"), "ia-gt-1");
        assert_eq!(
            analytics_pilot_for_ai_controller("ia-v9-in-training"),
            "ia-v9-in-training"
        );
    }

    #[test]
    fn legacy_sideboarding_accepts_a_final_main_deck_with_at_least_sixty_cards() {
        let player = PlayerDeck {
            id: "player-1".to_string(),
            name: "Legacy player".to_string(),
            starting_life: 20,
            cards: std::iter::once(sideboard_card("main", false))
                .chain(std::iter::once(sideboard_card("side", true)))
                .chain(
                    (0..59).map(|index| sideboard_card(&format!("player-filler-{index}"), false)),
                )
                .collect(),
        };
        let opponent = PlayerDeck {
            id: "player-2".to_string(),
            name: "Opponent".to_string(),
            starting_life: 20,
            cards: (0..60)
                .map(|index| sideboard_card(&format!("opponent-main-{index}"), false))
                .collect(),
        };
        let mut setup = GameSetup {
            players: vec![player, opponent],
            opening_hand_size: 7,
            starting_player: 0,
        };
        let state = GameEngine::new(setup.clone(), 1).unwrap().state().clone();
        let final_main_deck = std::iter::once("player-1:side:1".to_string())
            .chain((0..59).map(|index| format!("player-1:player-filler-{index}:{}", index + 2)))
            .collect();
        let mut provider = SideboardProvider {
            selections: vec![final_main_deck],
        };

        apply_sideboarding(&mut setup, &state, &mut provider, 1).unwrap();

        assert!(setup.players[0].cards[0].is_sideboard);
        assert!(!setup.players[0].cards[1].is_sideboard);
    }
}
