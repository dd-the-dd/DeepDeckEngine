use crate::engine::{
    DecisionProvider, EngineDecisionRequest, EngineError, GameState, decision_number_bounds,
};
use crate::pilot_catalog::{PilotCapabilities, pilot_definition};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DECISION_SCHEMA_VERSION: &str = "ai-decision/v1";
const CONTROLLER_SCHEMA_VERSION: &str = "ai-controller-catalog/v1";
const GROUND_TRUTH_URL_ENV: &str = "MTG_AI_GT_URL";
const TRAINING_URL_ENV: &str = "MTG_AI_TRAINING_URL";
const V9_TRAINING_URL_ENV: &str = "MTG_AI_V9_TRAINING_URL";
const V10_TRAINING_URL_ENV: &str = "MTG_AI_V10_TRAINING_URL";
const V11_TRAINING_URL_ENV: &str = "MTG_AI_V11_TRAINING_URL";
const V12_TRAINING_URL_ENV: &str = "MTG_AI_V12_TRAINING_URL";

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
    model: String,
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

pub fn controller_catalog() -> AiControllerCatalog {
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
                pilot_definition("ia-v10-in-training")
                    .expect("V10 pilot definition")
                    .label,
            )
        })
        .unwrap_or_else(|_| {
            let definition = pilot_definition("ia-v10-in-training").expect("V10 pilot definition");
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
                pilot_definition("ia-v11-in-training")
                    .expect("V11 pilot definition")
                    .label,
            )
        })
        .unwrap_or_else(|_| {
            let definition = pilot_definition("ia-v11-in-training").expect("V11 pilot definition");
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
                pilot_definition("ia-v12-in-training")
                    .expect("V12 pilot definition")
                    .label,
            )
        })
        .unwrap_or_else(|_| {
            let definition = pilot_definition("ia-v12-in-training").expect("V12 pilot definition");
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
    use super::RemoteAi;
    use std::time::Duration;

    #[test]
    fn v10_gameplay_remote_ai_samples_the_search_policy() {
        let client = RemoteAi::new("http://127.0.0.1:8795", Duration::from_secs(1))
            .with_deterministic(false);

        assert!(!client.deterministic);
    }
}
