use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotCapabilities {
    pub play: bool,
    pub deck_stats: bool,
    pub training: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingRunDefinition {
    pub id: &'static str,
    pub run_path: &'static str,
    pub config_path: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
    pub capabilities: PilotCapabilities,
    pub controller_id: Option<&'static str>,
    pub pilot_id: &'static str,
    pub training_run: Option<TrainingRunDefinition>,
}

const PLAY_AND_STATS: PilotCapabilities = PilotCapabilities {
    play: true,
    deck_stats: true,
    training: false,
};

const STATS_ONLY: PilotCapabilities = PilotCapabilities {
    play: false,
    deck_stats: true,
    training: false,
};

const MODEL_CAPABILITIES: PilotCapabilities = PilotCapabilities {
    play: true,
    deck_stats: true,
    training: true,
};

pub const PILOT_DEFINITIONS: &[PilotDefinition] = &[
    PilotDefinition {
        id: "human",
        label: "Humain",
        kind: "human",
        capabilities: PLAY_AND_STATS,
        controller_id: Some("human"),
        pilot_id: "human",
        training_run: None,
    },
    PilotDefinition {
        id: "network-human",
        label: "Humain · réseau",
        kind: "human",
        capabilities: STATS_ONLY,
        controller_id: None,
        pilot_id: "network-human",
        training_run: None,
    },
    PilotDefinition {
        id: "ai-random",
        label: "IA aléatoire",
        kind: "random",
        capabilities: PLAY_AND_STATS,
        controller_id: Some("ai-random"),
        pilot_id: "ai-random",
        training_run: None,
    },
    PilotDefinition {
        id: "ia-v6-in-training",
        label: "IA V6 · AlphaZero",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: None,
        pilot_id: "ia-v6-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v6",
            run_path: "runs/oracle-ai-league-v6-alpha-zero",
            config_path: "python/oracle_ai/configs/league-v6-alpha-zero.yaml",
        }),
    },
    PilotDefinition {
        id: "ia-v7-in-training",
        label: "IA V7 · stratégie",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: None,
        pilot_id: "ia-v7-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v7",
            run_path: "runs/oracle-ai-league-v7-strategic",
            config_path: "python/oracle_ai/configs/league-v7-strategic.yaml",
        }),
    },
    PilotDefinition {
        id: "ia-v8-in-training",
        label: "IA V8 · parties simplifiées",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: Some("ia-in-training"),
        pilot_id: "ia-v8-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v8",
            run_path: "runs/oracle-ai-league-v8-simplified",
            config_path: "python/oracle_ai/configs/league-v8-simplified.yaml",
        }),
    },
    PilotDefinition {
        id: "ia-v9-in-training",
        label: "IA V9 · conséquences",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: Some("ia-v9-in-training"),
        pilot_id: "ia-v9-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v9",
            run_path: "runs/oracle-ai-league-v9-consequence",
            config_path: "python/oracle_ai/configs/league-v9-consequence.yaml",
        }),
    },
    PilotDefinition {
        id: "ia-v10-step-94266",
        label: "IA V10 · checkpoint 94266",
        kind: "model",
        capabilities: PLAY_AND_STATS,
        controller_id: Some("ia-v10-step-94266"),
        pilot_id: "ia-v10-step-94266",
        training_run: None,
    },
    PilotDefinition {
        id: "ia-v10-in-training",
        label: "IA V10 · événements contextuels",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: Some("ia-v10-in-training"),
        pilot_id: "ia-v10-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v10",
            run_path: "runs/oracle-ai-league-v10-from-scratch",
            config_path: "python/oracle_ai/configs/league-v10-events.yaml",
        }),
    },
    PilotDefinition {
        id: "ia-v11-in-training",
        label: "IA V11 · AlphaStar",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: Some("ia-v11-in-training"),
        pilot_id: "ia-v11-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v11",
            run_path: "runs/oracle-ai-league-v11-alphastar",
            config_path: "python/oracle_ai/configs/league-v11-alphastar.yaml",
        }),
    },
    PilotDefinition {
        id: "ia-v12-in-training",
        label: "IA V12 · AlphaStar Legacy",
        kind: "model",
        capabilities: MODEL_CAPABILITIES,
        controller_id: Some("ia-v12-in-training"),
        pilot_id: "ia-v12-in-training",
        training_run: Some(TrainingRunDefinition {
            id: "v12",
            run_path: "runs/oracle-ai-league-v12-legacy",
            config_path: "python/oracle_ai/configs/league-v12-legacy.yaml",
        }),
    },
    PilotDefinition {
        id: "ai-training-anchor",
        label: "Ancre d'entraînement déterministe",
        kind: "anchor",
        capabilities: STATS_ONLY,
        controller_id: None,
        pilot_id: "ai-training-anchor",
        training_run: None,
    },
];

pub fn pilot_definition(id: &str) -> Option<&'static PilotDefinition> {
    PILOT_DEFINITIONS.iter().find(|definition| {
        definition.id == id
            || definition.pilot_id == id
            || definition.controller_id == Some(id)
            || definition.training_run.is_some_and(|run| run.id == id)
    })
}

pub fn is_playable_model_controller(id: &str) -> bool {
    id == "ia-v12-in-training"
        && pilot_definition(id).is_some_and(|definition| {
            definition.kind == "model"
                && definition.capabilities.play
                && definition.controller_id == Some(id)
        })
}

pub fn training_pilots() -> impl Iterator<Item = &'static PilotDefinition> {
    PILOT_DEFINITIONS
        .iter()
        .filter(|definition| definition.capabilities.training)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_v10_checkpoint_remains_visible_but_is_not_playable() {
        let pilot = pilot_definition("ia-v10-step-94266").expect("frozen V10 pilot");

        assert_eq!(pilot.controller_id, Some("ia-v10-step-94266"));
        assert!(pilot.capabilities.play);
        assert!(pilot.capabilities.deck_stats);
        assert!(!pilot.capabilities.training);
        assert!(pilot.training_run.is_none());
        assert!(!is_playable_model_controller("ia-v10-step-94266"));
        assert!(!is_playable_model_controller("unknown-model"));
    }

    #[test]
    fn v12_is_the_only_accepted_model_controller() {
        assert!(is_playable_model_controller("ia-v12-in-training"));
        assert!(!is_playable_model_controller("ia-v11-in-training"));
    }

    #[test]
    fn network_human_is_a_stats_only_pilot() {
        let pilot = pilot_definition("network-human").expect("network human pilot");

        assert_eq!(pilot.label, "Humain · réseau");
        assert!(!pilot.capabilities.play);
        assert!(pilot.capabilities.deck_stats);
        assert!(!pilot.capabilities.training);
        assert!(pilot.controller_id.is_none());
    }
}
