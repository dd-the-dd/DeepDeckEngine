use crate::agent_protocol::{handle_agent_websocket, registered_agent_manifests};
use crate::analytics::{DeckAnalyticsQuery, DeckAnalyticsService};
use crate::card_catalog::{CardLookupRequest, card_sets, lookup_cards};
use crate::engine::{
    GameMode, GameSetup, RandomSimulationRequest, rule_is_executable, simulate_random_games,
};
use crate::game::{DecisionRequest, build_player_decision_options};
use crate::game_rules::GameRules;
use crate::local_app::{
    compiled_local_deck, local_deck_for_validation, local_deck_session_exists,
    local_meta_deck_session_ids, route_local_app,
};
use crate::model::playable_rules_for_face;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};
use crate::remote_ai::{control_training, controller_catalog, training_dashboard};
use crate::session::{
    CreateGameSessionRequest, GameSessionError, GameSessionManager, GameSessionView,
    SubmitGameSessionAction, UpdateGameSessionSettings, game_format_catalog,
};
use rand::{
    Rng, SeedableRng,
    distributions::{Alphanumeric, Distribution, WeightedIndex},
    rngs::StdRng,
    seq::SliceRandom,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const HTTP_CONNECTION_STACK_SIZE: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct JsonResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Deserialize, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalDeckPlayerRequest {
    id: String,
    deck_session_id: String,
    #[serde(default)]
    name: Option<String>,
    starting_life: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateLocalDeckGameRequest {
    players: Vec<LocalDeckPlayerRequest>,
    seed: u64,
    #[serde(default)]
    game_mode: GameMode,
    #[serde(default = "default_http_max_turns")]
    max_turns: u32,
    #[serde(default)]
    opening_hand_size: usize,
    #[serde(default)]
    starting_player: usize,
    #[serde(default)]
    human_player_ids: Vec<String>,
    #[serde(default)]
    ai_controller_by_player_id: BTreeMap<String, String>,
    #[serde(default)]
    analytics_pilot_by_player_id: BTreeMap<String, String>,
    #[serde(default)]
    analytics_context_id: Option<String>,
    #[serde(default)]
    network_multiplayer: bool,
    #[serde(default)]
    host_player_id: Option<String>,
    #[serde(default)]
    host_username: Option<String>,
    #[serde(default)]
    hold_priority_player_ids: Vec<String>,
    #[serde(default)]
    mulligan_enabled: bool,
    #[serde(default)]
    free_mulligans: usize,
    #[serde(default)]
    max_mulligans: Option<usize>,
    #[serde(default = "default_http_wait_timeout_ms")]
    wait_timeout_ms: u64,
    #[serde(default)]
    human_decision_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegalDeckCatalogRequest {
    #[serde(default)]
    game_mode: GameMode,
    #[serde(default)]
    opening_hand_size: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegalDeckOption {
    deck_session_id: String,
    deck_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegalDeckCatalogResponse {
    schema_version: &'static str,
    game_mode: GameMode,
    decks: Vec<LegalDeckOption>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayMatchmakingRequest {
    #[serde(default)]
    game_mode: GameMode,
    human_deck_session_id: String,
    #[serde(default = "default_human_pilot_id")]
    human_pilot_id: String,
    #[serde(default)]
    allowed_ai_controller_ids: Vec<String>,
    #[serde(default)]
    opening_hand_size: usize,
    #[serde(default)]
    seed: u64,
}

fn default_human_pilot_id() -> String {
    "human".to_string()
}

#[derive(Clone, Debug)]
struct MatchmakingCandidate {
    controller_id: String,
    controller_label: String,
    deck: LegalDeckOption,
    matches: u64,
    pilot_id: String,
    rating: f64,
    rating_source: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MatchmakingRatingView {
    matches: u64,
    pilot_id: String,
    plackett_luce_ordinal: f64,
    rank: usize,
    pool_size: usize,
    source: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayMatchmakingResponse {
    schema_version: &'static str,
    game_mode: GameMode,
    human: MatchmakingRatingView,
    opponent: MatchmakingOpponentView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MatchmakingOpponentView {
    controller_id: String,
    controller_label: String,
    deck_session_id: String,
    deck_name: String,
    matches: u64,
    pilot_id: String,
    plackett_luce_ordinal: f64,
    rank: usize,
    pool_size: usize,
    rating_distance: f64,
    rating_source: &'static str,
}

fn default_http_max_turns() -> u32 {
    200
}

fn default_http_wait_timeout_ms() -> u64 {
    30_000
}

fn network_game_registry() -> &'static Mutex<NetworkGameRegistry> {
    static REGISTRY: OnceLock<Mutex<NetworkGameRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(NetworkGameRegistry::default()))
}

fn ensure_network_lobby_cleanup() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        thread::spawn(|| {
            loop {
                thread::sleep(Duration::from_secs(10));
                let Ok(mut registry) = network_game_registry().lock() else {
                    continue;
                };
                let now = Instant::now();
                registry.lobbies.retain(|_, lobby| {
                    lobby.claimed_player_ids.iter().any(|player_id| {
                        lobby
                            .last_seen_by_player_id
                            .get(player_id)
                            .is_some_and(|seen| {
                                now.saturating_duration_since(*seen) < Duration::from_secs(45)
                            })
                    })
                });
            }
        });
    });
}

fn random_access_value(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

fn random_game_number() -> String {
    const GAME_CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    let mut rng = rand::thread_rng();
    (0..4)
        .map(|_| {
            let index = rng.gen_range(0..GAME_CODE_ALPHABET.len());
            char::from(GAME_CODE_ALPHABET[index])
        })
        .collect()
}

fn hidden_card(instance: &mut crate::engine::CardInstance, zone: &str, index: usize) {
    instance.instance_id = format!("hidden:{zone}:{}:{index}", instance.owner);
    instance.definition = crate::engine::CardDefinition {
        id: "hidden-card".to_string(),
        name: "Hidden card".to_string(),
        type_line: String::new(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    };
    instance.printed_definition = None;
    instance.tapped = false;
    instance.summoning_sick = false;
    instance.damage_marked = 0;
    instance.power_modifier = 0;
    instance.toughness_modifier = 0;
    instance.counters.clear();
    instance.flags.clear();
    instance.battle_protector = None;
    instance.attached_to = None;
}

fn project_session_view(mut view: GameSessionView, viewer_player_id: &str) -> GameSessionView {
    let mut hidden_instance_ids = BTreeSet::new();
    let viewer_is_sideboarding = view.decision.as_ref().is_some_and(|decision| {
        decision.kind == crate::engine::DecisionKind::Sideboarding
            && decision.player_id == viewer_player_id
    });
    let continuously_revealed_hands = view
        .state
        .players
        .iter()
        .filter(|source_player| !source_player.has_lost)
        .flat_map(|source_player| {
            source_player
                .battlefield
                .iter()
                .filter(|source| source.controller == source_player.id)
                .flat_map(|source| source.definition.rules.iter())
                .filter(|rule| rule["kind"].as_str() == Some("staticAbility"))
                .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
                .filter(|modifier| {
                    modifier["kind"].as_str() == Some("revealHands")
                        && modifier["players"]["kind"].as_str() == Some("opponentsOf")
                })
                .flat_map(|_| {
                    view.state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost && player.id != source_player.id)
                        .map(|player| player.id.clone())
                })
        })
        .collect::<BTreeSet<_>>();
    for player in &mut view.state.players {
        for (index, card) in player.library.iter_mut().enumerate() {
            if viewer_is_sideboarding && player.id == viewer_player_id {
                continue;
            }
            hidden_instance_ids.insert(card.instance_id.clone());
            hidden_card(card, "library", index);
        }
        let hand_is_revealed = continuously_revealed_hands.contains(&player.id)
            || view.state.rule_modifiers.iter().any(|modifier| {
                modifier["kind"].as_str() == Some("revealedHand")
                    && modifier["playerId"].as_str() == Some(player.id.as_str())
                    && modifier["expiresAfterTurn"]
                        .as_u64()
                        .is_none_or(|turn| turn >= u64::from(view.state.turn_number))
            });
        if player.id != viewer_player_id && !hand_is_revealed {
            for (index, card) in player.hand.iter_mut().enumerate() {
                hidden_instance_ids.insert(card.instance_id.clone());
                hidden_card(card, "hand", index);
            }
            for (index, card) in player.sideboard.iter_mut().enumerate() {
                hidden_instance_ids.insert(card.instance_id.clone());
                hidden_card(card, "sideboard", index);
            }
        }
    }
    for event in &mut view.state.events {
        if event
            .card_instance_id
            .as_ref()
            .is_some_and(|instance_id| hidden_instance_ids.contains(instance_id))
        {
            event.card_instance_id = None;
        }
    }
    if view
        .decision
        .as_ref()
        .is_some_and(|decision| decision.player_id != viewer_player_id)
    {
        view.decision = None;
    }
    view
}

/// Build the private information projection sent to a single AI player.
///
/// The engine keeps the authoritative full state internally, but an AI must
/// receive the same visibility boundary as a human player. In particular,
/// opponent libraries and hands are replaced by opaque cards while the
/// acting player's own deck remains available for AlphaStar's pre-game
/// knowledge.
pub(crate) fn project_ai_state(
    state: crate::engine::GameState,
    viewer_player_id: &str,
) -> crate::engine::GameState {
    project_session_view(
        GameSessionView {
            schema_version: "mtg-game-session/v1".to_string(),
            session_id: String::new(),
            revision: 0,
            state,
            calculated_stats: BTreeMap::new(),
            decision: None,
            error: None,
            match_state: None,
        },
        viewer_player_id,
    )
    .state
}

fn project_network_session_view(
    mut view: GameSessionView,
    access: &NetworkGameAccess,
) -> GameSessionView {
    for player in &mut view.state.players {
        if let Some(username) = access.seat_usernames.get(&player.id) {
            player.name = username.clone();
        }
    }
    project_session_view(view, &access.player_id)
}

fn normalized_username(value: Option<&str>) -> Result<String, String> {
    let username = value.unwrap_or_default().trim();
    if username.is_empty() || username.chars().count() > 32 {
        return Err("A username between 1 and 32 characters is required".to_string());
    }
    Ok(username.to_string())
}

fn human_analytics_pilot(username: &str) -> String {
    format!("human:{}", username.trim())
}

fn lobby_access(lobby: &NetworkLobby, player_id: &str) -> NetworkGameAccess {
    NetworkGameAccess {
        invite_code: lobby.invite_code.clone(),
        player_id: player_id.to_string(),
        seat_token: lobby.seat_tokens[player_id].clone(),
        is_host: lobby.host_player_id == player_id,
        human_player_ids: lobby.human_player_ids.clone(),
        username: lobby
            .seat_usernames
            .get(player_id)
            .cloned()
            .unwrap_or_default(),
        seat_usernames: lobby.seat_usernames.clone(),
    }
}

fn lobby_view(lobby: &NetworkLobby, player_id: &str) -> NetworkLobbyView {
    NetworkLobbyView {
        schema_version: "mtg-network-lobby/v1",
        invite_code: lobby.invite_code.clone(),
        human_seat_count: lobby.human_player_ids.len(),
        player_count: lobby.request.players.len(),
        status: lobby.status,
        error: lobby.error.clone(),
        configuration: NetworkLobbyConfigurationView {
            seed: lobby.request.seed,
            game_mode: lobby.request.game_mode,
            max_turns: lobby.request.max_turns,
            opening_hand_size: if lobby.request.opening_hand_size == 0 {
                7
            } else {
                lobby.request.opening_hand_size
            },
            mulligan_enabled: lobby.request.mulligan_enabled,
            free_mulligans: lobby.request.free_mulligans,
            max_mulligans: lobby.request.max_mulligans,
        },
        seats: lobby
            .request
            .players
            .iter()
            .map(|player| {
                let claimed = lobby.claimed_player_ids.contains(&player.id);
                let joinable = lobby.human_player_ids.contains(&player.id);
                let controller_id = (!claimed).then(|| {
                    lobby
                        .request
                        .ai_controller_by_player_id
                        .get(&player.id)
                        .cloned()
                        .unwrap_or_else(|| "ai-random".to_string())
                });
                NetworkLobbySeatView {
                    player_id: player.id.clone(),
                    starting_life: player.starting_life,
                    username: lobby.seat_usernames.get(&player.id).cloned(),
                    deck_session_id: player.deck_session_id.clone(),
                    deck_name: if player.deck_session_id == "random" {
                        Some("Deck aléatoire".to_string())
                    } else {
                        player.name.clone()
                    },
                    claimed,
                    joinable,
                    ready: claimed && lobby.ready_player_ids.contains(&player.id),
                    controller_id,
                    pilot_id: lobby
                        .request
                        .analytics_pilot_by_player_id
                        .get(&player.id)
                        .cloned()
                        .unwrap_or_else(|| {
                            if claimed {
                                "network-human".to_string()
                            } else {
                                "ai-random".to_string()
                            }
                        }),
                }
            })
            .collect(),
        access: lobby_access(lobby, player_id),
    }
}

fn authenticated_lobby_player(
    invite_code: &str,
    headers: &BTreeMap<String, String>,
) -> Result<String, JsonResponse> {
    let mut registry = network_game_registry()
        .lock()
        .map_err(|_| error_response(500, "Network game registry is unavailable"))?;
    let lobby = registry
        .lobbies
        .get_mut(invite_code)
        .ok_or_else(|| error_response(404, "Unknown network lobby"))?;
    let player_id = headers.get("x-mtg-player-id").cloned().unwrap_or_default();
    let token = headers.get("x-mtg-seat-token").cloned().unwrap_or_default();
    if !lobby.claimed_player_ids.contains(&player_id)
        || lobby.seat_tokens.get(&player_id) != Some(&token)
    {
        return Err(error_response(
            401,
            "A valid network seat token is required",
        ));
    }
    lobby
        .last_seen_by_player_id
        .insert(player_id.clone(), Instant::now());
    Ok(player_id)
}

fn validate_game_mode_player_count(game_mode: GameMode, player_count: usize) -> Result<(), String> {
    match game_mode {
        GameMode::Commander if player_count != 4 => {
            Err("Commander games require exactly four players".to_string())
        }
        GameMode::DuelCommander if player_count != 2 => {
            Err("Duel Commander games require exactly two players".to_string())
        }
        _ => Ok(()),
    }
}

fn create_network_lobby(body: &str) -> JsonResponse {
    let mut request = match serde_json::from_str::<CreateLocalDeckGameRequest>(body) {
        Ok(request) => request,
        Err(error) => return error_response(400, format!("Invalid lobby request: {error}")),
    };
    let host_player_id = request
        .host_player_id
        .clone()
        .unwrap_or_else(|| "player-1".to_string());
    let host_username = match normalized_username(request.host_username.as_deref()) {
        Ok(username) => username,
        Err(error) => return error_response(400, error),
    };
    if let Err(error) = validate_game_mode_player_count(request.game_mode, request.players.len()) {
        return error_response(400, error);
    }
    if request.human_player_ids.len() < 2 || request.human_player_ids.len() > 4 {
        return error_response(400, "Network lobbies require two to four human seats");
    }
    if !request.human_player_ids.contains(&host_player_id) {
        return error_response(400, "The host must own a human seat");
    }
    request.network_multiplayer = false;
    for player_id in &request.human_player_ids {
        if player_id == &host_player_id {
            request
                .analytics_pilot_by_player_id
                .insert(player_id.clone(), human_analytics_pilot(&host_username));
            request.ai_controller_by_player_id.remove(player_id);
        } else {
            request.ai_controller_by_player_id.remove(player_id);
            request
                .analytics_pilot_by_player_id
                .entry(player_id.clone())
                .or_insert_with(|| "ai-random".to_string());
        }
    }
    let mut registry = match network_game_registry().lock() {
        Ok(registry) => registry,
        Err(_) => return error_response(500, "Network game registry is unavailable"),
    };
    let invite_code = loop {
        let candidate = random_game_number();
        if !registry.issued_invite_codes.contains(&candidate)
            && !registry.lobbies.contains_key(&candidate)
            && !registry.invite_to_session.contains_key(&candidate)
        {
            break candidate;
        }
    };
    registry.issued_invite_codes.insert(invite_code.clone());
    let seat_tokens = request
        .human_player_ids
        .iter()
        .map(|player_id| (player_id.clone(), random_access_value(40)))
        .collect::<BTreeMap<_, _>>();
    let lobby = NetworkLobby {
        claimed_player_ids: BTreeSet::from([host_player_id.clone()]),
        host_player_id: host_player_id.clone(),
        human_player_ids: request.human_player_ids.clone(),
        invite_code: invite_code.clone(),
        last_seen_by_player_id: BTreeMap::from([(host_player_id.clone(), Instant::now())]),
        error: None,
        ready_player_ids: BTreeSet::new(),
        request,
        seat_tokens,
        seat_usernames: BTreeMap::from([(host_player_id.clone(), host_username)]),
        status: NetworkLobbyStatus::Waiting,
    };
    let view = lobby_view(&lobby, &host_player_id);
    registry.lobbies.insert(invite_code, lobby);
    json_response(200, &view)
}

fn join_network_lobby(body: &str) -> JsonResponse {
    let request = match serde_json::from_str::<JoinNetworkGameRequest>(body) {
        Ok(request) => request,
        Err(error) => return error_response(400, format!("Invalid lobby join request: {error}")),
    };
    let invite_code = request.invite_code.trim().to_ascii_uppercase();
    let username = match normalized_username(request.username.as_deref()) {
        Ok(username) => username,
        Err(error) => return error_response(400, error),
    };
    let mut registry = match network_game_registry().lock() {
        Ok(registry) => registry,
        Err(_) => return error_response(500, "Network game registry is unavailable"),
    };
    let Some(lobby) = registry.lobbies.get_mut(&invite_code) else {
        return error_response(404, "Unknown network lobby");
    };
    let requested_player_id = request.player_id.as_deref().filter(|id| !id.is_empty());
    let player_id = if let Some(player_id) = requested_player_id {
        let resumes = request.seat_token.as_deref().is_some_and(|token| {
            lobby
                .seat_tokens
                .get(player_id)
                .is_some_and(|expected| expected == token)
        });
        if !resumes || !lobby.claimed_player_ids.contains(player_id) {
            return error_response(409, "The requested lobby seat is unavailable");
        }
        if lobby
            .seat_usernames
            .get(player_id)
            .is_some_and(|existing| !existing.eq_ignore_ascii_case(&username))
        {
            return error_response(409, "This seat belongs to another username");
        }
        player_id.to_string()
    } else {
        let Some(player_id) = lobby
            .human_player_ids
            .iter()
            .find(|id| !lobby.claimed_player_ids.contains(*id))
            .cloned()
        else {
            return error_response(409, "Every human seat is already occupied");
        };
        player_id
    };
    if lobby
        .seat_usernames
        .iter()
        .any(|(seat, existing)| seat != &player_id && existing.eq_ignore_ascii_case(&username))
    {
        return error_response(409, "This username is already in the lobby");
    }
    lobby.claimed_player_ids.insert(player_id.clone());
    lobby
        .last_seen_by_player_id
        .insert(player_id.clone(), Instant::now());
    lobby
        .seat_usernames
        .insert(player_id.clone(), username.clone());
    lobby.ready_player_ids.remove(&player_id);
    lobby.request.ai_controller_by_player_id.remove(&player_id);
    lobby
        .request
        .analytics_pilot_by_player_id
        .insert(player_id.clone(), human_analytics_pilot(&username));
    json_response(200, &lobby_view(lobby, &player_id))
}

fn network_lobby_can_start(lobby: &NetworkLobby) -> bool {
    lobby.claimed_player_ids.len() == lobby.human_player_ids.len()
        && lobby
            .claimed_player_ids
            .iter()
            .all(|player_id| lobby.ready_player_ids.contains(player_id))
        && lobby.request.players.iter().all(|player| {
            let human_seat = lobby.human_player_ids.contains(&player.id);
            if human_seat && player.deck_session_id == "random" {
                return false;
            }
            player.deck_session_id == "random"
                || (!player.deck_session_id.trim().is_empty()
                    && local_deck_session_exists(&player.deck_session_id).unwrap_or(false))
        })
}

fn fail_network_lobby(invite_code: &str, error: impl Into<String>) {
    if let Ok(mut registry) = network_game_registry().lock()
        && let Some(lobby) = registry.lobbies.get_mut(invite_code)
    {
        lobby.status = NetworkLobbyStatus::Failed;
        lobby.error = Some(error.into());
        lobby.ready_player_ids.clear();
    }
}

fn legal_meta_decks(
    game_mode: GameMode,
    opening_hand_size: usize,
) -> Result<Vec<LegalDeckOption>, String> {
    let session_ids = local_meta_deck_session_ids()?;
    let opening_hand_size = if opening_hand_size == 0 {
        7
    } else {
        opening_hand_size
    };
    let mut decks = thread::scope(|scope| {
        session_ids
            .into_iter()
            .map(|session_id| {
                scope.spawn(move || legal_local_deck(&session_id, game_mode, opening_hand_size))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|worker| worker.join().ok().and_then(Result::ok).flatten())
            .collect::<Vec<_>>()
    });
    decks.sort_by(|left, right| {
        left.deck_name
            .cmp(&right.deck_name)
            .then_with(|| left.deck_session_id.cmp(&right.deck_session_id))
    });
    if decks.is_empty() {
        return Err(format!(
            "No legal meta deck is available for the {:?} format",
            game_mode
        ));
    }
    Ok(decks)
}

fn legal_local_deck(
    session_id: &str,
    game_mode: GameMode,
    opening_hand_size: usize,
) -> Result<Option<LegalDeckOption>, String> {
    let deck = local_deck_for_validation(session_id)?;
    let setup = GameSetup {
        players: vec![crate::engine::PlayerDeck {
            id: "legal-deck-candidate".to_string(),
            name: deck.name.clone(),
            starting_life: game_mode.default_starting_life().unwrap_or(20),
            cards: deck.cards,
        }],
        opening_hand_size: opening_hand_size.max(1),
        starting_player: 0,
    };
    Ok(GameRules::for_format(game_mode)
        .validate(&setup)
        .is_ok()
        .then_some(LegalDeckOption {
            deck_session_id: session_id.to_string(),
            deck_name: deck.name,
        }))
}

fn select_legal_random_decks(
    request: &CreateLocalDeckGameRequest,
    required_count: usize,
) -> Result<Vec<(String, String)>, String> {
    if required_count == 0 {
        return Ok(Vec::new());
    }
    let mut candidates = legal_meta_decks(request.game_mode, request.opening_hand_size)?;
    candidates.shuffle(&mut rand::thread_rng());
    candidates.truncate(required_count);
    Ok(candidates
        .into_iter()
        .map(|deck| (deck.deck_session_id, deck.deck_name))
        .collect())
}

fn legal_deck_catalog(body: &str) -> JsonResponse {
    let request = match serde_json::from_str::<LegalDeckCatalogRequest>(body) {
        Ok(request) => request,
        Err(error) => return error_response(400, format!("Invalid legal deck request: {error}")),
    };
    match legal_meta_decks(request.game_mode, request.opening_hand_size) {
        Ok(decks) => json_response(
            200,
            &LegalDeckCatalogResponse {
                schema_version: "mtg-legal-deck-catalog/v1",
                game_mode: request.game_mode,
                decks,
            },
        ),
        Err(error) => error_response(400, error),
    }
}

fn choose_matchmaking_candidate(
    target_rating: f64,
    candidates: &mut [MatchmakingCandidate],
    seed: u64,
) -> Option<MatchmakingCandidate> {
    if candidates.is_empty() {
        return None;
    }
    candidates.shuffle(&mut StdRng::seed_from_u64(seed ^ 0xA11C_E5E1));
    let weights = candidates
        .iter()
        .map(|candidate| {
            let distance = (candidate.rating - target_rating).abs();
            let proximity = 1.0 / (1.0 + distance).powi(2);
            let evidence = 1.0 + (candidate.matches as f64 + 1.0).ln() * 0.05;
            proximity * evidence
        })
        .collect::<Vec<_>>();
    let distribution = WeightedIndex::new(&weights).ok()?;
    let mut rng = StdRng::seed_from_u64(seed);
    candidates.get(distribution.sample(&mut rng)).cloned()
}

fn play_matchmaking(body: &str) -> JsonResponse {
    let request = match serde_json::from_str::<PlayMatchmakingRequest>(body) {
        Ok(request) => request,
        Err(error) => return error_response(400, format!("Invalid matchmaking request: {error}")),
    };
    if request.human_deck_session_id.trim().is_empty() {
        return error_response(400, "Matchmaking requires the human deck session");
    }
    let legal_decks = match legal_meta_decks(request.game_mode, request.opening_hand_size) {
        Ok(decks) => decks,
        Err(error) => return error_response(400, error),
    };
    let human_deck_is_legal = match legal_local_deck(
        &request.human_deck_session_id,
        request.game_mode,
        request.opening_hand_size,
    ) {
        Ok(deck) => deck.is_some(),
        Err(error) => return error_response(400, error),
    };
    if !human_deck_is_legal {
        return error_response(400, "The human deck is not legal for the selected format");
    }

    let analytics = deck_analytics_service().query(DeckAnalyticsQuery {
        analytics_context_id: Some("player-match".to_string()),
        player_count: Some(2),
        game_mode: Some(request.game_mode),
        ..DeckAnalyticsQuery::default()
    });
    let exact_human = analytics.decks.iter().find(|row| {
        row.deck_id == request.human_deck_session_id && row.pilot_id == request.human_pilot_id
    });
    let global_human = analytics
        .human_leaderboard
        .iter()
        .find(|row| row.pilot_id == request.human_pilot_id);
    let (human_rating, human_matches, human_source) = exact_human.map_or_else(
        || {
            global_human.map_or((0.0, 0, "neutral"), |row| {
                (row.plackett_luce_ordinal, row.matches, "human-global")
            })
        },
        |row| (row.plackett_luce_ordinal, row.matches, "human-deck"),
    );

    let allowed = request
        .allowed_ai_controller_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let controllers = controller_catalog()
        .controllers
        .into_iter()
        .filter(|controller| {
            controller.available
                && controller.kind == "model"
                && controller.capabilities.play
                && controller
                    .controller_id
                    .as_deref()
                    .is_some_and(|id| allowed.is_empty() || allowed.contains(id))
        })
        .collect::<Vec<_>>();
    if controllers.is_empty() {
        return error_response(409, "No model AI is online for matchmaking");
    }

    // A trained model's deck strength is primarily learned in its training
    // context. Keep player-match ratings as a fallback for models without a
    // training history yet, so matchmaking remains available from day one.
    let training_analytics_by_pilot = controllers
        .iter()
        .map(|controller| {
            let context_id = format!("training:{}", controller.pilot_id);
            (
                controller.pilot_id.clone(),
                deck_analytics_service().query(DeckAnalyticsQuery {
                    analytics_context_id: Some(context_id),
                    player_count: Some(2),
                    game_mode: Some(request.game_mode),
                    ..DeckAnalyticsQuery::default()
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let has_distinct_deck = legal_decks
        .iter()
        .any(|deck| deck.deck_session_id != request.human_deck_session_id);
    let mut candidates = controllers
        .iter()
        .flat_map(|controller| {
            legal_decks
                .iter()
                .filter(|deck| {
                    !has_distinct_deck || deck.deck_session_id != request.human_deck_session_id
                })
                .map(|deck| {
                    let training_rating = training_analytics_by_pilot
                        .get(&controller.pilot_id)
                        .and_then(|training| {
                            training.decks.iter().find(|row| {
                                row.deck_id == deck.deck_session_id
                                    && row.pilot_id == controller.pilot_id
                            })
                        });
                    let rating = training_rating.or_else(|| {
                        analytics.decks.iter().find(|row| {
                            row.deck_id == deck.deck_session_id
                                && row.pilot_id == controller.pilot_id
                        })
                    });
                    MatchmakingCandidate {
                        controller_id: controller
                            .controller_id
                            .clone()
                            .expect("matchmaking controller has a controller id"),
                        controller_label: controller.label.clone(),
                        deck: deck.clone(),
                        matches: rating.map_or(0, |row| row.matches),
                        pilot_id: controller.pilot_id.clone(),
                        rating: rating.map_or(0.0, |row| row.plackett_luce_ordinal),
                        rating_source: if rating.is_some() {
                            "ai-deck"
                        } else {
                            "neutral"
                        },
                    }
                })
        })
        .collect::<Vec<_>>();
    let mut combined_ranking = candidates
        .iter()
        .map(|candidate| {
            (
                format!(
                    "ai:{}:{}",
                    candidate.controller_id, candidate.deck.deck_session_id
                ),
                candidate.rating,
            )
        })
        .collect::<Vec<_>>();
    combined_ranking.extend(
        analytics
            .human_leaderboard
            .iter()
            .filter(|row| row.pilot_id != request.human_pilot_id)
            .map(|row| (format!("human:{}", row.pilot_id), row.plackett_luce_ordinal)),
    );
    let human_ranking_id = format!("human:{}", request.human_pilot_id);
    combined_ranking.push((human_ranking_id.clone(), human_rating));
    combined_ranking.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let pool_size = combined_ranking.len();
    let human_rank = combined_ranking
        .iter()
        .position(|(id, _)| id == &human_ranking_id)
        .map_or(pool_size, |index| index + 1);
    let Some(opponent) = choose_matchmaking_candidate(human_rating, &mut candidates, request.seed)
    else {
        return error_response(409, "No legal AI deck is available for matchmaking");
    };
    let opponent_ranking_id = format!(
        "ai:{}:{}",
        opponent.controller_id, opponent.deck.deck_session_id
    );
    let opponent_rank = combined_ranking
        .iter()
        .position(|(id, _)| id == &opponent_ranking_id)
        .map_or(pool_size, |index| index + 1);
    json_response(
        200,
        &PlayMatchmakingResponse {
            schema_version: "mtg-play-matchmaking/v1",
            game_mode: request.game_mode,
            human: MatchmakingRatingView {
                matches: human_matches,
                pilot_id: request.human_pilot_id,
                plackett_luce_ordinal: human_rating,
                rank: human_rank,
                pool_size,
                source: human_source,
            },
            opponent: MatchmakingOpponentView {
                controller_id: opponent.controller_id,
                controller_label: opponent.controller_label,
                deck_session_id: opponent.deck.deck_session_id,
                deck_name: opponent.deck.deck_name,
                matches: opponent.matches,
                pilot_id: opponent.pilot_id,
                plackett_luce_ordinal: opponent.rating,
                rank: opponent_rank,
                pool_size,
                rating_distance: (opponent.rating - human_rating).abs(),
                rating_source: opponent.rating_source,
            },
        },
    )
}

fn launch_network_lobby(invite_code: String) {
    thread::spawn(move || {
        let Some(mut lobby) = network_game_registry()
            .lock()
            .ok()
            .and_then(|registry| registry.lobbies.get(&invite_code).cloned())
        else {
            return;
        };
        let random_deck_count = lobby
            .request
            .players
            .iter()
            .filter(|player| player.deck_session_id == "random")
            .count();
        let fixed_deck_session_ids = lobby
            .request
            .players
            .iter()
            .filter(|player| player.deck_session_id != "random")
            .map(|player| player.deck_session_id.clone())
            .collect::<Vec<_>>();
        let request_for_random_selection = lobby.request.clone();
        let (random_decks, fixed_deck_error) = thread::scope(|scope| {
            let random_worker = scope.spawn(move || {
                select_legal_random_decks(&request_for_random_selection, random_deck_count)
            });
            let fixed_workers = fixed_deck_session_ids
                .into_iter()
                .map(|session_id| scope.spawn(move || compiled_local_deck(&session_id, 0)))
                .collect::<Vec<_>>();
            let fixed_deck_error =
                fixed_workers
                    .into_iter()
                    .find_map(|worker| match worker.join() {
                        Ok(Ok(_)) => None,
                        Ok(Err(error)) => Some(error),
                        Err(_) => Some("local deck compilation panicked".to_string()),
                    });
            (random_worker.join(), fixed_deck_error)
        });
        if let Some(error) = fixed_deck_error {
            fail_network_lobby(&invite_code, error);
            return;
        }
        let mut random_decks = match random_decks {
            Ok(Ok(decks)) => decks,
            Err(_) => {
                fail_network_lobby(&invite_code, "random deck selection panicked");
                return;
            }
            Ok(Err(error)) => {
                fail_network_lobby(&invite_code, error);
                return;
            }
        };
        random_decks.shuffle(&mut rand::thread_rng());
        let mut random_decks = random_decks.into_iter().cycle();
        for player in &mut lobby.request.players {
            if player.deck_session_id != "random" {
                continue;
            }
            let Some((deck_session_id, deck_name)) = random_decks.next() else {
                fail_network_lobby(&invite_code, "No meta deck is available for a random seat");
                return;
            };
            player.deck_session_id = deck_session_id;
            player.name = Some(deck_name);
        }
        lobby.request.players.shuffle(&mut rand::thread_rng());
        lobby.request.starting_player = 0;
        let serialized_request = match serde_json::to_string(&lobby.request) {
            Ok(request) => request,
            Err(error) => {
                fail_network_lobby(&invite_code, format!("Lobby serialization failed: {error}"));
                return;
            }
        };
        let response = create_local_deck_game(&serialized_request);
        if response.status != 200 {
            let message = serde_json::from_str::<ErrorBody>(&response.body)
                .map(|body| body.error)
                .unwrap_or_else(|_| response.body);
            fail_network_lobby(&invite_code, message);
            return;
        }
        let payload = match serde_json::from_str::<Value>(&response.body) {
            Ok(payload) => payload,
            Err(error) => {
                fail_network_lobby(&invite_code, format!("Invalid game bootstrap: {error}"));
                return;
            }
        };
        let Some(session_id) = payload
            .get("session")
            .and_then(|session| session.get("sessionId"))
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            fail_network_lobby(&invite_code, "The game bootstrap has no session id");
            return;
        };
        let card_catalog = match payload
            .get("cardCatalog")
            .cloned()
            .and_then(|catalog| serde_json::from_value::<BTreeMap<String, Value>>(catalog).ok())
        {
            Some(catalog) => catalog,
            None => {
                let _ = game_session_manager().remove(&session_id);
                fail_network_lobby(&invite_code, "The game bootstrap has no card catalog");
                return;
            }
        };
        let Ok(mut registry) = network_game_registry().lock() else {
            let _ = game_session_manager().remove(&session_id);
            return;
        };
        if !registry.lobbies.contains_key(&invite_code) {
            let _ = game_session_manager().remove(&session_id);
            return;
        }
        registry.lobbies.remove(&invite_code);
        registry
            .invite_to_session
            .insert(invite_code.clone(), session_id.clone());
        registry.sessions.insert(
            session_id.clone(),
            NetworkGame {
                card_catalog,
                claimed_player_ids: lobby.claimed_player_ids,
                host_player_id: lobby.host_player_id,
                human_player_ids: lobby.human_player_ids,
                invite_code: lobby.invite_code,
                seat_tokens: lobby.seat_tokens,
                seat_usernames: lobby.seat_usernames,
                session_id,
            },
        );
    });
}

fn route_network_lobby(
    method: &str,
    path: &str,
    body: &str,
    headers: &BTreeMap<String, String>,
) -> Option<JsonResponse> {
    if method == "POST" && path == "/game/lobbies" {
        return Some(create_network_lobby(body));
    }
    if method == "POST" && path == "/game/lobbies/join" {
        return Some(join_network_lobby(body));
    }
    let suffix = path.strip_prefix("/game/lobbies/")?;
    let invite_code = suffix.split('/').next().unwrap_or_default();
    let player_id = match authenticated_lobby_player(invite_code, headers) {
        Ok(player_id) => player_id,
        Err(response) => return Some(response),
    };
    if method == "GET" && suffix == invite_code {
        let registry = network_game_registry().lock().ok()?;
        return registry
            .lobbies
            .get(invite_code)
            .map(|lobby| json_response(200, &lobby_view(lobby, &player_id)));
    }
    if method == "POST" && suffix == format!("{invite_code}/leave") {
        let mut registry = network_game_registry().lock().ok()?;
        let (close_lobby, next_host_player_id) = {
            let lobby = registry.lobbies.get_mut(invite_code)?;
            lobby.claimed_player_ids.remove(&player_id);
            lobby.last_seen_by_player_id.remove(&player_id);
            lobby.ready_player_ids.remove(&player_id);
            lobby.seat_usernames.remove(&player_id);
            lobby
                .seat_tokens
                .insert(player_id.clone(), random_access_value(40));
            lobby.request.ai_controller_by_player_id.remove(&player_id);
            lobby
                .request
                .analytics_pilot_by_player_id
                .insert(player_id.clone(), "ai-random".to_string());
            if let Some(player) = lobby
                .request
                .players
                .iter_mut()
                .find(|seat| seat.id == player_id)
            {
                player.deck_session_id = "random".to_string();
                player.name = Some("Deck aléatoire".to_string());
            }
            let close_lobby = lobby.claimed_player_ids.is_empty();
            if !close_lobby && lobby.host_player_id == player_id {
                lobby.host_player_id = lobby
                    .claimed_player_ids
                    .iter()
                    .next()
                    .expect("a non-empty lobby has a host candidate")
                    .clone();
            }
            (
                close_lobby,
                (!close_lobby).then(|| lobby.host_player_id.clone()),
            )
        };
        if close_lobby {
            registry.lobbies.remove(invite_code);
        }
        return Some(json_response(
            200,
            &json!({
                "closed": close_lobby,
                "hostPlayerId": next_host_player_id,
                "ok": true,
            }),
        ));
    }
    if method == "PUT" && suffix == format!("{invite_code}/seat") {
        let update = match serde_json::from_str::<UpdateLobbySeatRequest>(body) {
            Ok(update) => update,
            Err(error) => {
                return Some(error_response(400, format!("Invalid seat update: {error}")));
            }
        };
        let selected_deck = if let Some(deck_session_id) = update.deck_session_id.as_deref() {
            let deck_session_id = deck_session_id.trim();
            if deck_session_id == "random" {
                Some(("random".to_string(), "Deck aléatoire".to_string()))
            } else {
                match compiled_local_deck(deck_session_id, 0) {
                    Ok(deck) => Some((deck_session_id.to_string(), deck.name)),
                    Err(_) => return Some(error_response(400, "Unknown local deck")),
                }
            }
        } else {
            None
        };
        let username = if update.username.is_some() {
            match normalized_username(update.username.as_deref()) {
                Ok(username) => Some(username),
                Err(error) => return Some(error_response(400, error)),
            }
        } else {
            None
        };
        let mut registry = network_game_registry().lock().ok()?;
        let lobby = registry.lobbies.get_mut(invite_code)?;
        if lobby.status == NetworkLobbyStatus::Starting {
            return Some(error_response(409, "The network game is already starting"));
        }
        let target_player_id = update
            .player_id
            .as_deref()
            .unwrap_or(&player_id)
            .to_string();
        let edits_another_seat = target_player_id != player_id;
        if edits_another_seat
            && (lobby.host_player_id != player_id
                || lobby.claimed_player_ids.contains(&target_player_id))
        {
            return Some(error_response(
                403,
                "The host may edit only unoccupied AI seats",
            ));
        }
        if edits_another_seat && (update.username.is_some() || update.ready.is_some()) {
            return Some(error_response(
                403,
                "Only a seated player may change their username or ready state",
            ));
        }
        if let Some(username) = username.as_ref()
            && lobby.seat_usernames.iter().any(|(seat, existing)| {
                seat != &player_id && existing.eq_ignore_ascii_case(username)
            })
        {
            return Some(error_response(409, "This username is already in the lobby"));
        }
        let Some(player) = lobby
            .request
            .players
            .iter_mut()
            .find(|seat| seat.id == target_player_id)
        else {
            return Some(error_response(400, "Lobby seat has no matching player"));
        };
        if let Some((deck_session_id, deck_name)) = selected_deck {
            player.deck_session_id = deck_session_id;
            player.name = Some(deck_name);
            lobby.ready_player_ids.remove(&target_player_id);
        }
        if let Some(username) = username {
            lobby
                .request
                .analytics_pilot_by_player_id
                .insert(player_id.clone(), human_analytics_pilot(&username));
            lobby.seat_usernames.insert(player_id.clone(), username);
            lobby.ready_player_ids.remove(&player_id);
        }
        if let Some(ready) = update.ready {
            if ready
                && (player.deck_session_id.trim().is_empty() || player.deck_session_id == "random")
            {
                return Some(error_response(409, "Select a deck before becoming ready"));
            }
            if ready {
                lobby.ready_player_ids.insert(player_id.clone());
            } else {
                lobby.ready_player_ids.remove(&player_id);
            }
        }
        lobby.error = None;
        lobby.status = NetworkLobbyStatus::Waiting;
        let should_launch = network_lobby_can_start(lobby);
        if should_launch {
            lobby.status = NetworkLobbyStatus::Starting;
        }
        let view = json_response(200, &lobby_view(lobby, &player_id));
        drop(registry);
        if should_launch {
            launch_network_lobby(invite_code.to_string());
        }
        return Some(view);
    }
    if method == "POST" && suffix == format!("{invite_code}/start") {
        return Some(error_response(
            409,
            "Use each player's Ready button; the game starts automatically",
        ));
    }
    Some(error_response(404, "Unknown lobby operation"))
}

fn register_network_game(
    session_id: &str,
    host_player_id: &str,
    human_player_ids: &[String],
    card_catalog: BTreeMap<String, Value>,
    host_username: Option<&str>,
) -> Result<NetworkGameAccess, String> {
    if human_player_ids.is_empty() {
        return Err("Network games require at least one human seat".to_string());
    }
    if !human_player_ids
        .iter()
        .any(|player_id| player_id == host_player_id)
    {
        return Err("The network host must control a human seat".to_string());
    }
    let mut registry = network_game_registry()
        .lock()
        .map_err(|_| "network game registry is unavailable".to_string())?;
    let invite_code = loop {
        let candidate = random_game_number();
        if !registry.issued_invite_codes.contains(&candidate)
            && !registry.invite_to_session.contains_key(&candidate)
        {
            break candidate;
        }
    };
    registry.issued_invite_codes.insert(invite_code.clone());
    let seat_tokens = human_player_ids
        .iter()
        .map(|player_id| (player_id.clone(), random_access_value(40)))
        .collect::<BTreeMap<_, _>>();
    let host_username = normalized_username(host_username)?;
    let seat_usernames = BTreeMap::from([(host_player_id.to_string(), host_username.clone())]);
    let host_access = NetworkGameAccess {
        invite_code: invite_code.clone(),
        player_id: host_player_id.to_string(),
        seat_token: seat_tokens
            .get(host_player_id)
            .expect("host seat token exists")
            .clone(),
        is_host: true,
        human_player_ids: human_player_ids.to_vec(),
        username: host_username,
        seat_usernames: seat_usernames.clone(),
    };
    registry
        .invite_to_session
        .insert(invite_code.clone(), session_id.to_string());
    registry.sessions.insert(
        session_id.to_string(),
        NetworkGame {
            card_catalog,
            claimed_player_ids: BTreeSet::from([host_player_id.to_string()]),
            host_player_id: host_player_id.to_string(),
            human_player_ids: human_player_ids.to_vec(),
            invite_code,
            seat_tokens,
            seat_usernames,
            session_id: session_id.to_string(),
        },
    );
    Ok(host_access)
}

fn unregister_network_game(session_id: &str) {
    let Ok(mut registry) = network_game_registry().lock() else {
        return;
    };
    if let Some(game) = registry.sessions.remove(session_id) {
        registry.invite_to_session.remove(&game.invite_code);
    }
}

fn leave_network_game(session_id: &str, player_id: &str) -> Result<(), JsonResponse> {
    let mut registry = network_game_registry()
        .lock()
        .map_err(|_| error_response(500, "Network game registry is unavailable"))?;
    let game = registry
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| error_response(404, "Unknown network game"))?;
    if game.host_player_id == player_id {
        return Err(error_response(403, "The host must close the network game"));
    }
    game.claimed_player_ids.remove(player_id);
    Ok(())
}

fn authenticated_network_access(
    session_id: &str,
    headers: &BTreeMap<String, String>,
) -> Result<Option<NetworkGameAccess>, JsonResponse> {
    let registry = network_game_registry()
        .lock()
        .map_err(|_| error_response(500, "Network game registry is unavailable"))?;
    let Some(game) = registry.sessions.get(session_id) else {
        return Ok(None);
    };
    let player_id = headers
        .get("x-mtg-player-id")
        .map(String::as_str)
        .unwrap_or_default();
    let seat_token = headers
        .get("x-mtg-seat-token")
        .map(String::as_str)
        .unwrap_or_default();
    if player_id.is_empty()
        || game
            .seat_tokens
            .get(player_id)
            .is_none_or(|expected| expected != seat_token)
        || !game.claimed_player_ids.contains(player_id)
    {
        return Err(error_response(
            401,
            "A valid network seat token is required",
        ));
    }
    Ok(Some(NetworkGameAccess {
        invite_code: game.invite_code.clone(),
        player_id: player_id.to_string(),
        seat_token: seat_token.to_string(),
        is_host: game.host_player_id == player_id,
        human_player_ids: game.human_player_ids.clone(),
        username: game
            .seat_usernames
            .get(player_id)
            .cloned()
            .unwrap_or_default(),
        seat_usernames: game.seat_usernames.clone(),
    }))
}

fn join_network_game(body: &str) -> JsonResponse {
    let request = match serde_json::from_str::<JoinNetworkGameRequest>(body) {
        Ok(request) => request,
        Err(error) => return error_response(400, format!("Invalid network join request: {error}")),
    };
    let invite_code = request.invite_code.trim().to_ascii_uppercase();
    let joined =
        {
            let mut registry = match network_game_registry().lock() {
                Ok(registry) => registry,
                Err(_) => return error_response(500, "Network game registry is unavailable"),
            };
            let Some(session_id) = registry.invite_to_session.get(&invite_code).cloned() else {
                return error_response(404, "Unknown or expired network invitation");
            };
            let Some(game) = registry.sessions.get_mut(&session_id) else {
                return error_response(404, "Unknown or expired network invitation");
            };
            let username = match normalized_username(request.username.as_deref()) {
                Ok(username) => username,
                Err(error) => return error_response(400, error),
            };
            let requested_player_id = request
                .player_id
                .as_deref()
                .map(str::trim)
                .filter(|player_id| !player_id.is_empty());
            let player_id = if let Some(player_id) = requested_player_id {
                if !game
                    .human_player_ids
                    .iter()
                    .any(|candidate| candidate == player_id)
                {
                    return error_response(400, "The requested seat is not human-controlled");
                }
                if game.claimed_player_ids.contains(player_id) {
                    let resumes_owned_seat = request.seat_token.as_deref().is_some_and(|token| {
                        game.seat_tokens
                            .get(player_id)
                            .is_some_and(|expected| expected == token)
                    });
                    if !resumes_owned_seat {
                        return error_response(409, "The requested seat is already occupied");
                    }
                    if game
                        .seat_usernames
                        .get(player_id)
                        .is_some_and(|existing| !existing.eq_ignore_ascii_case(&username))
                    {
                        return error_response(409, "This seat belongs to another username");
                    }
                }
                player_id.to_string()
            } else {
                let Some(player_id) = game
                    .human_player_ids
                    .iter()
                    .find(|player_id| !game.claimed_player_ids.contains(*player_id))
                    .cloned()
                else {
                    return error_response(409, "Every human seat is already occupied");
                };
                player_id
            };
            if game.seat_usernames.iter().any(|(seat, existing)| {
                seat != &player_id && existing.eq_ignore_ascii_case(&username)
            }) {
                return error_response(409, "This username is already in the game");
            }
            game.claimed_player_ids.insert(player_id.clone());
            game.seat_usernames
                .insert(player_id.clone(), username.clone());
            let access = NetworkGameAccess {
                invite_code: game.invite_code.clone(),
                player_id: player_id.clone(),
                seat_token: game
                    .seat_tokens
                    .get(&player_id)
                    .expect("network seat token exists")
                    .clone(),
                is_host: false,
                human_player_ids: game.human_player_ids.clone(),
                username,
                seat_usernames: game.seat_usernames.clone(),
            };
            (game.session_id.clone(), game.card_catalog.clone(), access)
        };
    let view = match game_session_manager().view(&joined.0) {
        Ok(view) => project_network_session_view(view, &joined.2),
        Err(error) => {
            unregister_network_game(&joined.0);
            return session_error_response(error);
        }
    };
    json_response(
        200,
        &LocalDeckGameBootstrap {
            schema_version: "mtg-game-bootstrap/v1",
            session: view,
            card_catalog: joined.1,
            network_access: Some(joined.2),
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalDeckGameBootstrap {
    schema_version: &'static str,
    session: GameSessionView,
    card_catalog: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    network_access: Option<NetworkGameAccess>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkGameAccess {
    invite_code: String,
    player_id: String,
    seat_token: String,
    is_host: bool,
    human_player_ids: Vec<String>,
    username: String,
    seat_usernames: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JoinNetworkGameRequest {
    invite_code: String,
    #[serde(default)]
    player_id: Option<String>,
    #[serde(default)]
    seat_token: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Clone)]
struct NetworkGame {
    card_catalog: BTreeMap<String, Value>,
    claimed_player_ids: BTreeSet<String>,
    host_player_id: String,
    human_player_ids: Vec<String>,
    invite_code: String,
    seat_tokens: BTreeMap<String, String>,
    seat_usernames: BTreeMap<String, String>,
    session_id: String,
}

#[derive(Default)]
struct NetworkGameRegistry {
    issued_invite_codes: BTreeSet<String>,
    invite_to_session: BTreeMap<String, String>,
    sessions: BTreeMap<String, NetworkGame>,
    lobbies: BTreeMap<String, NetworkLobby>,
}

#[derive(Clone)]
struct NetworkLobby {
    claimed_player_ids: BTreeSet<String>,
    error: Option<String>,
    host_player_id: String,
    human_player_ids: Vec<String>,
    invite_code: String,
    last_seen_by_player_id: BTreeMap<String, Instant>,
    ready_player_ids: BTreeSet<String>,
    request: CreateLocalDeckGameRequest,
    seat_tokens: BTreeMap<String, String>,
    seat_usernames: BTreeMap<String, String>,
    status: NetworkLobbyStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateLobbySeatRequest {
    #[serde(default)]
    player_id: Option<String>,
    #[serde(default)]
    deck_session_id: Option<String>,
    #[serde(default)]
    ready: Option<bool>,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum NetworkLobbyStatus {
    #[default]
    Waiting,
    Starting,
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkLobbySeatView {
    player_id: String,
    starting_life: i32,
    username: Option<String>,
    deck_session_id: String,
    deck_name: Option<String>,
    claimed: bool,
    joinable: bool,
    ready: bool,
    controller_id: Option<String>,
    pilot_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkLobbyConfigurationView {
    seed: u64,
    game_mode: GameMode,
    max_turns: u32,
    opening_hand_size: usize,
    mulligan_enabled: bool,
    free_mulligans: usize,
    max_mulligans: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkLobbyView {
    schema_version: &'static str,
    invite_code: String,
    human_seat_count: usize,
    player_count: usize,
    status: NetworkLobbyStatus,
    error: Option<String>,
    configuration: NetworkLobbyConfigurationView,
    seats: Vec<NetworkLobbySeatView>,
    access: NetworkGameAccess,
}

fn create_local_deck_game(body: &str) -> JsonResponse {
    let mut request = match serde_json::from_str::<CreateLocalDeckGameRequest>(body) {
        Ok(request) => request,
        Err(error) => {
            return error_response(400, format!("Invalid local deck game request: {error}"));
        }
    };
    if request.players.len() < 2 || request.players.len() > 4 {
        return error_response(400, "Local deck games require two to four players");
    }
    if let Err(error) = validate_game_mode_player_count(request.game_mode, request.players.len()) {
        return error_response(400, error);
    }
    let random_deck_count = request
        .players
        .iter()
        .filter(|player| player.deck_session_id == "random")
        .count();
    if random_deck_count > 0 {
        let random_decks = match select_legal_random_decks(&request, random_deck_count) {
            Ok(decks) => decks,
            Err(error) => return error_response(400, error),
        };
        let mut random_decks = random_decks.into_iter().cycle();
        for player in &mut request.players {
            if player.deck_session_id != "random" {
                continue;
            }
            let Some((deck_session_id, deck_name)) = random_decks.next() else {
                return error_response(400, "No legal deck is available for a random seat");
            };
            player.deck_session_id = deck_session_id;
            player.name = Some(deck_name);
        }
    }
    let network_multiplayer = request.network_multiplayer;
    let host_player_id = request
        .host_player_id
        .clone()
        .unwrap_or_else(|| "player-1".to_string());
    let human_player_ids = request.human_player_ids.clone();
    let host_username = request.host_username.clone();
    let mut players = Vec::with_capacity(request.players.len());
    let mut card_catalog = BTreeMap::new();
    let mut analytics_deck_session_by_player_id = BTreeMap::new();
    const DECK_COMPILATION_STACK_BYTES: usize = 16 * 1024 * 1024;
    let compiled_decks = match thread::scope(|scope| {
        let workers = request
            .players
            .iter()
            .enumerate()
            .map(|(player_index, player)| {
                let deck_session_id = player.deck_session_id.clone();
                thread::Builder::new()
                    .name(format!("deck-compiler-{}", player_index + 1))
                    .stack_size(DECK_COMPILATION_STACK_BYTES)
                    .spawn_scoped(scope, move || {
                        compiled_local_deck(&deck_session_id, player_index)
                    })
                    .map_err(|error| format!("could not start deck compiler: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, String>(
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|_| Err("local deck compilation panicked".to_string()))
                })
                .collect::<Vec<_>>(),
        )
    }) {
        Ok(decks) => decks,
        Err(error) => return error_response(500, error),
    };
    for (player, compiled_deck) in request.players.iter().zip(compiled_decks) {
        let deck = match compiled_deck {
            Ok(deck) => deck,
            Err(error) => return error_response(400, error),
        };
        card_catalog.extend(deck.presentation_catalog);
        analytics_deck_session_by_player_id.insert(player.id.clone(), deck.session_id.clone());
        players.push(crate::engine::PlayerDeck {
            id: player.id.clone(),
            name: player.name.clone().unwrap_or(deck.name),
            starting_life: player.starting_life,
            cards: deck.cards,
        });
    }
    let mut session = match game_session_manager().create(CreateGameSessionRequest {
        setup: GameSetup {
            players,
            opening_hand_size: if request.opening_hand_size == 0 {
                7
            } else {
                request.opening_hand_size
            },
            starting_player: request.starting_player,
        },
        seed: request.seed,
        game_mode: request.game_mode,
        max_turns: request.max_turns,
        human_player_ids: request.human_player_ids,
        combat_declaration_revision_player_ids: None,
        ai_controller_by_player_id: request.ai_controller_by_player_id,
        analytics_pilot_by_player_id: request.analytics_pilot_by_player_id,
        analytics_context_id: request.analytics_context_id,
        analytics_deck_session_by_player_id,
        punching_bag_player_ids: Vec::new(),
        opening_hand_selection_pool_size_by_player_id: BTreeMap::new(),
        training_anchor_deadline_round_by_player_id: BTreeMap::new(),
        hold_priority_player_ids: request.hold_priority_player_ids,
        mulligan_enabled: request.mulligan_enabled,
        free_mulligans: request.free_mulligans,
        max_mulligans: request.max_mulligans,
        wait_timeout_ms: request.wait_timeout_ms,
        human_decision_timeout_ms: request.human_decision_timeout_ms,
    }) {
        Ok(session) => session,
        Err(error) => return session_error_response(error),
    };
    let network_access = if network_multiplayer {
        match register_network_game(
            &session.session_id,
            &host_player_id,
            &human_player_ids,
            card_catalog.clone(),
            host_username.as_deref(),
        ) {
            Ok(access) => {
                session = project_network_session_view(session, &access);
                Some(access)
            }
            Err(error) => {
                let _ = game_session_manager().remove(&session.session_id);
                return error_response(400, error);
            }
        }
    } else {
        None
    };
    json_response(
        200,
        &LocalDeckGameBootstrap {
            schema_version: "mtg-game-bootstrap/v1",
            session,
            card_catalog,
            network_access,
        },
    )
}

fn json_response<T: Serialize>(status: u16, body: &T) -> JsonResponse {
    JsonResponse {
        status,
        body: serde_json::to_string(body).expect("response body serializes"),
    }
}

fn error_response(status: u16, message: impl Into<String>) -> JsonResponse {
    json_response(
        status,
        &ErrorBody {
            error: message.into(),
        },
    )
}

fn collect_canonical_functions(
    value: &Value,
    path: &str,
    rule_index: usize,
    covered_by_executable_rule: bool,
    functions: &mut Vec<Value>,
) {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_canonical_functions(
                    value,
                    &format!("{path}[{index}]"),
                    rule_index,
                    covered_by_executable_rule,
                    functions,
                );
            }
        }
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str) {
                functions.push(json!({
                    "ruleIndex": rule_index,
                    "path": path,
                    "kind": kind,
                    "coveredByExecutableRule": covered_by_executable_rule,
                }));
            }
            for (key, value) in object {
                collect_canonical_functions(
                    value,
                    &format!("{path}.{key}"),
                    rule_index,
                    covered_by_executable_rule,
                    functions,
                );
            }
        }
        _ => {}
    }
}

fn compact_oracle_rules(request: OracleCardParseRequest) -> Value {
    let result = parse_oracle_card(request);
    let selected_face_id = result.context.faces.first().map(|face| face.id.as_str());
    let candidate_rules = playable_rules_for_face(&result, selected_face_id);
    let executable_rule_count = candidate_rules
        .iter()
        .filter(|rule| rule_is_executable(rule))
        .count();
    let unexecutable_rule_count = candidate_rules.len() - executable_rule_count;
    let rule_coverage = candidate_rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            json!({
                "index": index,
                "kind": rule["kind"],
                "executable": rule_is_executable(rule),
            })
        })
        .collect::<Vec<_>>();
    let mut function_coverage = Vec::new();
    for (index, rule) in candidate_rules.iter().enumerate() {
        collect_canonical_functions(
            rule,
            "$",
            index,
            rule_is_executable(rule),
            &mut function_coverage,
        );
    }
    let covered_canonical_function_count = function_coverage
        .iter()
        .filter(|function| function["coveredByExecutableRule"] == true)
        .count();
    let uncovered_canonical_function_count =
        function_coverage.len() - covered_canonical_function_count;
    let engine_status = if result.status == "canonical" && unexecutable_rule_count == 0 {
        "executable"
    } else {
        "incomplete"
    };
    let rules = candidate_rules
        .iter()
        .filter(|rule| rule_is_executable(rule))
        .cloned()
        .collect::<Vec<_>>();

    json!({
        "schemaVersion": "oracle-rules/v1",
        "parserVersion": result.schema_version,
        "canonicalSchemaVersion": "canonical-rule-ir/v1",
        "engineVersion": concat!("mtg-engine/", env!("CARGO_PKG_VERSION")),
        "status": result.status,
        "abilities": result.abilities,
        "engineStatus": engine_status,
        "executableRuleCount": executable_rule_count,
        "unexecutableRuleCount": unexecutable_rule_count,
        "ruleCoverage": rule_coverage,
        "canonicalFunctionCount": function_coverage.len(),
        "coveredCanonicalFunctionCount": covered_canonical_function_count,
        "uncoveredCanonicalFunctionCount": uncovered_canonical_function_count,
        "functionCoverage": function_coverage,
        "candidateRules": candidate_rules,
        "rules": rules,
    })
}

fn game_session_manager() -> &'static GameSessionManager {
    static MANAGER: OnceLock<GameSessionManager> = OnceLock::new();
    MANAGER.get_or_init(|| GameSessionManager::with_analytics(deck_analytics_service().clone()))
}

fn deck_analytics_service() -> &'static DeckAnalyticsService {
    static ANALYTICS: OnceLock<DeckAnalyticsService> = OnceLock::new();
    ANALYTICS.get_or_init(DeckAnalyticsService::from_env)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateGameSetupRequest {
    setup: GameSetup,
    #[serde(default)]
    game_mode: GameMode,
}

fn validate_game_setup(body: &str) -> JsonResponse {
    let Ok(request) = serde_json::from_str::<ValidateGameSetupRequest>(body) else {
        return error_response(400, "Invalid game setup validation request");
    };
    let mut setup = request.setup;
    match GameRules::for_format(request.game_mode).apply(&mut setup) {
        Ok(()) => json_response(
            200,
            &json!({
                "valid": true,
                "gameMode": request.game_mode,
                "startingLife": setup.players.first().map(|player| player.starting_life),
            }),
        ),
        Err(error) => json_response(
            200,
            &json!({
                "valid": false,
                "gameMode": request.game_mode,
                "violations": error.violations,
            }),
        ),
    }
}

fn session_error_response(error: GameSessionError) -> JsonResponse {
    let message = error.to_string();
    let status = if message.starts_with("unknown game session") {
        404
    } else if message.starts_with("stale game session")
        || message.starts_with("game session is awaiting")
        || message.starts_with("game session is not awaiting")
        || message.starts_with("action ")
    {
        409
    } else {
        400
    };
    error_response(status, message)
}

fn route_game_session(
    method: &str,
    path: &str,
    body: &str,
    headers: &BTreeMap<String, String>,
) -> Option<JsonResponse> {
    if method == "POST" && path == "/game/sessions/from-local-decks" {
        return Some(create_local_deck_game(body));
    }
    if method == "POST" && path == "/game/sessions/join" {
        return Some(join_network_game(body));
    }
    if method == "POST" && path == "/game/sessions" {
        return Some(
            match serde_json::from_str::<CreateGameSessionRequest>(body) {
                Ok(request) => match game_session_manager().create(request) {
                    Ok(view) => json_response(200, &view),
                    Err(error) => session_error_response(error),
                },
                Err(error) => error_response(400, format!("Invalid game session request: {error}")),
            },
        );
    }

    let suffix = path.strip_prefix("/game/sessions/")?;
    if method == "DELETE" && !suffix.contains('/') {
        let access = match authenticated_network_access(suffix, headers) {
            Ok(access) => access,
            Err(response) => return Some(response),
        };
        if access.as_ref().is_some_and(|access| !access.is_host) {
            return Some(error_response(
                403,
                "Only the network host can close the game",
            ));
        }
        return Some(match game_session_manager().remove(suffix) {
            Ok(()) => {
                unregister_network_game(suffix);
                json_response(200, &serde_json::json!({ "ok": true }))
            }
            Err(error) => session_error_response(error),
        });
    }
    if method == "GET" && !suffix.contains('/') {
        let access = match authenticated_network_access(suffix, headers) {
            Ok(access) => access,
            Err(response) => return Some(response),
        };
        return Some(match game_session_manager().view(suffix) {
            Ok(view) => json_response(
                200,
                &access
                    .map(|access| project_network_session_view(view.clone(), &access))
                    .unwrap_or(view),
            ),
            Err(error) => session_error_response(error),
        });
    }
    if method == "PUT" {
        let session_id = suffix.strip_suffix("/settings")?;
        if session_id.is_empty() || session_id.contains('/') {
            return None;
        }
        let access = match authenticated_network_access(session_id, headers) {
            Ok(access) => access,
            Err(response) => return Some(response),
        };
        if access.as_ref().is_some_and(|access| !access.is_host) {
            return Some(error_response(
                403,
                "Only the network host can change game-wide settings",
            ));
        }
        return Some(
            match serde_json::from_str::<UpdateGameSessionSettings>(body) {
                Ok(settings) => {
                    match game_session_manager().update_settings(session_id, settings) {
                        Ok(view) => json_response(
                            200,
                            &access
                                .map(|access| project_network_session_view(view.clone(), &access))
                                .unwrap_or(view),
                        ),
                        Err(error) => session_error_response(error),
                    }
                }
                Err(error) => {
                    error_response(400, format!("Invalid game session settings: {error}"))
                }
            },
        );
    }
    if method == "POST" {
        if let Some(session_id) = suffix.strip_suffix("/leave")
            && !session_id.is_empty()
            && !session_id.contains('/')
        {
            let access = match authenticated_network_access(session_id, headers) {
                Ok(Some(access)) => access,
                Ok(None) => return Some(error_response(400, "This is not a network game")),
                Err(response) => return Some(response),
            };
            return Some(match leave_network_game(session_id, &access.player_id) {
                Ok(()) => json_response(200, &json!({ "ok": true })),
                Err(response) => response,
            });
        }
    }
    if method == "POST" {
        let session_id = suffix.strip_suffix("/actions")?;
        if session_id.is_empty() || session_id.contains('/') {
            return None;
        }
        let access = match authenticated_network_access(session_id, headers) {
            Ok(access) => access,
            Err(response) => return Some(response),
        };
        return Some(
            match serde_json::from_str::<SubmitGameSessionAction>(body) {
                Ok(submission) => {
                    if let Some(access) = &access {
                        match game_session_manager().view(session_id) {
                            Ok(view)
                                if view.decision.as_ref().is_none_or(|decision| {
                                    decision.player_id != access.player_id
                                }) =>
                            {
                                return Some(error_response(
                                    403,
                                    "This decision belongs to another network seat",
                                ));
                            }
                            Ok(_) => {}
                            Err(error) => return Some(session_error_response(error)),
                        }
                    }
                    match game_session_manager().submit(session_id, submission) {
                        Ok(view) => json_response(
                            200,
                            &access
                                .map(|access| project_network_session_view(view.clone(), &access))
                                .unwrap_or(view),
                        ),
                        Err(error) => session_error_response(error),
                    }
                }
                Err(error) => error_response(400, format!("Invalid game action request: {error}")),
            },
        );
    }
    None
}

pub fn route_json(method: &str, path: &str, body: &str) -> JsonResponse {
    route_json_with_headers(method, path, body, &BTreeMap::new())
}

fn route_json_with_headers(
    method: &str,
    path: &str,
    body: &str,
    headers: &BTreeMap<String, String>,
) -> JsonResponse {
    if let Ok(expected) = std::env::var("MTG_ENGINE_API_KEY") {
        if !expected.is_empty()
            && headers.get("x-mtg-api-key").map(String::as_str) != Some(expected.as_str())
            && path != "/health"
            && method != "OPTIONS"
        {
            return error_response(401, "A valid MTG engine API key is required".to_string());
        }
    }
    if let Some(response) = route_local_app(method, path, body) {
        return match response {
            Ok(response) => json_response(response.status, &response.body),
            Err(error) => error_response(500, error),
        };
    }
    if method == "GET" && path == "/game/formats" {
        return json_response(200, &game_format_catalog());
    }
    if let Some(response) = route_network_lobby(method, path, body, headers) {
        return response;
    }
    if let Some(response) = route_game_session(method, path, body, headers) {
        return response;
    }
    match (method, path) {
        ("OPTIONS", _) => json_response(200, &serde_json::json!({ "ok": true })),
        ("GET", "/health") => match game_session_manager().history_status() {
            Ok(history) => {
                let mut response = serde_json::json!({ "ok": true });
                if let Some(history) = history {
                    response["history"] = serde_json::json!(history);
                }
                json_response(200, &response)
            }
            Err(error) => error_response(503, error),
        },
        ("GET", "/cards/sets") => match card_sets() {
            Ok(catalog) => json_response(200, &catalog),
            Err(error) => error_response(500, error),
        },
        ("POST", "/cards/lookup") => match serde_json::from_str::<CardLookupRequest>(body) {
            Ok(request) => match lookup_cards(request) {
                Ok(catalog) => json_response(200, &catalog),
                Err(error) => error_response(400, error),
            },
            Err(error) => error_response(400, format!("Invalid card lookup: {error}")),
        },
        ("GET", "/ai/controllers") => json_response(200, &controller_catalog()),
        ("GET", "/ai/agents") => json_response(
            200,
            &serde_json::json!({
                "schemaVersion": "agent-catalog/v1",
                "agents": registered_agent_manifests(),
            }),
        ),
        ("POST", "/game/decks/legal") => legal_deck_catalog(body),
        ("POST", "/game/matchmaking") => play_matchmaking(body),
        ("POST", "/game/setups/validate") => validate_game_setup(body),
        ("GET", training_path)
            if training_path == "/ai/training" || training_path.starts_with("/ai/training/") =>
        {
            match training_dashboard(
                training_path
                    .strip_prefix("/ai/training/")
                    .filter(|id| !id.is_empty()),
            ) {
                Ok(status) => json_response(200, &status),
                Err(error) => error_response(500, error.to_string()),
            }
        }
        ("POST", "/ai/training/control") => {
            match serde_json::from_str::<Value>(body)
                .ok()
                .and_then(|payload| {
                    payload.get("action").and_then(Value::as_str).map(|action| {
                        (
                            payload
                                .get("modelId")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            action.to_string(),
                        )
                    })
                }) {
                Some((model_id, action)) => match control_training(model_id.as_deref(), &action) {
                    Ok(status) => json_response(200, &status),
                    Err(error) => error_response(409, error.to_string()),
                },
                None => error_response(400, "Invalid AI training control request"),
            }
        }
        ("GET", "/analytics/decks") => json_response(
            200,
            &deck_analytics_service().query(DeckAnalyticsQuery::default()),
        ),
        ("POST", "/analytics/decks/query") => {
            match serde_json::from_str::<DeckAnalyticsQuery>(body) {
                Ok(query) => json_response(200, &deck_analytics_service().query(query)),
                Err(error) => error_response(400, format!("Invalid deck analytics query: {error}")),
            }
        }
        ("POST", "/oracle/parse") => match serde_json::from_str::<OracleCardParseRequest>(body) {
            Ok(request) => json_response(200, &parse_oracle_card(request)),
            Err(error) => error_response(400, format!("Invalid Oracle parse request: {}", error)),
        },
        ("POST", "/oracle/rules") => match serde_json::from_str::<OracleCardParseRequest>(body) {
            Ok(request) => json_response(200, &compact_oracle_rules(request)),
            Err(error) => error_response(400, format!("Invalid Oracle rules request: {}", error)),
        },
        ("POST", "/game/decision-options") => match serde_json::from_str::<DecisionRequest>(body) {
            Ok(request) => json_response(200, &build_player_decision_options(request)),
            Err(error) => error_response(400, format!("Invalid decision request: {}", error)),
        },
        ("POST", "/game/simulate") => match serde_json::from_str::<RandomSimulationRequest>(body) {
            Ok(request) => match simulate_random_games(request) {
                Ok(summary) => json_response(200, &summary),
                Err(error) => error_response(400, format!("Game simulation failed: {}", error)),
            },
            Err(error) => {
                error_response(400, format!("Invalid game simulation request: {}", error))
            }
        },
        _ => error_response(404, "Not found"),
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        409 => "Conflict",
        502 => "Bad Gateway",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn find_ui_dist_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|candidate| {
        candidate
            .join("dist/index.html")
            .is_file()
            .then(|| candidate.join("dist"))
    })
}

fn ui_dist_path() -> Result<&'static PathBuf, String> {
    static UI_DIST: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    UI_DIST
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("MTG_UI_DIST_PATH") {
                let path = PathBuf::from(path);
                return path
                    .join("index.html")
                    .is_file()
                    .then_some(path)
                    .ok_or_else(|| "MTG_UI_DIST_PATH does not contain index.html".to_string());
            }
            if let Ok(current) = std::env::current_dir()
                && let Some(path) = find_ui_dist_from(&current)
            {
                return Ok(path);
            }
            find_ui_dist_from(Path::new(env!("CARGO_MANIFEST_DIR")))
                .ok_or_else(|| "could not locate dist/index.html; run npm run build".to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn is_api_path(path: &str) -> bool {
    path == "/health"
        || [
            "/api/",
            "/cards/",
            "/ai/",
            "/analytics/",
            "/oracle/",
            "/game/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

fn scryfall_image_path(path: &str) -> Option<&str> {
    let upstream_path = path.strip_prefix("/api/scryfall-images/")?;
    (!upstream_path.is_empty()
        && !upstream_path.contains("..")
        && upstream_path
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/-_.".contains(character)))
    .then_some(upstream_path)
}

fn scryfall_image_response(path: &str) -> Result<(Vec<u8>, String), String> {
    let upstream_path =
        scryfall_image_path(path).ok_or_else(|| "invalid Scryfall image path".to_string())?;
    let url = format!("https://cards.scryfall.io/{upstream_path}");
    let response = ureq::get(&url)
        .set("Accept", "image/*")
        .set("User-Agent", "mtg-oracle-engine/0.1.0")
        .call()
        .map_err(|error| format!("Scryfall image request failed: {error}"))?;
    let content_type = response
        .header("Content-Type")
        .unwrap_or("application/octet-stream")
        .to_string();
    if !content_type.to_ascii_lowercase().starts_with("image/") {
        return Err(format!(
            "Scryfall returned unsupported content type {content_type}"
        ));
    }
    let mut body = Vec::new();
    response
        .into_reader()
        .take(20 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|error| format!("could not read Scryfall image: {error}"))?;
    Ok((body, content_type))
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") | Some("map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ttf") => "font/ttf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn static_response(path: &str) -> Result<Option<(Vec<u8>, &'static str, bool)>, String> {
    let requested = path.trim_start_matches('/');
    if requested
        .split('/')
        .any(|segment| segment == "." || segment == ".." || segment.contains('\\'))
    {
        return Ok(None);
    }
    let dist = ui_dist_path()?;
    let candidate = if requested.is_empty() {
        dist.join("index.html")
    } else {
        dist.join(requested)
    };
    let path = if candidate.is_file() {
        candidate
    } else if !requested.starts_with("assets/") && !requested.contains('.') {
        dist.join("index.html")
    } else {
        return Ok(None);
    };
    let immutable = requested.starts_with("assets/");
    let mime = content_type(&path);
    fs::read(&path)
        .map(|body| Some((body, mime, immutable)))
        .map_err(|error| format!("could not read UI asset {}: {error}", path.display()))
}

struct HttpRequest {
    body: Vec<u8>,
    close_connection: bool,
    headers: BTreeMap<String, String>,
    method: String,
    path: String,
}

fn read_http_request(
    stream: &mut TcpStream,
    pending: &mut Vec<u8>,
) -> io::Result<Option<HttpRequest>> {
    let mut chunk = [0_u8; 4096];
    loop {
        let Some(header_index) = pending.windows(4).position(|window| window == b"\r\n\r\n") else {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                if pending.is_empty() {
                    return Ok(None);
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed during HTTP headers",
                ));
            }
            pending.extend_from_slice(&chunk[..read]);
            continue;
        };
        let header_end = header_index + 4;
        let headers = String::from_utf8_lossy(&pending[..header_end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(key, value)| {
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        let request_end = header_end.saturating_add(content_length);
        while pending.len() < request_end {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed during HTTP body",
                ));
            }
            pending.extend_from_slice(&chunk[..read]);
        }
        let request_headers = headers
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect::<BTreeMap<_, _>>();
        let mut lines = headers.lines();
        let request_line = lines.next().unwrap_or_default();
        let parts = request_line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP request line",
            ));
        }
        let close_connection = parts[2].eq_ignore_ascii_case("HTTP/1.0")
            || lines.any(|line| {
                line.split_once(':').is_some_and(|(key, value)| {
                    key.eq_ignore_ascii_case("connection")
                        && value.trim().eq_ignore_ascii_case("close")
                })
            });
        let body = pending[header_end..request_end].to_vec();
        pending.drain(..request_end);
        return Ok(Some(HttpRequest {
            body,
            close_connection,
            headers: request_headers,
            method: parts[0].to_string(),
            path: parts[1].split('?').next().unwrap_or(parts[1]).to_string(),
        }));
    }
}

fn handle_stream(mut stream: TcpStream) -> std::io::Result<()> {
    let mut preview = [0_u8; 2048];
    let preview_length = stream.peek(&mut preview)?;
    let preview = String::from_utf8_lossy(&preview[..preview_length]);
    if preview.starts_with("GET /ai/agents/ws ") {
        return handle_agent_websocket(stream)
            .map_err(|error| io::Error::new(io::ErrorKind::ConnectionAborted, error));
    }
    let mut pending = Vec::new();
    while let Some(request) = read_http_request(&mut stream, &mut pending)? {
        let body = String::from_utf8_lossy(&request.body);
        let (status, content_type, response_body, cache_control) = if request.method == "GET"
            && scryfall_image_path(&request.path).is_some()
        {
            match scryfall_image_response(&request.path) {
                Ok((body, content_type)) => (200, content_type, body, "public, max-age=86400"),
                Err(error) => {
                    let response = error_response(502, error);
                    (
                        response.status,
                        "application/json".to_string(),
                        response.body.into_bytes(),
                        "no-store",
                    )
                }
            }
        } else if request.method == "GET" && !is_api_path(&request.path) {
            match static_response(&request.path) {
                Ok(Some((body, content_type, immutable))) => (
                    200,
                    content_type.to_string(),
                    body,
                    if immutable {
                        "public, max-age=31536000, immutable"
                    } else {
                        "no-cache"
                    },
                ),
                Ok(None) => {
                    let response = route_json_with_headers(
                        &request.method,
                        &request.path,
                        &body,
                        &request.headers,
                    );
                    (
                        response.status,
                        "application/json".to_string(),
                        response.body.into_bytes(),
                        "no-store",
                    )
                }
                Err(error) => {
                    let response = error_response(500, error);
                    (
                        response.status,
                        "application/json".to_string(),
                        response.body.into_bytes(),
                        "no-store",
                    )
                }
            }
        } else {
            let response =
                route_json_with_headers(&request.method, &request.path, &body, &request.headers);
            (
                response.status,
                "application/json".to_string(),
                response.body.into_bytes(),
                "no-store",
            )
        };
        let connection = if request.close_connection {
            "close"
        } else {
            "keep-alive"
        };
        let headers = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: DELETE, GET, POST, PUT, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, X-MTG-Player-ID, X-MTG-Seat-Token\r\nCache-Control: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
            status,
            reason_phrase(status),
            content_type,
            cache_control,
            response_body.len(),
            connection,
        );
        stream.write_all(headers.as_bytes())?;
        stream.write_all(&response_body)?;
        stream.flush()?;
        if request.close_connection {
            break;
        }
    }
    Ok(())
}

pub fn serve(addr: &str) -> std::io::Result<()> {
    ensure_network_lobby_cleanup();
    let listener = TcpListener::bind(addr)?;
    println!("mtg-engine listening on http://{}", addr);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = thread::Builder::new()
                    .name("mtg-http-connection".to_string())
                    .stack_size(HTTP_CONNECTION_STACK_SIZE)
                    .spawn(move || {
                        if let Err(error) = handle_stream(stream) {
                            eprintln!("request failed: {}", error);
                        }
                    })
                {
                    eprintln!("connection worker failed to start: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {}", error),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_http_worker_stack_parses_large_oracle_requests() {
        let result = thread::Builder::new()
            .stack_size(HTTP_CONNECTION_STACK_SIZE)
            .spawn(|| {
                parse_oracle_card(OracleCardParseRequest {
                    card_name: "Green Sun's Twilight".to_string(),
                    type_line: "Sorcery".to_string(),
                    mana_cost: Some("{X}{G}".to_string()),
                    oracle_text: Some(
                        "Reveal the top X plus one cards of your library. Choose a creature card and/or a land card from among them. Put those cards into your hand and the rest on the bottom of your library in a random order. If X is 5 or more, instead put the chosen cards onto the battlefield or into your hand and the rest on the bottom of your library in a random order."
                            .to_string(),
                    ),
                    layout: Some("normal".to_string()),
                    faces: Vec::new(),
                })
            })
            .expect("HTTP parser worker starts with the configured stack")
            .join()
            .expect("HTTP parser worker does not overflow its stack");

        assert_eq!(result.abilities.len(), 1);
        assert!(
            result.abilities[0]
                .source
                .text
                .starts_with("Reveal the top X plus one cards")
        );
    }

    fn matchmaking_candidate(deck_id: &str, rating: f64, matches: u64) -> MatchmakingCandidate {
        MatchmakingCandidate {
            controller_id: "ai-v12-in-training".to_string(),
            controller_label: "AlphaStar V12".to_string(),
            deck: LegalDeckOption {
                deck_session_id: deck_id.to_string(),
                deck_name: deck_id.to_string(),
            },
            matches,
            pilot_id: "ia-v12-in-training".to_string(),
            rating,
            rating_source: "ai-deck",
        }
    }

    #[test]
    fn matchmaking_randomness_is_deterministic_for_a_seed() {
        let candidates = vec![
            matchmaking_candidate("weak", 3.0, 100),
            matchmaking_candidate("close", 8.0, 20),
            matchmaking_candidate("strong", 20.0, 100),
        ];
        let mut first = candidates.clone();
        let mut second = candidates;

        let first_pick = choose_matchmaking_candidate(9.0, &mut first, 42).unwrap();
        let second_pick = choose_matchmaking_candidate(9.0, &mut second, 42).unwrap();

        assert_eq!(
            first_pick.deck.deck_session_id,
            second_pick.deck.deck_session_id
        );
    }

    #[test]
    fn matchmaking_favors_nearby_ratings_without_becoming_deterministic() {
        let candidates = vec![
            matchmaking_candidate("far", 20.0, 20),
            matchmaking_candidate("near", 8.0, 20),
        ];
        let mut near_picks = 0;
        let mut far_picks = 0;
        for seed in 0..512 {
            let mut pool = candidates.clone();
            match choose_matchmaking_candidate(9.0, &mut pool, seed)
                .unwrap()
                .deck
                .deck_session_id
                .as_str()
            {
                "near" => near_picks += 1,
                "far" => far_picks += 1,
                _ => unreachable!(),
            }
        }

        assert!(near_picks > far_picks * 10);
        assert!(far_picks > 0);
    }

    #[test]
    fn commander_player_counts_are_enforced_at_the_http_boundary() {
        assert!(validate_game_mode_player_count(GameMode::Commander, 4).is_ok());
        assert!(validate_game_mode_player_count(GameMode::Commander, 2).is_err());
        assert!(validate_game_mode_player_count(GameMode::DuelCommander, 2).is_ok());
        assert!(validate_game_mode_player_count(GameMode::DuelCommander, 4).is_err());
        assert!(validate_game_mode_player_count(GameMode::Legacy, 2).is_ok());
    }

    fn private_card(owner: &str, id: &str, name: &str) -> crate::engine::CardInstance {
        crate::engine::CardInstance {
            instance_id: id.to_string(),
            definition: crate::engine::CardDefinition {
                id: name.to_ascii_lowercase(),
                name: name.to_string(),
                type_line: "Instant".to_string(),
                is_commander: false,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                mana_cost: "{U}".to_string(),
                power: None,
                toughness: None,
                rules: vec![json!({ "kind": "spellAbility" })],
            },
            printed_definition: None,
            owner: owner.to_string(),
            controller: owner.to_string(),
            tapped: false,
            summoning_sick: false,
            damage_marked: 0,
            power_modifier: 0,
            toughness_modifier: 0,
            counters: BTreeMap::new(),
            flags: BTreeMap::new(),
            battle_protector: None,
            attached_to: None,
        }
    }

    fn private_player(id: &str, hand_card: &str) -> crate::engine::EnginePlayer {
        crate::engine::EnginePlayer {
            id: id.to_string(),
            name: id.to_string(),
            life: 20,
            has_lost: false,
            library: vec![private_card(id, &format!("{id}:library:0"), "Brainstorm")],
            hand: vec![private_card(id, &format!("{id}:hand:0"), hand_card)],
            battlefield: Vec::new(),
            graveyard: Vec::new(),
            exile: Vec::new(),
            sideboard: vec![private_card(id, &format!("{id}:sideboard:0"), "Negate")],
            command_zone: Vec::new(),
            commander_damage: Vec::new(),
            mana_pool: Vec::new(),
            counters: BTreeMap::new(),
            land_plays_remaining: 1,
            max_hand_size: 7,
        }
    }

    #[test]
    fn network_projection_keeps_only_the_viewers_private_cards_and_decision() {
        let mut view = GameSessionView {
            schema_version: "mtg-game-session/v1".to_string(),
            session_id: "network-test".to_string(),
            revision: 1,
            state: crate::engine::GameState {
                schema_version: "mtg-game/v1".to_string(),
                game_mode: GameMode::Free,
                status: crate::engine::GameStatus::InProgress,
                turn_number: 1,
                active_player: 0,
                priority_player: Some(0),
                step: crate::engine::GameStep::PrecombatMain,
                players: vec![
                    private_player("player-1", "Opt"),
                    private_player("player-2", "Shock"),
                ],
                game_pieces: Vec::new(),
                commanders: Vec::new(),
                stack: Vec::new(),
                combat: Default::default(),
                permissions: Vec::new(),
                rule_modifiers: Vec::new(),
                events: Vec::new(),
                unsupported_rules: Vec::new(),
                outcome: None,
            },
            calculated_stats: BTreeMap::new(),
            decision: Some(crate::engine::EngineDecisionRequest {
                id: "priority:1".to_string(),
                kind: crate::engine::DecisionKind::Priority,
                player_id: "player-1".to_string(),
                source_card: None,
                source_card_instance_id: None,
                choice: None,
                options: Vec::new(),
            }),
            error: None,
            match_state: None,
        };

        let player_one = project_session_view(view.clone(), "player-1");
        assert_eq!(player_one.state.players[0].hand[0].definition.name, "Opt");
        assert_eq!(
            player_one.state.players[1].hand[0].definition.name,
            "Hidden card"
        );
        assert_eq!(
            player_one.state.players[0].library[0].definition.name,
            "Hidden card"
        );
        assert_eq!(
            player_one.state.players[1].sideboard[0].definition.name,
            "Hidden card"
        );
        assert!(player_one.decision.is_some());

        let player_two = project_session_view(view.clone(), "player-2");
        assert_eq!(player_two.state.players[1].hand[0].definition.name, "Shock");
        assert_eq!(
            player_two.state.players[0].hand[0].definition.name,
            "Hidden card"
        );
        assert!(player_two.decision.is_none());

        view.state.rule_modifiers.push(json!({
            "kind": "revealedHand",
            "playerId": "player-2",
            "expiresAfterTurn": 1,
            "sourceCardInstanceId": "targeted-hand-source",
        }));
        let temporarily_revealed = project_session_view(view.clone(), "player-1");
        assert_eq!(
            temporarily_revealed.state.players[1].hand[0]
                .definition
                .name,
            "Shock"
        );
        view.state.rule_modifiers.clear();

        let mut telepathy = private_card("player-1", "telepathy", "Telepathy");
        telepathy.definition.type_line = "Enchantment".to_string();
        telepathy.definition.rules = vec![json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": { "kind": "battlefield" },
            "modifiers": [{
                "kind": "revealHands",
                "players": {
                    "kind": "opponentsOf",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                },
            }],
        })];
        view.state.players[0].battlefield.push(telepathy);
        let revealed = project_session_view(view, "player-1");
        assert_eq!(revealed.state.players[1].hand[0].definition.name, "Shock");
    }

    #[test]
    fn network_seat_tokens_are_required_and_unclaimed_seats_cannot_read() {
        let session_id = format!("auth-test-{}", random_access_value(12));
        let access = register_network_game(
            &session_id,
            "player-1",
            &["player-1".to_string(), "player-2".to_string()],
            BTreeMap::new(),
            Some("Host"),
        )
        .expect("network game registration");
        let missing = authenticated_network_access(&session_id, &BTreeMap::new())
            .expect_err("missing token must fail");
        assert_eq!(missing.status, 401);

        let host_headers = BTreeMap::from([
            ("x-mtg-player-id".to_string(), access.player_id.clone()),
            ("x-mtg-seat-token".to_string(), access.seat_token.clone()),
        ]);
        let authenticated = authenticated_network_access(&session_id, &host_headers)
            .expect("authentication query")
            .expect("network access");
        assert!(authenticated.is_host);

        let player_two_token = network_game_registry()
            .lock()
            .expect("network registry")
            .sessions
            .get(&session_id)
            .expect("network game")
            .seat_tokens["player-2"]
            .clone();
        let player_two_headers = BTreeMap::from([
            ("x-mtg-player-id".to_string(), "player-2".to_string()),
            ("x-mtg-seat-token".to_string(), player_two_token),
        ]);
        let unclaimed = authenticated_network_access(&session_id, &player_two_headers)
            .expect_err("unclaimed seat must fail");
        assert_eq!(unclaimed.status, 401);
        network_game_registry()
            .lock()
            .expect("network registry")
            .sessions
            .get_mut(&session_id)
            .expect("network game")
            .claimed_player_ids
            .insert("player-2".to_string());
        assert!(
            authenticated_network_access(&session_id, &player_two_headers)
                .expect("claimed seat authentication")
                .is_some()
        );
        leave_network_game(&session_id, "player-2").expect("guest leaves network game");
        assert_eq!(
            authenticated_network_access(&session_id, &player_two_headers)
                .expect_err("left seat must no longer authenticate")
                .status,
            401
        );
        unregister_network_game(&session_id);
    }

    #[test]
    fn one_human_seat_can_secure_a_game_against_ai_players() {
        let session_id = format!("human-ai-test-{}", random_access_value(12));
        let access = register_network_game(
            &session_id,
            "player-1",
            &["player-1".to_string()],
            BTreeMap::new(),
            Some("Friend"),
        )
        .expect("one human seat is a valid private network game");
        let headers = BTreeMap::from([
            ("x-mtg-player-id".to_string(), access.player_id.clone()),
            ("x-mtg-seat-token".to_string(), access.seat_token.clone()),
        ]);
        let authenticated = authenticated_network_access(&session_id, &headers)
            .expect("seat authentication")
            .expect("network access");
        assert_eq!(authenticated.human_player_ids, ["player-1"]);
        unregister_network_game(&session_id);
    }

    fn read_response(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let mut response = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let read = stream.read(&mut chunk).expect("response bytes");
            assert!(read > 0, "response closed before headers");
            response.extend_from_slice(&chunk[..read]);
            if let Some(index) = response.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers =
            String::from_utf8(response[..header_end].to_vec()).expect("response headers are UTF-8");
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(key, value)| {
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .expect("response has content length");
        while response.len() < header_end + content_length {
            let read = stream.read(&mut chunk).expect("response body bytes");
            assert!(read > 0, "response closed before body");
            response.extend_from_slice(&chunk[..read]);
        }
        (
            headers,
            response[header_end..header_end + content_length].to_vec(),
        )
    }

    #[test]
    fn http_connection_serves_multiple_requests_before_explicit_close() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener");
        let address = listener.local_addr().expect("test listener address");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("test connection");
            handle_stream(stream).expect("connection serves requests");
        });
        let mut client = TcpStream::connect(address).expect("connect to test server");

        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .expect("first request");
        let (first_headers, first_body) = read_response(&mut client);
        assert!(first_headers.contains("Connection: keep-alive"));
        assert_eq!(
            serde_json::from_slice::<Value>(&first_body).unwrap(),
            json!({ "ok": true })
        );

        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("second request");
        let (second_headers, second_body) = read_response(&mut client);
        assert!(second_headers.contains("Connection: close"));
        assert_eq!(
            serde_json::from_slice::<Value>(&second_body).unwrap(),
            json!({ "ok": true })
        );
        server.join().expect("test server joins");
    }
}
