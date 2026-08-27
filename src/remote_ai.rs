use crate::engine::{
    DecisionProvider, EngineDecisionRequest, EngineError, GameState, decision_number_bounds,
};
use crate::pilot_catalog::{PilotCapabilities, PilotDefinition, pilot_definition, training_pilots};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const DECISION_SCHEMA_VERSION: &str = "ai-decision/v1";
const CONTROLLER_SCHEMA_VERSION: &str = "ai-controller-catalog/v1";
const GROUND_TRUTH_URL_ENV: &str = "MTG_AI_GT_URL";
const TRAINING_URL_ENV: &str = "MTG_AI_TRAINING_URL";
const V9_TRAINING_URL_ENV: &str = "MTG_AI_V9_TRAINING_URL";
const V10_TRAINING_URL_ENV: &str = "MTG_AI_V10_TRAINING_URL";
const V11_TRAINING_URL_ENV: &str = "MTG_AI_V11_TRAINING_URL";
const V12_TRAINING_URL_ENV: &str = "MTG_AI_V12_TRAINING_URL";
const AUTOSTART_ENV: &str = "MTG_AI_AUTOSTART";
const PROJECT_ROOT_ENV: &str = "MTG_AI_PROJECT_ROOT";
const PYTHON_ENV: &str = "MTG_AI_PYTHON";
const INFERENCE_DEVICE_ENV: &str = "MTG_AI_INFERENCE_DEVICE";
const STARTUP_TIMEOUT_ENV: &str = "MTG_AI_STARTUP_TIMEOUT_MS";
const MODEL_REGISTRY_ENV: &str = "MTG_AI_MODEL_REGISTRY_PATH";
const V10_TRAINING_CHECKPOINT_ENV: &str = "MTG_AI_V10_TRAINING_CHECKPOINT";
const TRAINING_CONFIG_ENV: &str = "MTG_AI_TRAINING_CONFIG_PATH";
const TRAINING_RUN_ENV: &str = "MTG_AI_TRAINING_RUN_PATH";
const TRAINING_DASHBOARD_SCHEMA_VERSION: &str = "ai-training-dashboard/v1";

fn controller_label(model_id: &str, training_step: u64) -> String {
    if let Some(definition) = pilot_definition(model_id)
        && definition.kind == "model"
        && !definition.capabilities.training
    {
        return format!("{} (étape {training_step})", definition.label);
    }
    if model_id == "ia-in-training" {
        return format!("IA V8 simplifiée en entraînement (étape {training_step})");
    }
    if model_id == "ia-v9-in-training" {
        return format!("IA V9 en entraînement (étape {training_step})");
    }
    if model_id == "ia-v10-in-training" {
        return format!("IA V10 en entraînement (étape {training_step})");
    }
    if model_id == "ia-v11-in-training" {
        return format!("IA V11 AlphaStar en entraînement (étape {training_step})");
    }
    if model_id == "ia-v12-in-training" {
        return format!("IA V12 AlphaStar Legacy (étape {training_step})");
    }
    if model_id == "ia-gt-0" {
        return format!("IA V8 initiale (étape {training_step})");
    }
    format!("{model_id} (étape {training_step})")
}

#[derive(Clone, Debug)]
pub struct RemoteAi {
    controller_id: Option<String>,
    context_id: Option<String>,
    deterministic: bool,
    endpoint: String,
    timeout: Duration,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteDecisionRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    controller_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_id: Option<&'a str>,
    schema_version: &'static str,
    request_id: &'a str,
    player_id: &'a str,
    deterministic: bool,
    state: &'a GameState,
    decision: &'a EngineDecisionRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteDecisionResponse {
    schema_version: String,
    request_id: String,
    action_id: String,
    #[serde(default)]
    number_value: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteHealthResponse {
    #[serde(default)]
    checkpoint_path: Option<String>,
    model: String,
    #[serde(default)]
    registry_path: Option<String>,
    #[serde(default)]
    training_step: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteModelStatus {
    id: String,
    training_step: u64,
    #[serde(default)]
    available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteModelCatalog {
    schema_version: String,
    models: Vec<RemoteModelStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModelRegistry {
    current_ground_truth: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum LocalAiService {
    GroundTruth,
    V10Training,
}

#[derive(Default)]
struct LocalAiServiceManager {
    ground_truth: Option<Child>,
    v10_training: Option<Child>,
}

#[derive(Default)]
struct LocalTrainingProcessManager {
    trainers: BTreeMap<String, Child>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiControllerStatus {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub kind: String,
    pub capabilities: PilotCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_id: Option<String>,
    pub pilot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiControllerCatalog {
    pub schema_version: &'static str,
    pub controllers: Vec<AiControllerStatus>,
}

fn offline_pilot_status(id: &str) -> AiControllerStatus {
    let definition = pilot_definition(id).expect("known offline pilot definition");
    AiControllerStatus {
        id: definition.id.to_string(),
        label: definition.label.to_string(),
        available: false,
        kind: definition.kind.to_string(),
        capabilities: definition.capabilities,
        controller_id: definition.controller_id.map(str::to_string),
        pilot_id: definition.pilot_id.to_string(),
        model: None,
    }
}

impl RemoteAi {
    pub fn new(endpoint: impl Into<String>, timeout: Duration) -> Self {
        Self {
            controller_id: None,
            context_id: None,
            deterministic: true,
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            timeout,
        }
    }

    pub fn with_controller_id(mut self, controller_id: impl Into<String>) -> Self {
        self.controller_id = Some(controller_id.into());
        self
    }

    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }

    fn with_deterministic(mut self, deterministic: bool) -> Self {
        self.deterministic = deterministic;
        self
    }

    fn from_env(endpoint_variable: &str, default_endpoint: &str) -> Result<Self, EngineError> {
        let endpoint =
            std::env::var(endpoint_variable).unwrap_or_else(|_| default_endpoint.to_string());
        let timeout_ms = std::env::var("MTG_AI_TIMEOUT_MS")
            .or_else(|_| std::env::var("MTG_AI_V1_TIMEOUT_MS"))
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|error| EngineError::new(format!("invalid MTG_AI_TIMEOUT_MS: {error}")))?
            .unwrap_or(5_000);
        Ok(Self::new(endpoint, Duration::from_millis(timeout_ms)))
    }

    pub fn ground_truth_from_env() -> Result<Self, EngineError> {
        Self::from_env(GROUND_TRUTH_URL_ENV, "http://127.0.0.1:8790")
    }

    pub fn training_from_env() -> Result<Self, EngineError> {
        Self::from_env(TRAINING_URL_ENV, "http://127.0.0.1:8791")
    }

    pub fn v9_training_from_env() -> Result<Self, EngineError> {
        Self::from_env(V9_TRAINING_URL_ENV, "http://127.0.0.1:8792")
    }

    pub fn v10_training_from_env() -> Result<Self, EngineError> {
        // V10 is trained by sampling its search-improved policy. Using argmax
        // only in Play collapses a broad policy to the same highest-probability
        // action (often pass priority) at every decision.
        Self::from_env(V10_TRAINING_URL_ENV, "http://127.0.0.1:8795")
            .map(|client| client.with_deterministic(false))
    }

    pub fn v11_training_from_env() -> Result<Self, EngineError> {
        Self::from_env(V11_TRAINING_URL_ENV, "http://127.0.0.1:8803")
            .map(|client| client.with_deterministic(false))
    }

    pub fn v12_training_from_env() -> Result<Self, EngineError> {
        Self::from_env(V12_TRAINING_URL_ENV, "http://127.0.0.1:8813")
            .map(|client| client.with_deterministic(false))
    }

    fn status(&self, id: &str, fallback_label: &str) -> AiControllerStatus {
        if id != "ia-v12-in-training" {
            return offline_pilot_status(id);
        }
        let definition = pilot_definition(id);
        if let Ok(models) = self.models()
            && let Some(model) = models
                .into_iter()
                .find(|model| model.id == id && model.available)
        {
            return AiControllerStatus {
                id: definition.map_or(id, |item| item.id).to_string(),
                label: controller_label(&model.id, model.training_step),
                available: true,
                kind: definition.map_or("model", |item| item.kind).to_string(),
                capabilities: definition.map_or(
                    PilotCapabilities {
                        play: true,
                        deck_stats: false,
                        training: false,
                    },
                    |item| item.capabilities,
                ),
                controller_id: Some(id.to_string()),
                pilot_id: definition.map_or(id, |item| item.pilot_id).to_string(),
                model: Some(model.id),
            };
        }
        let health = ureq::get(&format!("{}/health", self.endpoint))
            .timeout(Duration::from_millis(750))
            .call()
            .ok()
            .and_then(|response| response.into_json::<RemoteHealthResponse>().ok());
        let available = health.as_ref().is_some_and(|response| response.model == id);
        AiControllerStatus {
            id: definition.map_or(id, |item| item.id).to_string(),
            label: health.as_ref().map_or_else(
                || fallback_label.to_string(),
                |response| controller_label(&response.model, response.training_step),
            ),
            available,
            kind: definition.map_or("model", |item| item.kind).to_string(),
            capabilities: definition.map_or(
                PilotCapabilities {
                    play: true,
                    deck_stats: false,
                    training: false,
                },
                |item| item.capabilities,
            ),
            controller_id: Some(id.to_string()),
            pilot_id: definition.map_or(id, |item| item.pilot_id).to_string(),
            model: health
                .filter(|response| response.model == id)
                .map(|response| response.model),
        }
    }

    fn models(&self) -> Result<Vec<RemoteModelStatus>, EngineError> {
        let response = ureq::get(&format!("{}/v1/models", self.endpoint))
            .timeout(Duration::from_millis(750))
            .call()
            .map_err(|error| EngineError::new(format!("Python AI catalog failed: {error}")))?;
        let catalog: RemoteModelCatalog = response
            .into_json()
            .map_err(|error| EngineError::new(format!("invalid Python AI catalog: {error}")))?;
        if catalog.schema_version != "oracle-ai-model-catalog/v1" {
            return Err(EngineError::new(format!(
                "Python AI catalog schema mismatch: {}",
                catalog.schema_version
            )));
        }
        Ok(catalog.models)
    }
}

fn autostart_enabled() -> bool {
    std::env::var(AUTOSTART_ENV)
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    endpoint
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| endpoint.strip_prefix("http://localhost:"))?
        .trim_end_matches('/')
        .parse()
        .ok()
}

fn endpoint_health(endpoint: &str) -> Option<RemoteHealthResponse> {
    ureq::get(&format!("{}/health", endpoint))
        .timeout(Duration::from_millis(500))
        .call()
        .ok()?
        .into_json()
        .ok()
}

fn paths_identify_same_file(actual: Option<&str>, expected: &Path) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    let actual = PathBuf::from(actual)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(actual));
    let expected = expected
        .canonicalize()
        .unwrap_or_else(|_| expected.to_path_buf());
    actual == expected
}

fn health_matches_service(
    health: &RemoteHealthResponse,
    service: LocalAiService,
    root: &Path,
) -> bool {
    match service {
        LocalAiService::GroundTruth => paths_identify_same_file(
            health.registry_path.as_deref(),
            &configured_path(
                root,
                MODEL_REGISTRY_ENV,
                "runs/oracle-ai-league-v8-simplified/model-registry.json",
            ),
        ),
        LocalAiService::V10Training => paths_identify_same_file(
            health.checkpoint_path.as_deref(),
            &configured_path(
                root,
                V10_TRAINING_CHECKPOINT_ENV,
                "runs/oracle-ai-league-v10-from-scratch/live/ia-v10-in-training",
            ),
        ),
    }
}

fn find_project_root_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|candidate| {
        candidate
            .join("python/oracle_ai/oracle_ai/app.py")
            .is_file()
            .then(|| candidate.to_path_buf())
    })
}

fn project_root() -> Result<PathBuf, EngineError> {
    if let Some(root) = std::env::var_os(PROJECT_ROOT_ENV) {
        let root = PathBuf::from(root);
        if root.join("python/oracle_ai/oracle_ai/app.py").is_file() {
            return Ok(root);
        }
        return Err(EngineError::new(format!(
            "{PROJECT_ROOT_ENV} does not contain python/oracle_ai: {}",
            root.display()
        )));
    }
    if let Ok(current) = std::env::current_dir()
        && let Some(root) = find_project_root_from(&current)
    {
        return Ok(root);
    }
    find_project_root_from(Path::new(env!("CARGO_MANIFEST_DIR")))
        .ok_or_else(|| EngineError::new("could not locate the Oracle AI project root"))
}

fn configured_path(root: &Path, variable: &str, default: &str) -> PathBuf {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default));
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn training_definition(model_id: Option<&str>) -> Result<&'static PilotDefinition, EngineError> {
    let requested = model_id.unwrap_or("v10");
    pilot_definition(requested)
        .filter(|definition| definition.capabilities.training)
        .ok_or_else(|| EngineError::new(format!("unknown training model: {requested}")))
}

fn training_run_path(root: &Path, definition: &PilotDefinition) -> PathBuf {
    let run = definition
        .training_run
        .expect("training-capable pilot has a run definition");
    if run.id == "v10" {
        configured_path(root, TRAINING_RUN_ENV, run.run_path)
    } else {
        root.join(run.run_path)
    }
}

fn training_config_path(root: &Path, definition: &PilotDefinition) -> PathBuf {
    let run = definition
        .training_run
        .expect("training-capable pilot has a run definition");
    if run.id == "v10" {
        configured_path(root, TRAINING_CONFIG_ENV, run.config_path)
    } else {
        root.join(run.config_path)
    }
}

fn read_json(path: &Path) -> Option<Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
}

fn recent_json_lines(path: &Path, limit: usize) -> Vec<Value> {
    if limit == 0 {
        return Vec::new();
    }
    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
        return Vec::new();
    };
    let read_length = length.min(1024 * 1024);
    if file
        .seek(SeekFrom::Start(length.saturating_sub(read_length)))
        .is_err()
    {
        return Vec::new();
    }
    let mut bytes = Vec::with_capacity(read_length as usize);
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let contents = String::from_utf8_lossy(&bytes);
    let complete_contents = if read_length < length {
        contents.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        contents.as_ref()
    };
    let mut values = complete_contents
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .take(limit)
        .collect::<Vec<_>>();
    values.reverse();
    values
}

fn compact_training_record(record: &Value) -> Value {
    let round_number = record.get("roundNumber").cloned().unwrap_or_else(|| {
        let turn_number = record.get("turnNumber").and_then(Value::as_u64);
        let player_count = record.get("players").and_then(Value::as_u64);
        match (turn_number, player_count) {
            (Some(0), _) => Value::from(0),
            (Some(turn), Some(players)) if players > 0 => Value::from(1 + (turn - 1) / players),
            _ => Value::Null,
        }
    });
    json!({
        "episode": record.get("episode"),
        "attempt": record.get("attempt"),
        "trainingStep": record.get("trainingStep"),
        "matchupId": record.get("matchupId"),
        "gameMode": record.get("gameMode"),
        "decks": record.get("decks"),
        "opponentMode": record.get("opponentMode"),
        "participantsByPlayer": record.get("participantsByPlayer"),
        "anchorDeadlineRound": record.get("anchorDeadlineRound"),
        "anchorOpeningHandPoolSize": record.get("anchorOpeningHandPoolSize"),
        "players": record.get("players"),
        "decisions": record.get("decisions"),
        "roundNumber": round_number,
        "gameDurationSeconds": record.get("gameDurationSeconds"),
        "trainingDurationSeconds": record.get("trainingDurationSeconds"),
        "episodeSeconds": record.get("episodeSeconds"),
        "collectionSeconds": record.get("collectionSeconds"),
        "ppoSeconds": record.get("ppoSeconds"),
        "trainingHour": record.get("trainingHour"),
        "rolloutBatch": record.get("rolloutBatch"),
        "gameplay": record.get("gameplay"),
        "ppo": record.get("ppo"),
        "behavior": compact_behavior(record.get("behavior")),
    })
}

fn compact_behavior(behavior: Option<&Value>) -> Value {
    let behavior = behavior.unwrap_or(&Value::Null);
    let anomalies = behavior.get("anomalies").unwrap_or(&Value::Null);
    json!({
        "totalDecisions": behavior.get("totalDecisions"),
        "meanConfidence": behavior.get("meanConfidence"),
        "meanEntropy": behavior.get("meanEntropy"),
        "priority": behavior.get("priority"),
        "combat": behavior.get("combat"),
        "mulligan": behavior.get("mulligan"),
        "anomalies": {
            "counts": anomalies.get("counts"),
            "rate": anomalies.get("rate"),
            "total": anomalies.get("total"),
        },
    })
}

fn compact_evaluation_record(record: &Value) -> Value {
    let summary = record.get("summary").unwrap_or(&Value::Null);
    json!({
        "period": record.get("period"),
        "candidateTrainingStep": record.get("candidateTrainingStep"),
        "opponentVersion": record.get("opponentVersion"),
        "evaluationSeconds": record.get("evaluationSeconds"),
        "perfectStreakAfter": record.get("perfectStreakAfter"),
        "promotionCountAfter": record.get("promotionCountAfter"),
        "promotion": record.get("promotion"),
        "summary": {
            "candidateWinRate": summary.get("candidateWinRate"),
            "candidateWins": summary.get("candidateWins"),
            "championWins": summary.get("championWins"),
            "completedGames": summary.get("completedGames"),
            "draws": summary.get("draws"),
            "errors": summary.get("errors"),
            "expectedGames": summary.get("expectedGames"),
            "meanRounds": summary.get("meanRounds"),
            "meanRoundsToCandidateWin": summary.get("meanRoundsToCandidateWin"),
            "perfect": summary.get("perfect"),
            "candidateBehavior": compact_behavior(summary.get("candidateBehavior")),
        },
    })
}

fn compact_ground_truth_evaluation(record: &Value) -> Value {
    json!({
        "period": record.get("period"),
        "completedEpisodes": record.get("completedEpisodes"),
        "trainingStep": record.get("trainingStep"),
        "scenarioCount": record.get("scenarioCount"),
        "decisionCount": record.get("decisionCount"),
        "coverage": record.get("coverage"),
        "metrics": record.get("metrics"),
        "evaluatedAt": record.get("evaluatedAt"),
    })
}

fn training_control_state(run: &Path) -> String {
    read_json(&run.join("training-control.json"))
        .and_then(|control| {
            control
                .get("desiredState")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|state| matches!(state.as_str(), "paused" | "running"))
        .unwrap_or_else(|| "running".to_string())
}

fn write_training_control(run: &Path, desired_state: &str) -> Result<(), EngineError> {
    fs::create_dir_all(run)
        .map_err(|error| EngineError::new(format!("could not create training run: {error}")))?;
    let path = run.join("training-control.json");
    let temporary = run.join(format!(".training-control.{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&json!({ "desiredState": desired_state }))
            .expect("training control serializes"),
    )
    .map_err(|error| EngineError::new(format!("could not write training control: {error}")))?;
    fs::rename(&temporary, &path)
        .map_err(|error| EngineError::new(format!("could not publish training control: {error}")))
}

#[cfg(windows)]
fn process_is_running(process_id: u32) -> bool {
    let mut command = Command::new("tasklist");
    command
        .args(["/FI", &format!("PID eq {process_id}"), "/FO", "CSV", "/NH"])
        .creation_flags(0x0800_0000);
    command.output().is_ok_and(|output| {
        String::from_utf8_lossy(&output.stdout).contains(&format!("\"{process_id}\""))
    })
}

#[cfg(not(windows))]
fn process_is_running(process_id: u32) -> bool {
    Path::new("/proc").join(process_id.to_string()).exists()
}

fn trainer_manager() -> &'static Mutex<LocalTrainingProcessManager> {
    static TRAINER: OnceLock<Mutex<LocalTrainingProcessManager>> = OnceLock::new();
    TRAINER.get_or_init(|| Mutex::new(LocalTrainingProcessManager::default()))
}

fn managed_trainer_is_running(
    manager: &mut LocalTrainingProcessManager,
    model_id: &str,
) -> Result<bool, EngineError> {
    let Some(child) = manager.trainers.get_mut(model_id) else {
        return Ok(false);
    };
    match child
        .try_wait()
        .map_err(|error| EngineError::new(format!("training process check failed: {error}")))?
    {
        None => Ok(true),
        Some(_) => {
            manager.trainers.remove(model_id);
            Ok(false)
        }
    }
}

fn spawn_training_process(
    manager: &mut LocalTrainingProcessManager,
    definition: &PilotDefinition,
) -> Result<(), EngineError> {
    let model_id = definition
        .training_run
        .expect("training-capable pilot has a run definition")
        .id;
    if managed_trainer_is_running(manager, model_id)? {
        return Ok(());
    }
    let root = project_root()?;
    let run = training_run_path(&root, definition);
    let config = training_config_path(&root, definition);
    if !config.is_file() {
        return Err(EngineError::new(format!(
            "training configuration is missing: {}",
            config.display()
        )));
    }
    fs::create_dir_all(&run)
        .map_err(|error| EngineError::new(format!("could not create training run: {error}")))?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.join("process.stdout.log"))
        .map_err(|error| EngineError::new(format!("could not open trainer output: {error}")))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.join("process.stderr.log"))
        .map_err(|error| EngineError::new(format!("could not open trainer errors: {error}")))?;
    let mut command = Command::new(python_executable(&root));
    command
        .args(["-m", "oracle_ai.training.league", "--config"])
        .arg(config)
        .current_dir(&root)
        .env("PYTHONPATH", root.join("python/oracle_ai"))
        .env("PYTHONUNBUFFERED", "1")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    manager.trainers.insert(
        model_id.to_string(),
        command
            .spawn()
            .map_err(|error| EngineError::new(format!("could not start trainer: {error}")))?,
    );
    Ok(())
}

#[cfg(windows)]
fn terminate_process_tree(process_id: u32) {
    let mut command = Command::new("taskkill");
    command
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000);
    let _ = command.output();
}

#[cfg(not(windows))]
fn terminate_process_tree(process_id: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &process_id.to_string()])
        .output();
}

fn stop_training_process(model_id: &str, state: Option<&Value>) -> Result<(), EngineError> {
    let mut manager = trainer_manager()
        .lock()
        .map_err(|_| EngineError::new("training process manager lock is poisoned"))?;
    if let Some(child) = manager.trainers.get_mut(model_id) {
        terminate_process_tree(child.id());
        let _ = child.kill();
        let _ = child.wait();
    }
    manager.trainers.remove(model_id);
    if let Some(process_id) = state
        .and_then(|state| state.get("processId"))
        .and_then(Value::as_u64)
        .and_then(|process_id| u32::try_from(process_id).ok())
        && process_is_running(process_id)
    {
        terminate_process_tree(process_id);
    }
    Ok(())
}

fn training_process_is_running(model_id: &str, state: Option<&Value>) -> Result<bool, EngineError> {
    let mut manager = trainer_manager()
        .lock()
        .map_err(|_| EngineError::new("training process manager lock is poisoned"))?;
    if managed_trainer_is_running(&mut manager, model_id)? {
        return Ok(true);
    }
    Ok(state
        .and_then(|state| state.get("processId"))
        .and_then(Value::as_u64)
        .and_then(|process_id| u32::try_from(process_id).ok())
        .is_some_and(process_is_running))
}

pub fn training_dashboard(model_id: Option<&str>) -> Result<Value, EngineError> {
    let root = project_root()?;
    let definition = training_definition(model_id)?;
    let training_run = definition
        .training_run
        .expect("training-capable pilot has a run definition");
    let run = training_run_path(&root, definition);
    let state = read_json(&run.join("league-state.json"));
    let desired_state = training_control_state(&run);
    let running = training_process_is_running(training_run.id, state.as_ref())?;
    let paused = state
        .as_ref()
        .and_then(|state| state.get("paused"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = match (running, desired_state.as_str(), paused) {
        (true, "paused", true) => "paused",
        (true, "paused", false) => "pausing",
        (true, _, _) => "running",
        (false, _, _) if state.is_none() => "not-started",
        (false, _, _) => "stopped",
    };
    let training_history = recent_json_lines(&run.join("training.jsonl"), 40)
        .iter()
        .map(compact_training_record)
        .collect::<Vec<_>>();
    let evaluation_history = recent_json_lines(&run.join("evaluations.jsonl"), 12)
        .iter()
        .map(compact_evaluation_record)
        .collect::<Vec<_>>();
    let ground_truth_history = recent_json_lines(&run.join("ground-truth-evaluations.jsonl"), 100)
        .iter()
        .map(compact_ground_truth_evaluation)
        .collect::<Vec<_>>();
    let latest_error = recent_json_lines(&run.join("training-errors.jsonl"), 1)
        .into_iter()
        .next();
    let baseline = read_json(&run.join("champions/ia-gt-0/manifest.json"));
    let gameplay_hourly = read_json(&run.join("training-gameplay-hourly.json"));
    let training_leaderboard = read_json(&run.join("training-leaderboard.json"));
    Ok(json!({
        "schemaVersion": TRAINING_DASHBOARD_SCHEMA_VERSION,
        "selectedModelId": training_run.id,
        "selectedModel": definition,
        "models": training_pilots().map(|pilot| {
            let run = pilot.training_run.expect("training pilot run");
            json!({
                "id": pilot.id,
                "label": pilot.label,
                "kind": pilot.kind,
                "capabilities": pilot.capabilities,
                "controllerId": pilot.controller_id,
                "pilotId": pilot.pilot_id,
                "trainingRun": run,
                "available": training_config_path(&root, pilot).is_file(),
            })
        }).collect::<Vec<_>>(),
        "status": status,
        "desiredState": desired_state,
        "running": running,
        "runPath": run.strip_prefix(&root).unwrap_or(&run).to_string_lossy(),
        "baseline": baseline.map(|manifest| json!({
            "modelFamily": manifest.get("model_family"),
            "trainingStep": manifest.get("training_step"),
        })),
        "state": state,
        "training": {
            "latest": training_history.last(),
            "history": training_history,
            "latestError": latest_error,
            "gameplayHourly": gameplay_hourly,
        },
        "evaluation": {
            "latest": evaluation_history.last(),
            "history": evaluation_history,
        },
        "groundTruthEvaluation": {
            "latest": ground_truth_history.last(),
            "history": ground_truth_history,
        },
        "trainingLeaderboard": training_leaderboard,
    }))
}

pub fn control_training(model_id: Option<&str>, action: &str) -> Result<Value, EngineError> {
    let root = project_root()?;
    let definition = training_definition(model_id)?;
    let training_run = definition
        .training_run
        .expect("training-capable pilot has a run definition");
    let run = training_run_path(&root, definition);
    match action {
        "pause" => write_training_control(&run, "paused")?,
        "resume" => {
            write_training_control(&run, "running")?;
            let state = read_json(&run.join("league-state.json"));
            if !training_process_is_running(training_run.id, state.as_ref())? {
                let mut manager = trainer_manager()
                    .lock()
                    .map_err(|_| EngineError::new("training process manager lock is poisoned"))?;
                spawn_training_process(&mut manager, definition)?;
            }
        }
        "restart" => {
            let state = read_json(&run.join("league-state.json"));
            stop_training_process(training_run.id, state.as_ref())?;
            write_training_control(&run, "running")?;
            let mut manager = trainer_manager()
                .lock()
                .map_err(|_| EngineError::new("training process manager lock is poisoned"))?;
            spawn_training_process(&mut manager, definition)?;
        }
        _ => {
            return Err(EngineError::new(format!(
                "unsupported training action: {action}"
            )));
        }
    }
    training_dashboard(Some(training_run.id))
}

fn python_executable(root: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os(PYTHON_ENV) {
        return PathBuf::from(path);
    }
    let candidate = if cfg!(windows) {
        root.join(".tmp/oracle-ai-venv/Scripts/python.exe")
    } else {
        root.join(".tmp/oracle-ai-venv/bin/python")
    };
    if candidate.is_file() {
        candidate
    } else {
        PathBuf::from("python")
    }
}

fn ground_truth_name(registry: &Path) -> String {
    fs::read_to_string(registry)
        .ok()
        .and_then(|contents| serde_json::from_str::<LocalModelRegistry>(&contents).ok())
        .and_then(|registry| registry.current_ground_truth)
        .unwrap_or_else(|| "ia-gt-0".to_string())
}

fn spawn_local_service(service: LocalAiService, endpoint: &str) -> Result<Child, EngineError> {
    let port = endpoint_port(endpoint).ok_or_else(|| {
        EngineError::new(format!(
            "AI autostart only supports local HTTP endpoints: {endpoint}"
        ))
    })?;
    let root = project_root()?;
    let registry = configured_path(
        &root,
        MODEL_REGISTRY_ENV,
        "runs/oracle-ai-league-v8-simplified/model-registry.json",
    );
    let checkpoint = match service {
        LocalAiService::V10Training => configured_path(
            &root,
            V10_TRAINING_CHECKPOINT_ENV,
            "runs/oracle-ai-league-v10-from-scratch/live/ia-v10-in-training",
        ),
        LocalAiService::GroundTruth => PathBuf::new(),
    };
    let (model_name, log_name) = match service {
        LocalAiService::GroundTruth => {
            if !registry.is_file() {
                return Err(EngineError::new(format!(
                    "ground-truth model registry is missing: {}",
                    registry.display()
                )));
            }
            (ground_truth_name(&registry), "ia-gt.log")
        }
        LocalAiService::V10Training => {
            if !checkpoint.join("manifest.json").is_file() {
                return Err(EngineError::new(format!(
                    "V10 training checkpoint is missing: {}",
                    checkpoint.display()
                )));
            }
            ("ia-v10-in-training".to_string(), "ia-v10-in-training.log")
        }
    };
    let log_directory = root.join(".tmp/ai-services");
    fs::create_dir_all(&log_directory)
        .map_err(|error| EngineError::new(format!("could not create AI log directory: {error}")))?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_directory.join(log_name))
        .map_err(|error| EngineError::new(format!("could not open AI service log: {error}")))?;
    let error_log = log
        .try_clone()
        .map_err(|error| EngineError::new(format!("could not clone AI service log: {error}")))?;
    let mut command = Command::new(python_executable(&root));
    command
        .args(["-m", "uvicorn", "oracle_ai.app:app", "--app-dir"])
        .arg(root.join("python/oracle_ai"))
        .args(["--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .current_dir(&root)
        .env(
            "ORACLE_AI_DEVICE",
            std::env::var(INFERENCE_DEVICE_ENV).unwrap_or_else(|_| "cuda".to_string()),
        )
        .env("ORACLE_AI_MODEL_NAME", &model_name)
        .env("ORACLE_AI_POLICY", "model")
        .env("PYTHONUNBUFFERED", "1")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(error_log));
    match service {
        LocalAiService::GroundTruth => {
            command
                .env("ORACLE_AI_MODEL_REGISTRY", registry)
                .env_remove("ORACLE_AI_CHECKPOINT");
        }
        LocalAiService::V10Training => {
            command
                .env("ORACLE_AI_CHECKPOINT", checkpoint)
                .env_remove("ORACLE_AI_MODEL_REGISTRY");
        }
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command.spawn().map_err(|error| {
        EngineError::new(format!("could not start Python AI {model_name}: {error}"))
    })
}

impl LocalAiServiceManager {
    fn ensure(&mut self, service: LocalAiService, endpoint: &str) -> Result<(), EngineError> {
        let root = project_root()?;
        if let Some(health) = endpoint_health(endpoint) {
            if health_matches_service(&health, service, &root) {
                return Ok(());
            }
            return Err(EngineError::new(format!(
                "Python AI endpoint {endpoint} serves {} from a different run",
                health.model
            )));
        }
        let slot = match service {
            LocalAiService::GroundTruth => &mut self.ground_truth,
            LocalAiService::V10Training => &mut self.v10_training,
        };
        if let Some(child) = slot.as_mut()
            && child
                .try_wait()
                .map_err(|error| EngineError::new(format!("AI process check failed: {error}")))?
                .is_some()
        {
            *slot = None;
        }
        if slot.is_none() {
            *slot = Some(spawn_local_service(service, endpoint)?);
        }
        let timeout = std::env::var(STARTUP_TIMEOUT_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60_000);
        let deadline = Instant::now() + Duration::from_millis(timeout);
        while Instant::now() < deadline {
            if let Some(health) = endpoint_health(endpoint) {
                if health_matches_service(&health, service, &root) {
                    return Ok(());
                }
                return Err(EngineError::new(format!(
                    "Python AI endpoint {endpoint} started with the wrong model identity"
                )));
            }
            if let Some(child) = slot.as_mut()
                && let Some(status) = child.try_wait().map_err(|error| {
                    EngineError::new(format!("AI process check failed: {error}"))
                })?
            {
                *slot = None;
                return Err(EngineError::new(format!(
                    "Python AI service exited with {status}"
                )));
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(EngineError::new(format!(
            "Python AI service did not become healthy at {endpoint}"
        )))
    }
}

fn ensure_local_ai_services() -> Result<(), EngineError> {
    // V12 training owns the sole inference service. Reading the Play catalog
    // must not revive retired ground-truth, V8, V9, V10, or V11 processes.
    Ok(())
}

pub fn controller_catalog() -> AiControllerCatalog {
    let _ = ensure_local_ai_services();
    let mut controllers = vec![AiControllerStatus {
        id: "ai-random".to_string(),
        kind: "random".to_string(),
        capabilities: PilotCapabilities {
            play: true,
            deck_stats: true,
            training: false,
        },
        controller_id: Some("ai-random".to_string()),
        pilot_id: "ai-random".to_string(),
        label: "IA aléatoire".to_string(),
        available: false,
        model: None,
    }];
    let ground_truth_models: Vec<RemoteModelStatus> = Vec::new();
    if ground_truth_models.is_empty() {
        controllers.push(AiControllerStatus {
            id: "ia-gt-0".to_string(),
            kind: "model".to_string(),
            capabilities: PilotCapabilities {
                play: true,
                deck_stats: true,
                training: false,
            },
            controller_id: Some("ia-gt-0".to_string()),
            pilot_id: "ia-gt-0".to_string(),
            label: "ia-gt-0".to_string(),
            available: false,
            model: None,
        });
    } else {
        controllers.extend(
            ground_truth_models
                .into_iter()
                .filter(|model| model.available)
                .map(|model| AiControllerStatus {
                    kind: "model".to_string(),
                    capabilities: PilotCapabilities {
                        play: true,
                        deck_stats: true,
                        training: false,
                    },
                    controller_id: Some(model.id.clone()),
                    pilot_id: model.id.clone(),
                    label: controller_label(&model.id, model.training_step),
                    model: Some(model.id.clone()),
                    id: model.id,
                    available: false,
                }),
        );
    }
    let mut training = RemoteAi::training_from_env()
        .map(|client| client.status("ia-in-training", "IA V8 simplifiée en entraînement"))
        .unwrap_or_else(|_| AiControllerStatus {
            id: "ia-in-training".to_string(),
            kind: "model".to_string(),
            capabilities: pilot_definition("ia-in-training")
                .expect("V8 pilot definition")
                .capabilities,
            controller_id: Some("ia-in-training".to_string()),
            pilot_id: "ia-v8-in-training".to_string(),
            label: "IA V8 simplifiée en entraînement".to_string(),
            available: false,
            model: None,
        });
    training.available = false;
    controllers.push(training);
    let mut v9_training = RemoteAi::v9_training_from_env()
        .map(|client| client.status("ia-v9-in-training", "IA V9 en entraînement"))
        .unwrap_or_else(|_| AiControllerStatus {
            id: "ia-v9-in-training".to_string(),
            kind: "model".to_string(),
            capabilities: pilot_definition("ia-v9-in-training")
                .expect("V9 pilot definition")
                .capabilities,
            controller_id: Some("ia-v9-in-training".to_string()),
            pilot_id: "ia-v9-in-training".to_string(),
            label: "IA V9 en entraînement".to_string(),
            available: false,
            model: None,
        });
    v9_training.available = false;
    controllers.push(v9_training);
    let mut v10_training = RemoteAi::v10_training_from_env()
        .map(|client| {
            client.status(
                "ia-v10-in-training",
                pilot_definition("v10").expect("V10 pilot definition").label,
            )
        })
        .unwrap_or_else(|_| {
            let definition = pilot_definition("v10").expect("V10 pilot definition");
            AiControllerStatus {
                id: definition.id.to_string(),
                label: definition.label.to_string(),
                available: false,
                kind: definition.kind.to_string(),
                capabilities: definition.capabilities,
                controller_id: definition.controller_id.map(str::to_string),
                pilot_id: definition.pilot_id.to_string(),
                model: None,
            }
        });
    v10_training.available = false;
    controllers.push(v10_training);
    let mut v11_training = RemoteAi::v11_training_from_env()
        .map(|client| {
            client.status(
                "ia-v11-in-training",
                pilot_definition("v11").expect("V11 pilot definition").label,
            )
        })
        .unwrap_or_else(|_| {
            let definition = pilot_definition("v11").expect("V11 pilot definition");
            AiControllerStatus {
                id: definition.id.to_string(),
                label: definition.label.to_string(),
                available: false,
                kind: definition.kind.to_string(),
                capabilities: definition.capabilities,
                controller_id: definition.controller_id.map(str::to_string),
                pilot_id: definition.pilot_id.to_string(),
                model: None,
            }
        });
    v11_training.available = false;
    controllers.push(v11_training);
    let v12_training = RemoteAi::v12_training_from_env()
        .map(|client| {
            client.status(
                "ia-v12-in-training",
                pilot_definition("v12").expect("V12 pilot definition").label,
            )
        })
        .unwrap_or_else(|_| {
            let definition = pilot_definition("v12").expect("V12 pilot definition");
            AiControllerStatus {
                id: definition.id.to_string(),
                label: definition.label.to_string(),
                available: false,
                kind: definition.kind.to_string(),
                capabilities: definition.capabilities,
                controller_id: definition.controller_id.map(str::to_string),
                pilot_id: definition.pilot_id.to_string(),
                model: None,
            }
        });
    controllers.push(v12_training);
    for definition in crate::pilot_catalog::PILOT_DEFINITIONS {
        if !definition.capabilities.play {
            continue;
        }
        if controllers.iter().any(|entry| entry.id == definition.id) {
            continue;
        }
        controllers.push(AiControllerStatus {
            id: definition.id.to_string(),
            label: definition.label.to_string(),
            available: definition.kind == "human",
            kind: definition.kind.to_string(),
            capabilities: definition.capabilities,
            controller_id: definition.controller_id.map(str::to_string),
            pilot_id: definition.pilot_id.to_string(),
            model: None,
        });
    }
    controllers.sort_by_key(|entry| match entry.kind.as_str() {
        "human" => (0, entry.label.clone()),
        "random" => (1, entry.label.clone()),
        _ => (2, entry.label.clone()),
    });
    AiControllerCatalog {
        schema_version: CONTROLLER_SCHEMA_VERSION,
        controllers,
    }
}

impl DecisionProvider for RemoteAi {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        ensure_local_ai_services()?;
        let agent_request = request.agent_facing();
        if agent_request.options.is_empty() {
            return Err(EngineError::new(format!(
                "decision {} contains only UI actions",
                request.id
            )));
        }
        let payload = RemoteDecisionRequest {
            controller_id: self.controller_id.as_deref(),
            context_id: self.context_id.as_deref(),
            schema_version: DECISION_SCHEMA_VERSION,
            request_id: &request.id,
            player_id: &request.player_id,
            deterministic: self.deterministic,
            state,
            decision: &agent_request,
        };
        let response = ureq::post(&format!("{}/v1/decisions", self.endpoint))
            .timeout(self.timeout)
            .send_json(&payload)
            .map_err(|error| EngineError::new(format!("Python AI request failed: {error}")))?;
        let response: RemoteDecisionResponse = response
            .into_json()
            .map_err(|error| EngineError::new(format!("invalid Python AI response: {error}")))?;

        if response.schema_version != DECISION_SCHEMA_VERSION {
            return Err(EngineError::new(format!(
                "Python AI schema mismatch: expected {DECISION_SCHEMA_VERSION}, received {}",
                response.schema_version
            )));
        }
        if response.request_id != request.id {
            return Err(EngineError::new(format!(
                "Python AI answered {} while {} was pending",
                response.request_id, request.id
            )));
        }

        request
            .options
            .iter()
            .position(|action| action.id == response.action_id)
            .ok_or_else(|| {
                EngineError::new(format!(
                    "Python AI returned action {} that is not legal for {}",
                    response.action_id, request.id
                ))
            })
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        ensure_local_ai_services()?;
        let agent_request = request.agent_facing();
        if agent_request.options.is_empty() {
            return Err(EngineError::new(format!(
                "decision {} contains only UI actions",
                request.id
            )));
        }
        let payload = RemoteDecisionRequest {
            controller_id: self.controller_id.as_deref(),
            context_id: self.context_id.as_deref(),
            schema_version: DECISION_SCHEMA_VERSION,
            request_id: &request.id,
            player_id: &request.player_id,
            deterministic: self.deterministic,
            state,
            decision: &agent_request,
        };
        let response = ureq::post(&format!("{}/v1/decisions", self.endpoint))
            .timeout(self.timeout)
            .send_json(&payload)
            .map_err(|error| EngineError::new(format!("Python AI request failed: {error}")))?;
        let response: RemoteDecisionResponse = response
            .into_json()
            .map_err(|error| EngineError::new(format!("invalid Python AI response: {error}")))?;
        if response.schema_version != DECISION_SCHEMA_VERSION || response.request_id != request.id {
            return Err(EngineError::new(format!(
                "Python AI returned an invalid response for {}",
                request.id
            )));
        }
        let selected = response.number_value.ok_or_else(|| {
            EngineError::new(format!("Python AI omitted numberValue for {}", request.id))
        })?;
        let Some((minimum, maximum)) = decision_number_bounds(request) else {
            return Err(EngineError::new(format!(
                "decision {} is not a number selection",
                request.id
            )));
        };
        if !(minimum..=maximum).contains(&selected) {
            return Err(EngineError::new(format!(
                "Python AI returned number {selected} outside {minimum}..={maximum} for {}",
                request.id
            )));
        }
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RemoteAi, RemoteHealthResponse, compact_training_record, endpoint_port, recent_json_lines,
        training_control_state, write_training_control,
    };
    use serde_json::json;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn v10_gameplay_remote_ai_samples_the_search_policy() {
        let client = RemoteAi::new("http://127.0.0.1:8795", Duration::from_secs(1))
            .with_deterministic(false);

        assert!(!client.deterministic);
    }

    #[test]
    fn autostart_accepts_only_local_http_endpoints() {
        assert_eq!(endpoint_port("http://127.0.0.1:8790"), Some(8790));
        assert_eq!(endpoint_port("http://localhost:8791/"), Some(8791));
        assert_eq!(endpoint_port("https://example.test:8790"), None);
    }

    #[test]
    fn health_response_reads_camel_case_model_paths() {
        let health: RemoteHealthResponse = serde_json::from_str(
            r#"{
                "checkpointPath": "runs/live/ia-in-training",
                "model": "ia-in-training",
                "registryPath": "runs/model-registry.json",
                "trainingStep": 42
            }"#,
        )
        .expect("health response");

        assert_eq!(
            health.checkpoint_path.as_deref(),
            Some("runs/live/ia-in-training")
        );
        assert_eq!(
            health.registry_path.as_deref(),
            Some("runs/model-registry.json")
        );
        assert_eq!(health.training_step, 42);
    }

    #[test]
    fn dashboard_reads_only_recent_metrics_and_persists_pause() {
        let run = std::env::temp_dir().join(format!("oracle-ai-dashboard-{}", std::process::id()));
        let _ = fs::remove_dir_all(&run);
        fs::create_dir_all(&run).expect("temporary run directory");
        fs::write(
            run.join("training.jsonl"),
            "{\"episode\":1}\n{\"episode\":2}\n{\"episode\":3}\n",
        )
        .expect("training fixture");

        let recent = recent_json_lines(&run.join("training.jsonl"), 2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0]["episode"], 2);
        assert_eq!(recent[1]["episode"], 3);

        write_training_control(&run, "paused").expect("pause control");
        assert_eq!(training_control_state(&run), "paused");
        fs::remove_dir_all(&run).expect("temporary run cleanup");
    }

    #[test]
    fn dashboard_keeps_game_and_optimizer_durations_distinct() {
        let compact = compact_training_record(&json!({
            "episode": 8,
            "gameDurationSeconds": 12.5,
            "trainingDurationSeconds": 3.25,
            "episodeSeconds": 15.75,
        }));

        assert_eq!(compact["gameDurationSeconds"], 12.5);
        assert_eq!(compact["trainingDurationSeconds"], 3.25);
        assert_eq!(compact["episodeSeconds"], 15.75);
    }
}
