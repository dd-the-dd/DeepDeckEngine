use crate::engine::{
    CardDefinition, DecisionProvider, EngineDecisionRequest, EngineError, GameState,
    decision_number_bounds, validate_decision_card_instance_ids,
};
use crate::history_queue::HistoryStream;
use crate::observation_delta::merge_patch;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::net::TcpStream;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::{Message, WebSocket, accept_hdr};

pub const AGENT_PROTOCOL_VERSION: &str = "mtg-agent/v1";
pub const OBSERVATION_SCHEMA_VERSION: &str = "player-observation/v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationStream {
    FullObservationStream,
    DeltaEventStream,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeoutCategory {
    Realtime,
    Standard,
    Extended,
    Offline,
}

impl TimeoutCategory {
    pub fn decision_timeout(self) -> Duration {
        Duration::from_millis(match self {
            Self::Realtime => 500,
            Self::Standard => 5_000,
            Self::Extended => 30_000,
            Self::Offline => 300_000,
        })
    }

    pub fn starting_analysis_timeout(self) -> Duration {
        self.decision_timeout().saturating_mul(5)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameSharing {
    Private,
    ResultsOnly,
    PublicReplay,
    ResearchDataset,
    TrainingDataset,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRepository {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScientificPublication {
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arxiv: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "selection", rename_all = "kebab-case")]
pub enum DeckSelection {
    All,
    AllowList {
        #[serde(rename = "deckIds")]
        deck_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCompatibility {
    pub game_modes: Vec<String>,
    pub decks: DeckSelection,
    pub time_controls: Vec<TimeoutCategory>,
    pub observation_streams: Vec<ObservationStream>,
    pub game_sharing: Vec<GameSharing>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default)]
    pub starting_situation_analysis: bool,
    #[serde(default)]
    pub decision_exploration: bool,
    #[serde(default)]
    pub stateful_memory: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentManifest {
    pub schema_version: String,
    pub agent_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub authors: Vec<AgentAuthor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<AgentRepository>,
    #[serde(default)]
    pub publications: Vec<ScientificPublication>,
    pub compatibility: AgentCompatibility,
    #[serde(default)]
    pub capabilities: AgentCapabilities,
}

impl AgentManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != "agent-manifest/v1" {
            return Err("unsupported agent manifest schema".to_string());
        }
        if self.agent_id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.version.trim().is_empty()
        {
            return Err("agentId, name and version are required".to_string());
        }
        if self.authors.is_empty()
            || self
                .authors
                .iter()
                .any(|author| author.name.trim().is_empty())
        {
            return Err("at least one named author is required".to_string());
        }
        if self.compatibility.game_modes.is_empty()
            || self.compatibility.time_controls.is_empty()
            || self.compatibility.observation_streams.is_empty()
            || self.compatibility.game_sharing.is_empty()
        {
            return Err(
                "game modes, time controls, observation streams and sharing modes cannot be empty"
                    .to_string(),
            );
        }
        if matches!(&self.compatibility.decks, DeckSelection::AllowList { deck_ids } if deck_ids.is_empty())
        {
            return Err("an allow-list requires at least one deckId".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterAgentRequest {
    #[serde(rename = "type")]
    message_type: String,
    protocol_version: String,
    manifest: AgentManifest,
    observation_stream: ObservationStream,
    timeout_category: TimeoutCategory,
    game_sharing: GameSharing,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentReply {
    #[serde(rename = "type")]
    message_type: String,
    request_id: String,
    #[serde(default)]
    action_id: Option<String>,
    #[serde(default)]
    number_value: Option<i32>,
    #[serde(default)]
    card_instance_ids: Option<Vec<String>>,
}

struct AgentConnection {
    manifest: AgentManifest,
    observation_stream: ObservationStream,
    timeout_category: TimeoutCategory,
    game_sharing: GameSharing,
    outgoing: mpsc::Sender<Value>,
    pending: Mutex<HashMap<String, mpsc::Sender<AgentReply>>>,
}

impl AgentConnection {
    fn request(
        &self,
        message: Value,
        request_id: &str,
        timeout: Duration,
    ) -> Result<AgentReply, EngineError> {
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| EngineError::new("agent response registry is poisoned"))?
            .insert(request_id.to_string(), sender);
        if self.outgoing.send(message).is_err() {
            let _ = self
                .pending
                .lock()
                .map(|mut pending| pending.remove(request_id));
            return Err(EngineError::new("agent websocket is disconnected"));
        }
        match receiver.recv_timeout(timeout) {
            Ok(reply) => Ok(reply),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self
                    .pending
                    .lock()
                    .map(|mut pending| pending.remove(request_id));
                Err(EngineError::new(format!(
                    "agent timed out after {} ms",
                    timeout.as_millis()
                )))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(EngineError::new("agent websocket response channel closed"))
            }
        }
    }

    fn deliver(&self, reply: AgentReply) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some(sender) = pending.remove(&reply.request_id)
        {
            let _ = sender.send(reply);
        }
    }
}

fn registry() -> &'static Mutex<BTreeMap<String, Arc<AgentConnection>>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<String, Arc<AgentConnection>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn unregister_connection(agent_id: &str, connection: &Arc<AgentConnection>) {
    let Ok(mut agents) = registry().lock() else {
        return;
    };
    if agents
        .get(agent_id)
        .is_some_and(|registered| Arc::ptr_eq(registered, connection))
    {
        agents.remove(agent_id);
    }
}

pub fn is_registered_agent_controller(controller_id: &str) -> bool {
    let Some(agent_id) = controller_id.strip_prefix("agent:") else {
        return false;
    };
    registry()
        .lock()
        .is_ok_and(|agents| agents.contains_key(agent_id))
}

pub fn validate_registered_agent_assignment(
    controller_id: &str,
    game_mode: &str,
    deck_id: Option<&str>,
) -> Result<(), String> {
    let connection = registered_connection(controller_id).map_err(|error| error.to_string())?;
    if !connection
        .manifest
        .compatibility
        .game_modes
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(game_mode))
    {
        return Err(format!("agent does not support game mode {game_mode}"));
    }
    if let (DeckSelection::AllowList { deck_ids }, Some(deck_id)) =
        (&connection.manifest.compatibility.decks, deck_id)
        && !deck_ids.iter().any(|allowed| allowed == deck_id)
    {
        return Err(format!("agent does not support deck {deck_id}"));
    }
    Ok(())
}

pub fn registered_agent_manifests() -> Vec<AgentManifest> {
    registry()
        .lock()
        .map(|agents| {
            agents
                .values()
                .map(|agent| agent.manifest.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn registered_connection(controller_id: &str) -> Result<Arc<AgentConnection>, EngineError> {
    let agent_id = controller_id
        .strip_prefix("agent:")
        .ok_or_else(|| EngineError::new("external agent controller must start with agent:"))?;
    registry()
        .lock()
        .map_err(|_| EngineError::new("agent registry is poisoned"))?
        .get(agent_id)
        .cloned()
        .ok_or_else(|| EngineError::new(format!("agent {agent_id} is not connected")))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub struct WebSocketAgent {
    controller_id: String,
    context_id: String,
    player_id: String,
    known_deck: Vec<CardDefinition>,
    history: Option<HistoryStream>,
    previous_observation: Option<Value>,
    starting_analysis_sent: bool,
    sequence: u64,
}

impl WebSocketAgent {
    pub fn new(
        controller_id: impl Into<String>,
        context_id: impl Into<String>,
        player_id: impl Into<String>,
        known_deck: Vec<CardDefinition>,
        history: Option<HistoryStream>,
    ) -> Result<Self, EngineError> {
        let controller_id = controller_id.into();
        registered_connection(&controller_id)?;
        Ok(Self {
            controller_id,
            context_id: context_id.into(),
            player_id: player_id.into(),
            known_deck,
            history,
            previous_observation: None,
            starting_analysis_sent: false,
            sequence: 0,
        })
    }

    fn connection(&self) -> Result<Arc<AgentConnection>, EngineError> {
        registered_connection(&self.controller_id)
    }

    fn observation_update(&mut self, connection: &AgentConnection, observation: &Value) -> Value {
        self.sequence += 1;
        let update = match (
            connection.observation_stream,
            self.previous_observation.as_ref(),
        ) {
            (ObservationStream::DeltaEventStream, Some(previous)) => json!({
                "kind": "observationDelta",
                "sequence": self.sequence,
                "previousSequence": self.sequence - 1,
                "patch": merge_patch(previous, observation),
            }),
            _ => json!({
                "kind": "fullObservation",
                "sequence": self.sequence,
                "observation": observation,
            }),
        };
        self.previous_observation = Some(observation.clone());
        update
    }

    fn prepare(&mut self, state: &GameState) -> Result<(Arc<AgentConnection>, Value), EngineError> {
        let connection = self.connection()?;
        let observation = serde_json::to_value(state).map_err(|error| {
            EngineError::new(format!("could not serialize player observation: {error}"))
        })?;
        if !self.starting_analysis_sent {
            self.starting_analysis_sent = true;
            let timeout = connection.timeout_category.starting_analysis_timeout();
            let request_id = format!("{}:starting", self.context_id);
            let started = Instant::now();
            let result = connection.request(
                json!({
                    "type": "startingSituationRequested",
                    "schemaVersion": AGENT_PROTOCOL_VERSION,
                    "requestId": request_id,
                    "contextId": self.context_id,
                    "deadlineUnixMs": now_millis() + timeout.as_millis(),
                    "targetDurationMs": connection.timeout_category.decision_timeout().as_millis(),
                    "analysisDurationMs": timeout.as_millis(),
                    "observation": observation,
                    "knownDeck": self.known_deck,
                }),
                &request_id,
                timeout,
            );
            if let Some(history) = &self.history {
                history.publish_player_event(
                    "agent.setup.completed",
                    &self.player_id,
                    json!({
                        "responseTimeMs": started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                        "succeeded": result.is_ok(),
                    }),
                );
            }
        }
        Ok((connection, observation))
    }

    fn decide(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<AgentReply, EngineError> {
        let agent_request = request.agent_facing();
        if agent_request.options.is_empty() {
            return Err(EngineError::new(format!(
                "decision {} contains only UI actions",
                request.id
            )));
        }
        let (connection, observation) = self.prepare(state)?;
        let update = self.observation_update(&connection, &observation);
        let timeout = connection.timeout_category.decision_timeout();
        connection.request(
            json!({
                "type": "decisionRequested",
                "schemaVersion": AGENT_PROTOCOL_VERSION,
                "observationSchemaVersion": OBSERVATION_SCHEMA_VERSION,
                "requestId": request.id,
                "contextId": self.context_id,
                "playerId": request.player_id,
                "deadlineUnixMs": now_millis() + timeout.as_millis(),
                "observationUpdate": update,
                "decision": agent_request,
            }),
            &request.id,
            timeout,
        )
    }
}

impl DecisionProvider for WebSocketAgent {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let reply = self.decide(state, request)?;
        if reply.message_type != "decisionSubmitted" {
            return Err(EngineError::new(
                "agent returned an unexpected response type",
            ));
        }
        let action_id = reply
            .action_id
            .ok_or_else(|| EngineError::new("agent omitted actionId"))?;
        request
            .options
            .iter()
            .position(|action| action.id == action_id)
            .ok_or_else(|| EngineError::new(format!("agent returned illegal action {action_id}")))
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let reply = self.decide(state, request)?;
        let selected = reply
            .number_value
            .ok_or_else(|| EngineError::new("agent omitted numberValue"))?;
        let (minimum, maximum) = decision_number_bounds(request)
            .ok_or_else(|| EngineError::new("decision is not numeric"))?;
        if !(minimum..=maximum).contains(&selected) {
            return Err(EngineError::new(format!(
                "agent number {selected} is outside {minimum}..={maximum}"
            )));
        }
        Ok(selected)
    }

    fn choose_card_instance_ids(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        let reply = self.decide(state, request)?;
        if reply.message_type != "decisionSubmitted" {
            return Err(EngineError::new(
                "agent returned an unexpected response type",
            ));
        }
        let selected = reply
            .card_instance_ids
            .ok_or_else(|| EngineError::new("agent omitted cardInstanceIds"))?;
        validate_decision_card_instance_ids(request, &selected)?;
        Ok(selected)
    }
}

fn write_json(socket: &mut WebSocket<TcpStream>, value: &Value) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .map_err(|error| error.to_string())
}

fn secrets_match(expected: &[u8], actual: &[u8]) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .iter()
        .zip(actual)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn handle_agent_websocket_with_key(
    stream: TcpStream,
    expected_api_key: Option<&str>,
) -> Result<(), String> {
    let expected_api_key = expected_api_key.map(str::as_bytes).map(Vec::from);
    let mut socket = accept_hdr(
        stream,
        move |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
            let authenticated = expected_api_key.as_deref().is_none_or(|expected| {
                request
                    .headers()
                    .get("x-mtg-api-key")
                    .is_some_and(|actual| secrets_match(expected, actual.as_bytes()))
            });
            if authenticated {
                Ok(response)
            } else {
                let mut error = ErrorResponse::new(Some("unauthorized agent connection".into()));
                *error.status_mut() = StatusCode::UNAUTHORIZED;
                Err(error)
            }
        },
    )
    .map_err(|error| error.to_string())?;
    let message = socket.read().map_err(|error| error.to_string())?;
    let text = message.into_text().map_err(|error| error.to_string())?;
    let register: RegisterAgentRequest = serde_json::from_str(&text)
        .map_err(|error| format!("invalid agent registration: {error}"))?;
    if register.message_type != "registerAgent"
        || register.protocol_version != AGENT_PROTOCOL_VERSION
    {
        return Err("first websocket message must register mtg-agent/v1".to_string());
    }
    register.manifest.validate()?;
    if !register
        .manifest
        .compatibility
        .observation_streams
        .contains(&register.observation_stream)
        || !register
            .manifest
            .compatibility
            .time_controls
            .contains(&register.timeout_category)
        || !register
            .manifest
            .compatibility
            .game_sharing
            .contains(&register.game_sharing)
    {
        return Err("registration choices are not declared by the manifest".to_string());
    }
    let agent_id = register.manifest.agent_id.clone();
    let (outgoing, incoming) = mpsc::channel();
    let connection = Arc::new(AgentConnection {
        manifest: register.manifest,
        observation_stream: register.observation_stream,
        timeout_category: register.timeout_category,
        game_sharing: register.game_sharing,
        outgoing,
        pending: Mutex::new(HashMap::new()),
    });
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|error| error.to_string())?;
    registry()
        .lock()
        .map_err(|_| "agent registry is poisoned".to_string())?
        .insert(agent_id.clone(), Arc::clone(&connection));
    write_json(
        &mut socket,
        &json!({
            "type": "registrationAccepted",
            "protocolVersion": AGENT_PROTOCOL_VERSION,
            "agentId": agent_id,
            "controllerId": format!("agent:{agent_id}"),
            "observationStream": connection.observation_stream,
            "timeoutCategory": connection.timeout_category,
            "decisionTimeoutMs": connection.timeout_category.decision_timeout().as_millis(),
            "startingAnalysisTimeoutMs": connection.timeout_category.starting_analysis_timeout().as_millis(),
            "gameSharing": connection.game_sharing,
        }),
    )?;

    loop {
        while let Ok(outgoing) = incoming.try_recv() {
            if let Err(error) = write_json(&mut socket, &outgoing) {
                unregister_connection(&agent_id, &connection);
                return Err(error);
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Ok(reply) = serde_json::from_str::<AgentReply>(&text) {
                    connection.deliver(reply);
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload));
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                unregister_connection(&agent_id, &connection);
                return Err(error.to_string());
            }
        }
    }
    unregister_connection(&agent_id, &connection);
    Ok(())
}

pub fn handle_agent_websocket(stream: TcpStream) -> Result<(), String> {
    let expected_api_key = env::var("MTG_ENGINE_API_KEY")
        .ok()
        .filter(|key| !key.is_empty());
    handle_agent_websocket_with_key(stream, expected_api_key.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::HeaderValue;

    fn manifest() -> AgentManifest {
        AgentManifest {
            schema_version: "agent-manifest/v1".to_string(),
            agent_id: "org.example.agent".to_string(),
            name: "Example".to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            authors: vec![AgentAuthor {
                name: "Author".to_string(),
                orcid: None,
                url: None,
            }],
            repository: None,
            publications: Vec::new(),
            compatibility: AgentCompatibility {
                game_modes: vec!["legacy".to_string()],
                decks: DeckSelection::All,
                time_controls: vec![TimeoutCategory::Standard],
                observation_streams: vec![ObservationStream::DeltaEventStream],
                game_sharing: vec![GameSharing::Private],
            },
            capabilities: AgentCapabilities::default(),
        }
    }

    #[test]
    fn stale_connection_cannot_unregister_its_replacement() {
        fn connection(agent_id: &str) -> Arc<AgentConnection> {
            let mut agent_manifest = manifest();
            agent_manifest.agent_id = agent_id.to_string();
            let (outgoing, _incoming) = mpsc::channel();
            Arc::new(AgentConnection {
                manifest: agent_manifest,
                observation_stream: ObservationStream::FullObservationStream,
                timeout_category: TimeoutCategory::Standard,
                game_sharing: GameSharing::PublicReplay,
                outgoing,
                pending: Mutex::new(HashMap::new()),
            })
        }

        let agent_id = "org.example.replacement";
        let old = connection(agent_id);
        let replacement = connection(agent_id);
        registry()
            .lock()
            .expect("registry lock")
            .insert(agent_id.to_string(), Arc::clone(&old));
        registry()
            .lock()
            .expect("registry lock")
            .insert(agent_id.to_string(), Arc::clone(&replacement));

        unregister_connection(agent_id, &old);

        let registered = registry()
            .lock()
            .expect("registry lock")
            .get(agent_id)
            .cloned()
            .expect("replacement remains registered");
        assert!(Arc::ptr_eq(&registered, &replacement));

        unregister_connection(agent_id, &replacement);
        assert!(!is_registered_agent_controller(&format!(
            "agent:{agent_id}"
        )));
    }

    #[test]
    fn manifest_requires_non_empty_compatibility_and_authors() {
        assert!(manifest().validate().is_ok());
        let mut invalid = manifest();
        invalid.authors.clear();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn agent_reply_deserializes_card_selection() {
        let reply: AgentReply = serde_json::from_value(json!({
            "type": "decisionSubmitted",
            "requestId": "choose-cards",
            "cardInstanceIds": ["card-a", "card-b"]
        }))
        .expect("card selection reply");

        assert_eq!(
            reply.card_instance_ids,
            Some(vec!["card-a".to_string(), "card-b".to_string()])
        );
    }

    #[test]
    fn starting_analysis_is_five_times_the_decision_target() {
        for category in [
            TimeoutCategory::Realtime,
            TimeoutCategory::Standard,
            TimeoutCategory::Extended,
            TimeoutCategory::Offline,
        ] {
            assert_eq!(
                category.starting_analysis_timeout(),
                category.decision_timeout() * 5
            );
        }
    }

    #[test]
    fn merge_patch_replaces_arrays_and_tracks_object_fields() {
        let before = json!({"life": 20, "hand": [1], "nested": {"a": 1, "gone": true}});
        let after = json!({"life": 18, "hand": [1, 2], "nested": {"a": 1, "new": "x"}});
        assert_eq!(
            merge_patch(&before, &after),
            json!({
                "life": 18,
                "hand": [1, 2],
                "nested": {"gone": null, "new": "x"}
            })
        );
    }

    #[test]
    fn websocket_registration_negotiates_manifest_stream_and_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test websocket listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("agent connection");
            handle_agent_websocket(stream).expect("agent websocket protocol");
        });
        let (mut socket, _) =
            tungstenite::connect(format!("ws://{address}/ai/agents/ws")).expect("websocket client");
        socket
            .send(Message::Text(
                json!({
                    "type": "registerAgent",
                    "protocolVersion": AGENT_PROTOCOL_VERSION,
                    "manifest": manifest(),
                    "observationStream": "delta-event-stream",
                    "timeoutCategory": "standard",
                    "gameSharing": "private"
                })
                .to_string()
                .into(),
            ))
            .expect("registration request");
        let accepted: Value = serde_json::from_str(
            &socket
                .read()
                .expect("registration response")
                .into_text()
                .expect("text response"),
        )
        .expect("registration json");
        assert_eq!(accepted["type"], "registrationAccepted");
        assert_eq!(accepted["controllerId"], "agent:org.example.agent");
        assert_eq!(accepted["decisionTimeoutMs"], 5_000);
        assert_eq!(accepted["startingAnalysisTimeoutMs"], 25_000);
        assert!(is_registered_agent_controller("agent:org.example.agent"));
        socket.close(None).expect("close websocket");
        server.join().expect("server thread");
    }

    #[test]
    fn websocket_registration_requires_configured_api_key() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test websocket listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (missing_key, _) = listener.accept().expect("missing-key connection");
            assert!(handle_agent_websocket_with_key(missing_key, Some("secret")).is_err());
            let (valid_key, _) = listener.accept().expect("valid-key connection");
            assert!(handle_agent_websocket_with_key(valid_key, Some("secret")).is_err());
        });

        let missing = tungstenite::connect(format!("ws://{address}/ai/agents/ws"));
        assert!(missing.is_err());

        let mut request = format!("ws://{address}/ai/agents/ws")
            .into_client_request()
            .expect("websocket request");
        request
            .headers_mut()
            .insert("x-mtg-api-key", HeaderValue::from_static("secret"));
        let (mut socket, response) = tungstenite::connect(request).expect("authorized websocket");
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        socket.close(None).expect("close websocket");
        server.join().expect("server thread");
    }
}
