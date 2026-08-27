use crate::engine::{
    CardDefinition, DecisionProvider, EngineDecisionRequest, EngineError, GameEngine, GameOutcome,
    GameSetup, GameState, GameStatus, GameStep, PlayerDeck, UntapBoundaryStatus,
    decision_number_bounds,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::Serialize;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

const SEARCH_BRANCH_SIGNAL: &str = "punching-bag-search-branch";
pub const PUNCHING_BAG_PLAYER_ID: &str = "punching-bag";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PunchingBagSearchRoot {
    OpeningDecision,
    WinningTurnUntap,
}

#[derive(Clone, Debug)]
pub struct PunchingBagBenchmarkConfig {
    pub seed: u64,
    pub search_root: PunchingBagSearchRoot,
    pub maximum_random_games: usize,
    pub maximum_turns: u32,
    pub maximum_unique_nodes: usize,
    pub maximum_depth: usize,
    pub maximum_choices_per_node: usize,
}

impl Default for PunchingBagBenchmarkConfig {
    fn default() -> Self {
        Self {
            seed: 20_260_810,
            search_root: PunchingBagSearchRoot::WinningTurnUntap,
            maximum_random_games: 1_000,
            maximum_turns: 80,
            maximum_unique_nodes: 1_000_000,
            maximum_depth: 128,
            maximum_choices_per_node: 4_096,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProgressivePunchingBagConfig {
    pub seed: u64,
    pub maximum_segments: usize,
    pub maximum_turns: u32,
    pub maximum_unique_nodes: usize,
    pub maximum_depth: usize,
    pub maximum_choices_per_node: usize,
    pub position: Option<PunchingBagPosition>,
}

impl Default for ProgressivePunchingBagConfig {
    fn default() -> Self {
        Self {
            seed: 20_260_810,
            maximum_segments: 80,
            maximum_turns: 160,
            maximum_unique_nodes: 1_000_000,
            maximum_depth: 128,
            maximum_choices_per_node: 4_096,
            position: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PunchingBagPosition {
    pub learner_battlefield_definition_ids: Vec<String>,
    pub learner_hand_definition_ids: Vec<String>,
    pub opponent_battlefield: Vec<CardDefinition>,
    pub opponent_library_size: usize,
    pub opponent_skips_draw_step: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PunchingBagSpecification {
    pub player_id: String,
    pub initial_hand_size: usize,
    pub library_card_count: usize,
    pub card_types: Vec<String>,
    pub mana_cost: String,
    pub creature_power: i32,
    pub creature_toughness: i32,
    pub vitality_period: i32,
    pub supply_period: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WinningPositionReport {
    pub seed: u64,
    pub games_attempted: usize,
    pub discovery_elapsed_ms: u128,
    pub winning_turn: u32,
    pub root_turn: u32,
    pub root_step: GameStep,
    pub root_player_id: String,
    pub root_decision_id: String,
    pub recorded_winning_choice: String,
    pub prefix_choice_count: usize,
    pub known_winning_suffix_choice_count: usize,
    pub known_winning_line: Vec<ScenarioDecisionChoice>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDecisionChoice {
    pub decision_id: String,
    pub player_id: String,
    pub active_player_id: String,
    pub turn_number: u32,
    pub step: GameStep,
    pub choice_kind: String,
    pub choice: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootChoiceReport {
    pub choice_kind: String,
    pub choice: String,
    pub complete: bool,
    pub total_leaves: u64,
    pub learner_win_leaves: u64,
    pub opponent_win_leaves: u64,
    pub draw_leaves: u64,
    pub next_untap_leaves: u64,
    pub search_limit_leaves: u64,
    pub engine_safety_limit_leaves: u64,
    pub learner_win_leaf_fraction: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UntapTreeReport {
    pub complete: bool,
    pub exact_for_fixed_random_tape: bool,
    pub elapsed_ms: u128,
    pub replay_runs: u64,
    pub unique_decision_nodes: u64,
    pub expanded_edges: u64,
    pub transposition_hits: u64,
    pub total_leaves: u64,
    pub learner_win_leaves: u64,
    pub opponent_win_leaves: u64,
    pub draw_leaves: u64,
    pub next_untap_leaves: u64,
    pub turn_limit_leaves: u64,
    pub search_limit_leaves: u64,
    pub engine_safety_limit_leaves: u64,
    pub maximum_observed_depth: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PunchingBagBenchmarkReport {
    pub schema_version: &'static str,
    pub learner_deck: String,
    pub punching_bag: PunchingBagSpecification,
    pub discovery: WinningPositionReport,
    pub tree: UntapTreeReport,
    pub root_state: GameState,
    pub root_decision: EngineDecisionRequest,
    pub root_choices: Vec<RootChoiceReport>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressivePunchingBagSegmentReport {
    pub segment_index: usize,
    pub root_turn: u32,
    pub root_step: GameStep,
    pub root_decision_id: String,
    pub tree: UntapTreeReport,
    pub root_choices: Vec<RootChoiceReport>,
    pub selected_leaf_outcome: String,
    pub selected_leaf_choice_count: usize,
    pub selected_leaf: Vec<ScenarioDecisionChoice>,
    pub next_root_turn: Option<u32>,
    pub minimum_opponent_library_size: usize,
    pub maximum_observed_mill_count: usize,
    pub maximum_observed_mana_spent: usize,
    pub draw_failed_leaf_count: u64,
    pub draw_skipped_leaf_count: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressivePunchingBagReport {
    pub schema_version: &'static str,
    pub learner_deck: String,
    pub punching_bag: PunchingBagSpecification,
    pub seed: u64,
    pub opening_hand_size: usize,
    pub mulligan_enabled: bool,
    pub mulligans_taken: usize,
    pub prior_choice_count: usize,
    pub segments_explored: usize,
    pub winning_segment_index: usize,
    pub winning_turn: u32,
    pub winning_outcome: GameOutcome,
    pub winning_witness_source: String,
    pub progression: Vec<ProgressivePunchingBagSegmentReport>,
    pub known_winning_line: Vec<ScenarioDecisionChoice>,
    pub tree: UntapTreeReport,
    pub root_state: GameState,
    pub root_decision: EngineDecisionRequest,
    pub root_choices: Vec<RootChoiceReport>,
}

#[derive(Clone, Debug)]
pub struct PunchingBagScenarioGenerationConfig {
    pub scenario_count: usize,
    pub maximum_generation_attempts: usize,
    pub benchmark: PunchingBagBenchmarkConfig,
}

impl Default for PunchingBagScenarioGenerationConfig {
    fn default() -> Self {
        Self {
            scenario_count: 1,
            maximum_generation_attempts: 4,
            benchmark: PunchingBagBenchmarkConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PunchingBagScenarioSession {
    pub schema_version: &'static str,
    pub state: GameState,
    pub decision: EngineDecisionRequest,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedPunchingBagScenario {
    pub schema_version: &'static str,
    pub id: String,
    pub tags: Vec<String>,
    pub learner_deck: PlayerDeck,
    pub punching_bag: PunchingBagSpecification,
    pub discovery: WinningPositionReport,
    pub initial_session: PunchingBagScenarioSession,
    pub root_choices: Vec<RootChoiceReport>,
    pub tree: UntapTreeReport,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PunchingBagScenarioDataset {
    pub schema_version: &'static str,
    pub generation_seed: u64,
    pub requested_scenario_count: usize,
    pub generated_scenario_count: usize,
    pub generation_attempt_count: usize,
    pub incomplete_scenario_count: usize,
    pub generation_elapsed_ms: u128,
    pub scenarios: Vec<GeneratedPunchingBagScenario>,
}

#[derive(Clone, Debug)]
enum ScriptedChoice {
    Action {
        decision_id: String,
        action_id: String,
    },
    Number {
        decision_id: String,
        value: i32,
    },
}

impl ScriptedChoice {
    fn label(&self) -> String {
        match self {
            Self::Action { action_id, .. } => action_id.clone(),
            Self::Number { value, .. } => value.to_string(),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Action { .. } => "action",
            Self::Number { .. } => "number",
        }
    }
}

#[derive(Clone, Debug)]
struct RecordedChoice {
    choice: ScriptedChoice,
    player_id: String,
    active_player_id: String,
    turn_number: u32,
    step: GameStep,
}

impl RecordedChoice {
    fn scenario_choice(&self) -> ScenarioDecisionChoice {
        ScenarioDecisionChoice {
            decision_id: match &self.choice {
                ScriptedChoice::Action { decision_id, .. }
                | ScriptedChoice::Number { decision_id, .. } => decision_id.clone(),
            },
            player_id: self.player_id.clone(),
            active_player_id: self.active_player_id.clone(),
            turn_number: self.turn_number,
            step: self.step.clone(),
            choice_kind: self.choice.kind().to_string(),
            choice: self.choice.label(),
        }
    }
}

struct RecordingRandomProvider {
    rng: StdRng,
    choices: Vec<RecordedChoice>,
}

impl RecordingRandomProvider {
    fn seeded(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            choices: Vec::new(),
        }
    }

    fn active_player_id(state: &GameState) -> String {
        state.players[state.active_player].id.clone()
    }
}

impl DecisionProvider for RecordingRandomProvider {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.options.is_empty() {
            return Err(EngineError::new("decision request has no legal options"));
        }
        let selected = self.rng.gen_range(0..request.options.len());
        self.choices.push(RecordedChoice {
            choice: ScriptedChoice::Action {
                decision_id: request.id.clone(),
                action_id: request.options[selected].id.clone(),
            },
            player_id: request.player_id.clone(),
            active_player_id: Self::active_player_id(state),
            turn_number: state.turn_number,
            step: state.step.clone(),
        });
        Ok(selected)
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let Some((minimum, maximum)) = decision_number_bounds(request) else {
            return Err(EngineError::new(format!(
                "decision {} is not a number selection",
                request.id
            )));
        };
        let selected = self.rng.gen_range(minimum..=maximum);
        self.choices.push(RecordedChoice {
            choice: ScriptedChoice::Number {
                decision_id: request.id.clone(),
                value: selected,
            },
            player_id: request.player_id.clone(),
            active_player_id: Self::active_player_id(state),
            turn_number: state.turn_number,
            step: state.step.clone(),
        });
        Ok(selected)
    }
}

struct ReplayThenRandomProvider<'a> {
    prefix: &'a [ScriptedChoice],
    cursor: usize,
    rng: StdRng,
    choices: Vec<RecordedChoice>,
}

impl<'a> ReplayThenRandomProvider<'a> {
    fn seeded(prefix: &'a [ScriptedChoice], seed: u64) -> Self {
        Self {
            prefix,
            cursor: 0,
            rng: StdRng::seed_from_u64(seed),
            choices: Vec::new(),
        }
    }

    fn next_scripted(&mut self) -> Option<ScriptedChoice> {
        let choice = self.prefix.get(self.cursor).cloned();
        if choice.is_some() {
            self.cursor += 1;
        }
        choice
    }

    fn record(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
        choice: ScriptedChoice,
    ) {
        self.choices.push(RecordedChoice {
            choice,
            player_id: request.player_id.clone(),
            active_player_id: RecordingRandomProvider::active_player_id(state),
            turn_number: state.turn_number,
            step: state.step.clone(),
        });
    }
}

impl DecisionProvider for ReplayThenRandomProvider<'_> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.options.is_empty() {
            return Err(EngineError::new("decision request has no legal options"));
        }
        let (selected, choice) = if let Some(choice) = self.next_scripted() {
            let ScriptedChoice::Action {
                decision_id,
                action_id,
            } = &choice
            else {
                return Err(EngineError::new(format!(
                    "replay expected a number but reached action decision {}",
                    request.id
                )));
            };
            if decision_id != &request.id {
                return Err(EngineError::new(format!(
                    "replay decision mismatch: expected {decision_id}, received {}",
                    request.id
                )));
            }
            let selected = request
                .options
                .iter()
                .position(|option| option.id == *action_id)
                .ok_or_else(|| {
                    EngineError::new(format!(
                        "replay action {action_id} is absent from decision {}",
                        request.id
                    ))
                })?;
            (selected, choice)
        } else {
            let selected = self.rng.gen_range(0..request.options.len());
            (
                selected,
                ScriptedChoice::Action {
                    decision_id: request.id.clone(),
                    action_id: request.options[selected].id.clone(),
                },
            )
        };
        self.record(state, request, choice);
        Ok(selected)
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let Some((minimum, maximum)) = decision_number_bounds(request) else {
            return Err(EngineError::new(format!(
                "decision {} is not a number selection",
                request.id
            )));
        };
        let (selected, choice) = if let Some(choice) = self.next_scripted() {
            let ScriptedChoice::Number { decision_id, value } = &choice else {
                return Err(EngineError::new(format!(
                    "replay expected an action but reached number decision {}",
                    request.id
                )));
            };
            if decision_id != &request.id {
                return Err(EngineError::new(format!(
                    "replay decision mismatch: expected {decision_id}, received {}",
                    request.id
                )));
            }
            if !(minimum..=maximum).contains(value) {
                return Err(EngineError::new(format!(
                    "replay number {value} is outside {minimum}..={maximum} for {}",
                    request.id
                )));
            }
            (*value, choice)
        } else {
            let selected = self.rng.gen_range(minimum..=maximum);
            (
                selected,
                ScriptedChoice::Number {
                    decision_id: request.id.clone(),
                    value: selected,
                },
            )
        };
        self.record(state, request, choice);
        Ok(selected)
    }
}

#[derive(Clone, Debug)]
struct PendingBranch {
    state: GameState,
    rng_fingerprint: u64,
    request: EngineDecisionRequest,
    decision_id: String,
    player_id: String,
    choices: Vec<ScriptedChoice>,
}

struct BranchingReplayProvider<'a> {
    script: &'a [ScriptedChoice],
    cursor: usize,
    pending: Option<PendingBranch>,
}

impl<'a> BranchingReplayProvider<'a> {
    fn new(script: &'a [ScriptedChoice]) -> Self {
        Self {
            script,
            cursor: 0,
            pending: None,
        }
    }

    fn next_scripted(&mut self) -> Option<ScriptedChoice> {
        let choice = self.script.get(self.cursor).cloned();
        if choice.is_some() {
            self.cursor += 1;
        }
        choice
    }
}

impl DecisionProvider for BranchingReplayProvider<'_> {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(choice) = self.next_scripted() {
            let ScriptedChoice::Action {
                decision_id,
                action_id,
            } = choice
            else {
                return Err(EngineError::new(format!(
                    "replay expected a number but reached action decision {}",
                    request.id
                )));
            };
            if decision_id != request.id {
                return Err(EngineError::new(format!(
                    "replay decision mismatch: expected {decision_id}, received {}",
                    request.id
                )));
            }
            return request
                .options
                .iter()
                .position(|option| option.id == action_id)
                .ok_or_else(|| {
                    EngineError::new(format!(
                        "replay action {action_id} is absent from decision {}",
                        request.id
                    ))
                });
        }

        self.pending = Some(PendingBranch {
            state: state.clone(),
            rng_fingerprint: 0,
            request: request.clone(),
            decision_id: request.id.clone(),
            player_id: request.player_id.clone(),
            choices: request
                .options
                .iter()
                .map(|option| ScriptedChoice::Action {
                    decision_id: request.id.clone(),
                    action_id: option.id.clone(),
                })
                .collect(),
        });
        Err(EngineError::new(SEARCH_BRANCH_SIGNAL))
    }

    fn choose_number(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        if let Some(choice) = self.next_scripted() {
            let ScriptedChoice::Number { decision_id, value } = choice else {
                return Err(EngineError::new(format!(
                    "replay expected an action but reached number decision {}",
                    request.id
                )));
            };
            if decision_id != request.id {
                return Err(EngineError::new(format!(
                    "replay decision mismatch: expected {decision_id}, received {}",
                    request.id
                )));
            }
            let Some((minimum, maximum)) = decision_number_bounds(request) else {
                return Err(EngineError::new(format!(
                    "decision {} is not a number selection",
                    request.id
                )));
            };
            if !(minimum..=maximum).contains(&value) {
                return Err(EngineError::new(format!(
                    "replay number {value} is outside {minimum}..={maximum} for {}",
                    request.id
                )));
            }
            return Ok(value);
        }

        let Some((minimum, maximum)) = decision_number_bounds(request) else {
            return Err(EngineError::new(format!(
                "decision {} is not a number selection",
                request.id
            )));
        };
        self.pending = Some(PendingBranch {
            state: state.clone(),
            rng_fingerprint: 0,
            request: request.clone(),
            decision_id: request.id.clone(),
            player_id: request.player_id.clone(),
            choices: (minimum..=maximum)
                .map(|value| ScriptedChoice::Number {
                    decision_id: request.id.clone(),
                    value,
                })
                .collect(),
        });
        Err(EngineError::new(SEARCH_BRANCH_SIGNAL))
    }
}

#[derive(Clone, Debug, Default)]
struct TreeCounts {
    complete: bool,
    total_leaves: u64,
    learner_win_leaves: u64,
    opponent_win_leaves: u64,
    draw_leaves: u64,
    next_untap_leaves: u64,
    turn_limit_leaves: u64,
    search_limit_leaves: u64,
    engine_safety_limit_leaves: u64,
}

impl TreeCounts {
    fn complete_leaf() -> Self {
        Self {
            complete: true,
            total_leaves: 1,
            ..Self::default()
        }
    }

    fn search_limit_leaf() -> Self {
        Self {
            complete: false,
            total_leaves: 1,
            search_limit_leaves: 1,
            ..Self::default()
        }
    }

    fn merge(&mut self, other: &Self) {
        self.complete &= other.complete;
        self.total_leaves = self.total_leaves.saturating_add(other.total_leaves);
        self.learner_win_leaves = self
            .learner_win_leaves
            .saturating_add(other.learner_win_leaves);
        self.opponent_win_leaves = self
            .opponent_win_leaves
            .saturating_add(other.opponent_win_leaves);
        self.draw_leaves = self.draw_leaves.saturating_add(other.draw_leaves);
        self.next_untap_leaves = self
            .next_untap_leaves
            .saturating_add(other.next_untap_leaves);
        self.turn_limit_leaves = self
            .turn_limit_leaves
            .saturating_add(other.turn_limit_leaves);
        self.search_limit_leaves = self
            .search_limit_leaves
            .saturating_add(other.search_limit_leaves);
        self.engine_safety_limit_leaves = self
            .engine_safety_limit_leaves
            .saturating_add(other.engine_safety_limit_leaves);
    }

    fn root_choice_report(&self, choice: &ScriptedChoice) -> RootChoiceReport {
        RootChoiceReport {
            choice_kind: choice.kind().to_string(),
            choice: choice.label(),
            complete: self.complete,
            total_leaves: self.total_leaves,
            learner_win_leaves: self.learner_win_leaves,
            opponent_win_leaves: self.opponent_win_leaves,
            draw_leaves: self.draw_leaves,
            next_untap_leaves: self.next_untap_leaves,
            search_limit_leaves: self.search_limit_leaves,
            engine_safety_limit_leaves: self.engine_safety_limit_leaves,
            learner_win_leaf_fraction: if self.total_leaves == 0 {
                0.0
            } else {
                self.learner_win_leaves as f64 / self.total_leaves as f64
            },
        }
    }
}

struct SearchContext<'a> {
    setup: &'a GameSetup,
    seed: u64,
    learner_player_id: &'a str,
    punching_bag_player_id: &'a str,
    position: Option<&'a PunchingBagPosition>,
    prefix: &'a [ScriptedChoice],
    root_turn: u32,
    root_event_count: usize,
    limits: &'a PunchingBagBenchmarkConfig,
    memo: HashMap<u64, TreeCounts>,
    replay_runs: u64,
    unique_decision_nodes: u64,
    expanded_edges: u64,
    transposition_hits: u64,
    maximum_observed_depth: usize,
    root_state: Option<GameState>,
    root_decision: Option<EngineDecisionRequest>,
    root_choices: Vec<RootChoiceReport>,
    winning_branch: Option<Vec<ScriptedChoice>>,
    minimum_opponent_library_size: usize,
    maximum_observed_mill_count: usize,
    maximum_observed_mana_spent: usize,
    draw_failed_leaf_count: u64,
    draw_skipped_leaf_count: u64,
}

impl SearchContext<'_> {
    fn replay(&mut self, branch: &[ScriptedChoice]) -> Result<ReplayResult, EngineError> {
        self.replay_runs = self.replay_runs.saturating_add(1);
        let mut script = Vec::with_capacity(self.prefix.len() + branch.len());
        script.extend_from_slice(self.prefix);
        script.extend_from_slice(branch);
        let mut provider = BranchingReplayProvider::new(&script);
        let mut engine = GameEngine::new(self.setup.clone(), self.seed)?;
        engine.configure_punching_bag(self.punching_bag_player_id)?;
        if let Some(position) = self.position {
            engine.configure_punching_bag_position(
                self.learner_player_id,
                &position.learner_battlefield_definition_ids,
                &position.learner_hand_definition_ids,
                self.punching_bag_player_id,
                &position.opponent_battlefield,
                position.opponent_library_size,
                position.opponent_skips_draw_step,
            )?;
        }
        engine.record_opening_hands();
        let result = engine.run_until_next_untap(
            &mut provider,
            self.learner_player_id,
            self.root_turn,
            self.limits.maximum_turns,
        );
        match result {
            Err(error) if error.to_string() == SEARCH_BRANCH_SIGNAL => {
                let mut pending = provider
                    .pending
                    .take()
                    .ok_or_else(|| EngineError::new("branch signal had no pending decision"))?;
                pending.rng_fingerprint = engine.rng_fingerprint_for_search();
                Ok(ReplayResult::Decision(pending))
            }
            Err(error) => Err(error),
            Ok(boundary) => Ok(ReplayResult::Leaf {
                boundary,
                state: engine.state().clone(),
            }),
        }
    }

    fn explore(&mut self, branch: &mut Vec<ScriptedChoice>) -> Result<TreeCounts, EngineError> {
        self.maximum_observed_depth = self.maximum_observed_depth.max(branch.len());
        match self.replay(branch)? {
            ReplayResult::Leaf { boundary, state } => {
                if let Some(opponent) = state
                    .players
                    .iter()
                    .find(|player| player.id == self.punching_bag_player_id)
                {
                    self.minimum_opponent_library_size = self
                        .minimum_opponent_library_size
                        .min(opponent.library.len());
                }
                let mut branch_events = state.events.iter().skip(self.root_event_count);
                self.maximum_observed_mill_count = self.maximum_observed_mill_count.max(
                    branch_events
                        .clone()
                        .filter(|event| {
                            event.kind == "cardMilled"
                                && event.player_id.as_deref() == Some(self.punching_bag_player_id)
                        })
                        .count(),
                );
                self.maximum_observed_mana_spent = self.maximum_observed_mana_spent.max(
                    branch_events
                        .clone()
                        .filter(|event| event.kind == "spellCast")
                        .map(|event| {
                            let decisions = &event.detail["decisions"];
                            let activated = decisions["manaPayment"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter(|payment| {
                                    payment["kind"].as_str() != Some("phyrexianManaPayment")
                                })
                                .map(|payment| {
                                    payment["mana"].as_array().map(Vec::len).unwrap_or(0)
                                })
                                .sum::<usize>();
                            let pooled = decisions["manaPoolPayment"]
                                .as_array()
                                .map(Vec::len)
                                .unwrap_or(0);
                            activated.saturating_add(pooled)
                        })
                        .max()
                        .unwrap_or(0),
                );
                if branch_events
                    .clone()
                    .any(|event| event.kind == "drawFailedEmptyLibrary")
                {
                    self.draw_failed_leaf_count = self.draw_failed_leaf_count.saturating_add(1);
                }
                if branch_events.any(|event| event.kind == "drawStepSkipped") {
                    self.draw_skipped_leaf_count = self.draw_skipped_leaf_count.saturating_add(1);
                }
                let mut counts = TreeCounts::complete_leaf();
                let engine_safety_limit = state
                    .events
                    .iter()
                    .skip(self.root_event_count)
                    .any(|event| event.kind == "decisionLoopTurnEndRequested");
                if engine_safety_limit {
                    counts.complete = false;
                    counts.engine_safety_limit_leaves = 1;
                }
                match boundary {
                    UntapBoundaryStatus::Reached => counts.next_untap_leaves = 1,
                    UntapBoundaryStatus::TurnLimitReached => {
                        counts.complete = false;
                        counts.turn_limit_leaves = 1;
                    }
                    UntapBoundaryStatus::GameEnded => {
                        match state
                            .outcome
                            .as_ref()
                            .and_then(|outcome| outcome.winner.as_deref())
                        {
                            Some(winner) if winner == self.learner_player_id => {
                                counts.learner_win_leaves = 1;
                                if self.winning_branch.is_none() {
                                    self.winning_branch = Some(branch.clone());
                                }
                            }
                            Some(_) => counts.opponent_win_leaves = 1,
                            None => counts.draw_leaves = 1,
                        }
                    }
                }
                Ok(counts)
            }
            ReplayResult::Decision(pending) => {
                if self.root_state.is_none() && branch.is_empty() {
                    self.root_event_count = pending.state.events.len();
                    self.root_state = Some(pending.state.clone());
                    self.root_decision = Some(pending.request.clone());
                }
                if branch.len() >= self.limits.maximum_depth
                    || pending.choices.len() > self.limits.maximum_choices_per_node
                    || usize::try_from(self.unique_decision_nodes).unwrap_or(usize::MAX)
                        >= self.limits.maximum_unique_nodes
                {
                    return Ok(TreeCounts::search_limit_leaf());
                }
                let key = decision_node_key(&pending, branch.len());
                if let Some(cached) = self.memo.get(&key) {
                    self.transposition_hits = self.transposition_hits.saturating_add(1);
                    return Ok(cached.clone());
                }
                self.unique_decision_nodes = self.unique_decision_nodes.saturating_add(1);
                self.expanded_edges = self
                    .expanded_edges
                    .saturating_add(pending.choices.len() as u64);
                let mut totals = TreeCounts {
                    complete: true,
                    ..TreeCounts::default()
                };
                let is_root = branch.is_empty();
                for choice in pending.choices {
                    branch.push(choice.clone());
                    let child = self.explore(branch)?;
                    branch.pop();
                    if is_root {
                        self.root_choices.push(child.root_choice_report(&choice));
                    }
                    totals.merge(&child);
                }
                self.memo.insert(key, totals.clone());
                Ok(totals)
            }
        }
    }
}

enum ReplayResult {
    Decision(PendingBranch),
    Leaf {
        boundary: UntapBoundaryStatus,
        state: GameState,
    },
}

fn decision_node_key(pending: &PendingBranch, depth: usize) -> u64 {
    let mut state = pending.state.clone();
    state.events.clear();
    let mut hasher = DefaultHasher::new();
    serde_json::to_string(&state)
        .unwrap_or_default()
        .hash(&mut hasher);
    pending.decision_id.hash(&mut hasher);
    pending.player_id.hash(&mut hasher);
    pending.rng_fingerprint.hash(&mut hasher);
    depth.hash(&mut hasher);
    for choice in &pending.choices {
        choice.label().hash(&mut hasher);
    }
    hasher.finish()
}

pub fn punching_bag_deck(starting_life: i32) -> PlayerDeck {
    let cards = (0..99)
        .map(|index| {
            let (suffix, type_line, power, toughness) = match index % 3 {
                0 => ("artifact", "Artifact", None, None),
                1 => (
                    "creature",
                    "Creature — Construct",
                    Some("0".to_string()),
                    Some("1".to_string()),
                ),
                _ => ("enchantment", "Enchantment", None, None),
            };
            CardDefinition {
                id: format!("punching-bag-{suffix}"),
                name: format!("Punching Bag {suffix}"),
                type_line: type_line.to_string(),
                is_commander: false,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                mana_cost: "{0}".to_string(),
                power,
                toughness,
                rules: Vec::new(),
            }
        })
        .collect();
    PlayerDeck {
        id: PUNCHING_BAG_PLAYER_ID.to_string(),
        name: "Punching Bag".to_string(),
        starting_life,
        cards,
    }
}

fn scenario_setup(learner_deck: PlayerDeck) -> GameSetup {
    let starting_life = learner_deck.starting_life.max(1);
    GameSetup {
        players: vec![learner_deck, punching_bag_deck(starting_life)],
        opening_hand_size: 7,
        starting_player: 0,
    }
}

fn punching_bag_specification() -> PunchingBagSpecification {
    PunchingBagSpecification {
        player_id: PUNCHING_BAG_PLAYER_ID.to_string(),
        initial_hand_size: 0,
        library_card_count: 99,
        card_types: vec![
            "Artifact".to_string(),
            "Creature 0/1".to_string(),
            "Enchantment".to_string(),
        ],
        mana_cost: "{0}".to_string(),
        creature_power: 0,
        creature_toughness: 1,
        vitality_period: 3,
        supply_period: 7,
    }
}

pub fn benchmark_punching_bag_tree(
    learner_deck: PlayerDeck,
    config: PunchingBagBenchmarkConfig,
) -> Result<PunchingBagBenchmarkReport, EngineError> {
    if config.maximum_random_games == 0 {
        return Err(EngineError::new("maximum_random_games must be positive"));
    }
    let learner_name = learner_deck.name.clone();
    let learner_player_id = learner_deck.id.clone();
    let setup = scenario_setup(learner_deck);
    let discovery_started = Instant::now();
    let mut winning_trace = None;

    for game_index in 0..config.maximum_random_games {
        let game_seed = config
            .seed
            .wrapping_add((game_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut engine = GameEngine::new(setup.clone(), game_seed)?;
        engine.configure_punching_bag(PUNCHING_BAG_PLAYER_ID)?;
        engine.record_opening_hands();
        let mut provider = RecordingRandomProvider::seeded(game_seed ^ 0xD1B5_4A32_D192_ED03);
        let status = engine.run(&mut provider, config.maximum_turns)?;
        let learner_won = status == GameStatus::Completed
            && engine
                .state()
                .outcome
                .as_ref()
                .and_then(|outcome| outcome.winner.as_deref())
                == Some(learner_player_id.as_str());
        if learner_won {
            winning_trace = Some((
                game_seed,
                game_index + 1,
                engine.state().turn_number,
                provider.choices,
            ));
            break;
        }
    }

    let Some((winning_seed, games_attempted, winning_turn, choices)) = winning_trace else {
        return Err(EngineError::new(format!(
            "no random learner victory found in {} games",
            config.maximum_random_games
        )));
    };
    let root_turn = match config.search_root {
        PunchingBagSearchRoot::OpeningDecision => choices
            .iter()
            .find(|choice| choice.active_player_id == learner_player_id)
            .map(|choice| choice.turn_number),
        PunchingBagSearchRoot::WinningTurnUntap => choices
            .iter()
            .filter(|choice| choice.active_player_id == learner_player_id)
            .map(|choice| choice.turn_number)
            .max(),
    }
    .ok_or_else(|| EngineError::new("winning trace has no learner turn"))?;
    let root_index = choices
        .iter()
        .position(|choice| {
            choice.turn_number == root_turn && choice.active_player_id == learner_player_id
        })
        .ok_or_else(|| EngineError::new("winning trace has no decision at the search root"))?;
    let root = &choices[root_index];
    let prefix = choices[..root_index]
        .iter()
        .map(|choice| choice.choice.clone())
        .collect::<Vec<_>>();
    let discovery = WinningPositionReport {
        seed: winning_seed,
        games_attempted,
        discovery_elapsed_ms: discovery_started.elapsed().as_millis(),
        winning_turn,
        root_turn: root.turn_number,
        root_step: root.step.clone(),
        root_player_id: root.active_player_id.clone(),
        root_decision_id: match &root.choice {
            ScriptedChoice::Action { decision_id, .. }
            | ScriptedChoice::Number { decision_id, .. } => decision_id.clone(),
        },
        recorded_winning_choice: root.choice.label(),
        prefix_choice_count: prefix.len(),
        known_winning_suffix_choice_count: choices.len() - root_index,
        known_winning_line: choices[root_index..]
            .iter()
            .map(|choice| ScenarioDecisionChoice {
                decision_id: match &choice.choice {
                    ScriptedChoice::Action { decision_id, .. }
                    | ScriptedChoice::Number { decision_id, .. } => decision_id.clone(),
                },
                player_id: choice.player_id.clone(),
                active_player_id: choice.active_player_id.clone(),
                turn_number: choice.turn_number,
                step: choice.step.clone(),
                choice_kind: choice.choice.kind().to_string(),
                choice: choice.choice.label(),
            })
            .collect(),
    };

    let search_started = Instant::now();
    let mut context = SearchContext {
        setup: &setup,
        seed: winning_seed,
        learner_player_id: &learner_player_id,
        punching_bag_player_id: PUNCHING_BAG_PLAYER_ID,
        position: None,
        prefix: &prefix,
        root_turn: root.turn_number,
        root_event_count: 0,
        limits: &config,
        memo: HashMap::new(),
        replay_runs: 0,
        unique_decision_nodes: 0,
        expanded_edges: 0,
        transposition_hits: 0,
        maximum_observed_depth: 0,
        root_state: None,
        root_decision: None,
        root_choices: Vec::new(),
        winning_branch: None,
        minimum_opponent_library_size: usize::MAX,
        maximum_observed_mill_count: 0,
        maximum_observed_mana_spent: 0,
        draw_failed_leaf_count: 0,
        draw_skipped_leaf_count: 0,
    };
    let counts = context.explore(&mut Vec::new())?;
    let root_state = context
        .root_state
        .ok_or_else(|| EngineError::new("search did not reach its root decision"))?;
    let root_decision = context
        .root_decision
        .ok_or_else(|| EngineError::new("search did not capture its root decision"))?;
    let tree = UntapTreeReport {
        complete: counts.complete,
        exact_for_fixed_random_tape: counts.complete,
        elapsed_ms: search_started.elapsed().as_millis(),
        replay_runs: context.replay_runs,
        unique_decision_nodes: context.unique_decision_nodes,
        expanded_edges: context.expanded_edges,
        transposition_hits: context.transposition_hits,
        total_leaves: counts.total_leaves,
        learner_win_leaves: counts.learner_win_leaves,
        opponent_win_leaves: counts.opponent_win_leaves,
        draw_leaves: counts.draw_leaves,
        next_untap_leaves: counts.next_untap_leaves,
        turn_limit_leaves: counts.turn_limit_leaves,
        search_limit_leaves: counts.search_limit_leaves,
        engine_safety_limit_leaves: counts.engine_safety_limit_leaves,
        maximum_observed_depth: context.maximum_observed_depth,
    };

    Ok(PunchingBagBenchmarkReport {
        schema_version: "mtg-punching-bag-benchmark/v1",
        learner_deck: learner_name,
        punching_bag: punching_bag_specification(),
        discovery,
        tree,
        root_state,
        root_decision,
        root_choices: context.root_choices,
    })
}

struct ProgressiveSegmentSearch {
    root_state: GameState,
    root_decision: EngineDecisionRequest,
    root_choices: Vec<RootChoiceReport>,
    tree: UntapTreeReport,
    winning_branch: Option<Vec<ScriptedChoice>>,
    minimum_opponent_library_size: usize,
    maximum_observed_mill_count: usize,
    maximum_observed_mana_spent: usize,
    draw_failed_leaf_count: u64,
    draw_skipped_leaf_count: u64,
}

fn search_progressive_segment(
    setup: &GameSetup,
    learner_player_id: &str,
    seed: u64,
    prefix: &[ScriptedChoice],
    root_turn: u32,
    config: &ProgressivePunchingBagConfig,
) -> Result<ProgressiveSegmentSearch, EngineError> {
    let limits = PunchingBagBenchmarkConfig {
        seed,
        search_root: PunchingBagSearchRoot::OpeningDecision,
        maximum_random_games: 1,
        maximum_turns: config.maximum_turns,
        maximum_unique_nodes: config.maximum_unique_nodes,
        maximum_depth: config.maximum_depth,
        maximum_choices_per_node: config.maximum_choices_per_node,
    };
    let search_started = Instant::now();
    let mut context = SearchContext {
        setup,
        seed,
        learner_player_id,
        punching_bag_player_id: PUNCHING_BAG_PLAYER_ID,
        position: config.position.as_ref(),
        prefix,
        root_turn,
        root_event_count: 0,
        limits: &limits,
        memo: HashMap::new(),
        replay_runs: 0,
        unique_decision_nodes: 0,
        expanded_edges: 0,
        transposition_hits: 0,
        maximum_observed_depth: 0,
        root_state: None,
        root_decision: None,
        root_choices: Vec::new(),
        winning_branch: None,
        minimum_opponent_library_size: usize::MAX,
        maximum_observed_mill_count: 0,
        maximum_observed_mana_spent: 0,
        draw_failed_leaf_count: 0,
        draw_skipped_leaf_count: 0,
    };
    let counts = context.explore(&mut Vec::new())?;
    let tree = UntapTreeReport {
        complete: counts.complete,
        exact_for_fixed_random_tape: counts.complete,
        elapsed_ms: search_started.elapsed().as_millis(),
        replay_runs: context.replay_runs,
        unique_decision_nodes: context.unique_decision_nodes,
        expanded_edges: context.expanded_edges,
        transposition_hits: context.transposition_hits,
        total_leaves: counts.total_leaves,
        learner_win_leaves: counts.learner_win_leaves,
        opponent_win_leaves: counts.opponent_win_leaves,
        draw_leaves: counts.draw_leaves,
        next_untap_leaves: counts.next_untap_leaves,
        turn_limit_leaves: counts.turn_limit_leaves,
        search_limit_leaves: counts.search_limit_leaves,
        engine_safety_limit_leaves: counts.engine_safety_limit_leaves,
        maximum_observed_depth: context.maximum_observed_depth,
    };
    Ok(ProgressiveSegmentSearch {
        root_state: context
            .root_state
            .ok_or_else(|| EngineError::new("segment search did not reach its root decision"))?,
        root_decision: context
            .root_decision
            .ok_or_else(|| EngineError::new("segment search did not capture its root decision"))?,
        root_choices: context.root_choices,
        tree,
        winning_branch: context.winning_branch,
        minimum_opponent_library_size: context.minimum_opponent_library_size,
        maximum_observed_mill_count: context.maximum_observed_mill_count,
        maximum_observed_mana_spent: context.maximum_observed_mana_spent,
        draw_failed_leaf_count: context.draw_failed_leaf_count,
        draw_skipped_leaf_count: context.draw_skipped_leaf_count,
    })
}

struct ReplayedSegment {
    boundary: UntapBoundaryStatus,
    state: GameState,
    all_choices: Vec<RecordedChoice>,
}

fn replay_segment(
    setup: &GameSetup,
    seed: u64,
    learner_player_id: &str,
    root_turn: u32,
    maximum_turns: u32,
    scripted_prefix: &[ScriptedChoice],
    random_seed: u64,
    position: Option<&PunchingBagPosition>,
) -> Result<ReplayedSegment, EngineError> {
    let mut engine = GameEngine::new(setup.clone(), seed)?;
    engine.configure_punching_bag(PUNCHING_BAG_PLAYER_ID)?;
    if let Some(position) = position {
        engine.configure_punching_bag_position(
            learner_player_id,
            &position.learner_battlefield_definition_ids,
            &position.learner_hand_definition_ids,
            PUNCHING_BAG_PLAYER_ID,
            &position.opponent_battlefield,
            position.opponent_library_size,
            position.opponent_skips_draw_step,
        )?;
    }
    engine.record_opening_hands();
    let mut provider = ReplayThenRandomProvider::seeded(scripted_prefix, random_seed);
    let boundary =
        engine.run_until_next_untap(&mut provider, learner_player_id, root_turn, maximum_turns)?;
    if provider.cursor < scripted_prefix.len() {
        return Err(EngineError::new(format!(
            "segment replay consumed {} of {} scripted choices",
            provider.cursor,
            scripted_prefix.len()
        )));
    }
    Ok(ReplayedSegment {
        boundary,
        state: engine.state().clone(),
        all_choices: provider.choices,
    })
}

fn learner_won(state: &GameState, learner_player_id: &str) -> bool {
    state
        .outcome
        .as_ref()
        .and_then(|outcome| outcome.winner.as_deref())
        == Some(learner_player_id)
}

fn recorded_suffix(
    choices: &[RecordedChoice],
    prefix_choice_count: usize,
) -> Vec<ScenarioDecisionChoice> {
    choices[prefix_choice_count.min(choices.len())..]
        .iter()
        .map(RecordedChoice::scenario_choice)
        .collect()
}

fn scripted_choices(choices: &[RecordedChoice]) -> Vec<ScriptedChoice> {
    choices.iter().map(|choice| choice.choice.clone()).collect()
}

pub fn progressively_find_punching_bag_win(
    learner_deck: PlayerDeck,
    config: ProgressivePunchingBagConfig,
) -> Result<ProgressivePunchingBagReport, EngineError> {
    if config.maximum_segments == 0 {
        return Err(EngineError::new("maximum_segments must be positive"));
    }
    let learner_name = learner_deck.name.clone();
    let learner_player_id = learner_deck.id.clone();
    let setup = scenario_setup(learner_deck);
    let mut prefix = Vec::<ScriptedChoice>::new();
    let mut root_turn = 1_u32;
    let mut progression = Vec::<ProgressivePunchingBagSegmentReport>::new();

    for segment_index in 0..config.maximum_segments {
        let segment = search_progressive_segment(
            &setup,
            &learner_player_id,
            config.seed,
            &prefix,
            root_turn,
            &config,
        )?;
        let prior_choice_count = prefix.len();
        let root_step = segment.root_state.step.clone();
        let root_decision_id = segment.root_decision.id.clone();

        if let Some(winning_branch) = segment.winning_branch.clone() {
            let mut winning_script = prefix.clone();
            winning_script.extend(winning_branch);
            let replay = replay_segment(
                &setup,
                config.seed,
                &learner_player_id,
                root_turn,
                config.maximum_turns,
                &winning_script,
                config.seed ^ 0xA5A5_5A5A_D3C1_B7E9,
                config.position.as_ref(),
            )?;
            if replay.boundary != UntapBoundaryStatus::GameEnded
                || !learner_won(&replay.state, &learner_player_id)
            {
                return Err(EngineError::new(
                    "the exhaustive winning witness did not replay to a learner victory",
                ));
            }
            let known_winning_line = recorded_suffix(&replay.all_choices, prior_choice_count);
            progression.push(ProgressivePunchingBagSegmentReport {
                segment_index,
                root_turn,
                root_step,
                root_decision_id,
                tree: segment.tree.clone(),
                root_choices: segment.root_choices.clone(),
                selected_leaf_outcome: "learnerWin".to_string(),
                selected_leaf_choice_count: known_winning_line.len(),
                selected_leaf: known_winning_line.clone(),
                next_root_turn: None,
                minimum_opponent_library_size: segment.minimum_opponent_library_size,
                maximum_observed_mill_count: segment.maximum_observed_mill_count,
                maximum_observed_mana_spent: segment.maximum_observed_mana_spent,
                draw_failed_leaf_count: segment.draw_failed_leaf_count,
                draw_skipped_leaf_count: segment.draw_skipped_leaf_count,
            });
            return Ok(ProgressivePunchingBagReport {
                schema_version: "mtg-progressive-punching-bag-benchmark/v1",
                learner_deck: learner_name,
                punching_bag: punching_bag_specification(),
                seed: config.seed,
                opening_hand_size: 7,
                mulligan_enabled: false,
                mulligans_taken: 0,
                prior_choice_count,
                segments_explored: progression.len(),
                winning_segment_index: segment_index,
                winning_turn: replay.state.turn_number,
                winning_outcome: replay
                    .state
                    .outcome
                    .clone()
                    .ok_or_else(|| EngineError::new("winning replay has no outcome"))?,
                winning_witness_source: "exhaustiveTree".to_string(),
                progression,
                known_winning_line,
                tree: segment.tree,
                root_state: segment.root_state,
                root_decision: segment.root_decision,
                root_choices: segment.root_choices,
            });
        }

        let mut selected_replay = None;
        for leaf_attempt in 0_u64..128 {
            let random_seed = config.seed
                ^ ((segment_index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                ^ leaf_attempt.wrapping_mul(0xD1B5_4A32_D192_ED03);
            let replay = replay_segment(
                &setup,
                config.seed,
                &learner_player_id,
                root_turn,
                config.maximum_turns,
                &prefix,
                random_seed,
                config.position.as_ref(),
            )?;
            if replay.boundary == UntapBoundaryStatus::Reached
                || learner_won(&replay.state, &learner_player_id)
            {
                selected_replay = Some(replay);
                break;
            }
        }
        let replay = selected_replay.ok_or_else(|| {
            EngineError::new(format!(
                "no continuable random leaf found from learner turn {root_turn}"
            ))
        })?;
        let selected_leaf = recorded_suffix(&replay.all_choices, prior_choice_count);

        if learner_won(&replay.state, &learner_player_id) {
            progression.push(ProgressivePunchingBagSegmentReport {
                segment_index,
                root_turn,
                root_step,
                root_decision_id,
                tree: segment.tree.clone(),
                root_choices: segment.root_choices.clone(),
                selected_leaf_outcome: "learnerWin".to_string(),
                selected_leaf_choice_count: selected_leaf.len(),
                selected_leaf: selected_leaf.clone(),
                next_root_turn: None,
                minimum_opponent_library_size: segment.minimum_opponent_library_size,
                maximum_observed_mill_count: segment.maximum_observed_mill_count,
                maximum_observed_mana_spent: segment.maximum_observed_mana_spent,
                draw_failed_leaf_count: segment.draw_failed_leaf_count,
                draw_skipped_leaf_count: segment.draw_skipped_leaf_count,
            });
            return Ok(ProgressivePunchingBagReport {
                schema_version: "mtg-progressive-punching-bag-benchmark/v1",
                learner_deck: learner_name,
                punching_bag: punching_bag_specification(),
                seed: config.seed,
                opening_hand_size: 7,
                mulligan_enabled: false,
                mulligans_taken: 0,
                prior_choice_count,
                segments_explored: progression.len(),
                winning_segment_index: segment_index,
                winning_turn: replay.state.turn_number,
                winning_outcome: replay
                    .state
                    .outcome
                    .clone()
                    .ok_or_else(|| EngineError::new("winning replay has no outcome"))?,
                winning_witness_source: "randomLeafAfterIncompleteSearch".to_string(),
                progression,
                known_winning_line: selected_leaf,
                tree: segment.tree,
                root_state: segment.root_state,
                root_decision: segment.root_decision,
                root_choices: segment.root_choices,
            });
        }

        let next_root_turn = replay.state.turn_number;
        progression.push(ProgressivePunchingBagSegmentReport {
            segment_index,
            root_turn,
            root_step,
            root_decision_id,
            tree: segment.tree,
            root_choices: segment.root_choices,
            selected_leaf_outcome: "nextLearnerUntap".to_string(),
            selected_leaf_choice_count: selected_leaf.len(),
            selected_leaf,
            next_root_turn: Some(next_root_turn),
            minimum_opponent_library_size: segment.minimum_opponent_library_size,
            maximum_observed_mill_count: segment.maximum_observed_mill_count,
            maximum_observed_mana_spent: segment.maximum_observed_mana_spent,
            draw_failed_leaf_count: segment.draw_failed_leaf_count,
            draw_skipped_leaf_count: segment.draw_skipped_leaf_count,
        });
        prefix = scripted_choices(&replay.all_choices);
        root_turn = next_root_turn;
    }

    let diagnostics = progression
        .iter()
        .map(|segment| {
            format!(
                "turn={} complete={} nodes={} leaves={} wins={} nextUntaps={} searchLimits={} minOpponentLibrary={} maxMill={} maxManaSpent={} drawFailedLeaves={} drawSkippedLeaves={} rootChoices={:?}",
                segment.root_turn,
                segment.tree.complete,
                segment.tree.unique_decision_nodes,
                segment.tree.total_leaves,
                segment.tree.learner_win_leaves,
                segment.tree.next_untap_leaves,
                segment.tree.search_limit_leaves,
                segment.minimum_opponent_library_size,
                segment.maximum_observed_mill_count,
                segment.maximum_observed_mana_spent,
                segment.draw_failed_leaf_count,
                segment.draw_skipped_leaf_count,
                segment
                    .root_choices
                    .iter()
                    .map(|choice| choice.choice.as_str())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Err(EngineError::new(format!(
        "no winning turn found in {} progressive segments ({diagnostics})",
        config.maximum_segments
    )))
}

fn scenario_slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    slug.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn generate_punching_bag_scenarios(
    learner_deck: PlayerDeck,
    config: PunchingBagScenarioGenerationConfig,
) -> Result<PunchingBagScenarioDataset, EngineError> {
    if config.scenario_count == 0 {
        return Err(EngineError::new("scenario_count must be positive"));
    }
    if config.maximum_generation_attempts < config.scenario_count {
        return Err(EngineError::new(
            "maximum_generation_attempts must be at least scenario_count",
        ));
    }

    let started = Instant::now();
    let generation_seed = config.benchmark.seed;
    let learner_slug = scenario_slug(&learner_deck.name);
    let mut scenarios = Vec::with_capacity(config.scenario_count);
    let mut attempt_count = 0usize;
    let mut incomplete_scenario_count = 0usize;

    while scenarios.len() < config.scenario_count
        && attempt_count < config.maximum_generation_attempts
    {
        let mut benchmark_config = config.benchmark.clone();
        benchmark_config.seed = generation_seed
            .wrapping_add((attempt_count as u64).wrapping_mul(0xA076_1D64_78BD_642F));
        attempt_count += 1;
        let report = benchmark_punching_bag_tree(learner_deck.clone(), benchmark_config)?;
        if !report.tree.complete || report.tree.learner_win_leaves == 0 {
            incomplete_scenario_count += 1;
            continue;
        }
        let scenario_index = scenarios.len() + 1;
        scenarios.push(GeneratedPunchingBagScenario {
            schema_version: "mtg-punching-bag-scenario/v1",
            id: format!(
                "punching-bag-{learner_slug}-{}-{scenario_index}",
                report.discovery.seed
            ),
            tags: vec![
                "punching-bag".to_string(),
                "winning-position".to_string(),
                "untap-horizon".to_string(),
                "full-tree".to_string(),
            ],
            learner_deck: learner_deck.clone(),
            punching_bag: report.punching_bag,
            discovery: report.discovery,
            initial_session: PunchingBagScenarioSession {
                schema_version: "mtg-game-session-snapshot/v1",
                state: report.root_state,
                decision: report.root_decision,
            },
            root_choices: report.root_choices,
            tree: report.tree,
        });
    }

    if scenarios.len() < config.scenario_count {
        return Err(EngineError::new(format!(
            "generated {} complete scenarios out of {} requested after {} attempts",
            scenarios.len(),
            config.scenario_count,
            attempt_count
        )));
    }

    Ok(PunchingBagScenarioDataset {
        schema_version: "mtg-punching-bag-scenarios/v1",
        generation_seed,
        requested_scenario_count: config.scenario_count,
        generated_scenario_count: scenarios.len(),
        generation_attempt_count: attempt_count,
        incomplete_scenario_count,
        generation_elapsed_ms: started.elapsed().as_millis(),
        scenarios,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn learner_deck() -> PlayerDeck {
        let land = CardDefinition {
            id: "training-wastes".to_string(),
            name: "Training Wastes".to_string(),
            type_line: "Basic Land — Wastes".to_string(),
            is_commander: false,
            is_token: false,
            is_game_piece: false,
            is_sideboard: false,
            mana_cost: String::new(),
            power: None,
            toughness: None,
            rules: Vec::new(),
        };
        let creature = CardDefinition {
            id: "training-finisher".to_string(),
            name: "Training Finisher".to_string(),
            type_line: "Creature — Avatar".to_string(),
            is_commander: false,
            is_token: false,
            is_game_piece: false,
            is_sideboard: false,
            mana_cost: "{0}".to_string(),
            power: Some("20".to_string()),
            toughness: Some("20".to_string()),
            rules: vec![
                json!({
                    "kind": "keywordAbility",
                    "source": { "kind": "self" },
                    "ability": { "kind": "flying" },
                }),
                json!({
                    "kind": "keywordAbility",
                    "source": { "kind": "self" },
                    "ability": { "kind": "haste" },
                }),
            ],
        };
        let mut cards = vec![land; 6];
        cards.push(creature);
        PlayerDeck {
            id: "learner".to_string(),
            name: "Search Fixture".to_string(),
            starting_life: 20,
            cards,
        }
    }

    #[test]
    fn punching_bag_has_no_hand_and_two_clocks() {
        let setup = scenario_setup(learner_deck());
        let mut engine = GameEngine::new(setup, 17).unwrap();
        engine
            .configure_punching_bag(PUNCHING_BAG_PLAYER_ID)
            .unwrap();
        let bag = engine
            .state()
            .players
            .iter()
            .find(|player| player.id == PUNCHING_BAG_PLAYER_ID)
            .unwrap();
        assert!(bag.hand.is_empty());
        assert_eq!(bag.library.len(), 99);
        assert_eq!(bag.battlefield.len(), 2);
        assert!(bag.battlefield.iter().any(|card| {
            card.definition.rules.iter().any(|rule| {
                rule["modifiers"].as_array().is_some_and(|modifiers| {
                    modifiers
                        .iter()
                        .any(|modifier| modifier["kind"] == "skipDrawStep")
                })
            })
        }));
    }

    #[test]
    fn random_win_can_be_turned_into_a_bounded_tree() {
        let report = benchmark_punching_bag_tree(
            learner_deck(),
            PunchingBagBenchmarkConfig {
                seed: 91,
                search_root: PunchingBagSearchRoot::WinningTurnUntap,
                maximum_random_games: 1_000,
                maximum_turns: 8,
                maximum_unique_nodes: 100_000,
                maximum_depth: 64,
                maximum_choices_per_node: 4_096,
            },
        )
        .unwrap();
        assert!(report.discovery.games_attempted <= 1_000);
        assert!(report.tree.total_leaves > 0);
        assert!(report.tree.learner_win_leaves > 0);
        assert!(!report.discovery.known_winning_line.is_empty());
        assert_eq!(
            report.root_choices.len(),
            report.root_decision.options.len()
        );
        assert_eq!(
            report
                .root_choices
                .iter()
                .map(|choice| choice.total_leaves)
                .sum::<u64>(),
            report.tree.total_leaves
        );
    }

    #[test]
    fn progressive_search_starts_with_seven_cards_and_no_mulligan() {
        let report = progressively_find_punching_bag_win(
            learner_deck(),
            ProgressivePunchingBagConfig {
                seed: 91,
                maximum_segments: 4,
                maximum_turns: 8,
                maximum_unique_nodes: 100_000,
                maximum_depth: 64,
                maximum_choices_per_node: 4_096,
                position: None,
            },
        )
        .unwrap();

        let learner = report
            .root_state
            .players
            .iter()
            .find(|player| player.id == "learner")
            .unwrap();
        assert_eq!(report.opening_hand_size, 7);
        assert!(!report.mulligan_enabled);
        assert_eq!(report.mulligans_taken, 0);
        assert_eq!(learner.hand.len(), 7);
        assert_eq!(report.winning_segment_index, 0);
        assert!(report.tree.learner_win_leaves > 0);
        assert!(!report.known_winning_line.is_empty());
    }

    #[test]
    fn generator_keeps_only_complete_replayable_scenarios() {
        let dataset = generate_punching_bag_scenarios(
            learner_deck(),
            PunchingBagScenarioGenerationConfig {
                scenario_count: 1,
                maximum_generation_attempts: 2,
                benchmark: PunchingBagBenchmarkConfig {
                    seed: 117,
                    search_root: PunchingBagSearchRoot::WinningTurnUntap,
                    maximum_random_games: 1_000,
                    maximum_turns: 8,
                    maximum_unique_nodes: 100_000,
                    maximum_depth: 64,
                    maximum_choices_per_node: 4_096,
                },
            },
        )
        .unwrap();

        assert_eq!(dataset.schema_version, "mtg-punching-bag-scenarios/v1");
        assert_eq!(dataset.generated_scenario_count, 1);
        let scenario = &dataset.scenarios[0];
        assert!(scenario.tree.complete);
        assert!(scenario.tree.exact_for_fixed_random_tape);
        assert!(scenario.tree.learner_win_leaves > 0);
        assert_eq!(
            scenario.initial_session.decision.id,
            scenario.discovery.root_decision_id
        );
    }
}
