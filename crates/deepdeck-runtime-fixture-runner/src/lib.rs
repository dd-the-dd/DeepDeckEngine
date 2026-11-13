use deepdeck_engine_runtime::engine::{
    CanonicalActivatedAbilityActivation, CanonicalLandPlay, CanonicalManaAbilityActivation,
    CanonicalRuleExecution, CanonicalSpellCast, CanonicalTargetSelection, CardDefinition,
    DecisionChoice, DecisionProvider, DeterministicGamePosition, DeterministicPlayerPosition,
    EngineDecisionRequest, EngineError, GameEngine, GameSetup, GameState, GameStep, PlayerDeck,
    TargetRef, validate_decision_card_instance_ids,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

pub const FIXTURE_SCHEMA_VERSION: &str = "deepdeck-runtime-fixture/v1";
pub const RESULT_SCHEMA_VERSION: &str = "deepdeck-runtime-fixture-result/v1";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeFixture {
    pub schema_version: String,
    pub id: String,
    #[serde(default)]
    pub seed: u64,
    pub rule: Value,
    pub initial_state: FixtureState,
    #[serde(default)]
    pub execution: Option<FixtureExecution>,
    #[serde(default)]
    pub actions: Vec<FixtureAction>,
    #[serde(default)]
    pub decisions: Vec<ScriptedDecision>,
    pub assertions: Vec<FixtureAssertion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixtureState {
    pub active_player_id: String,
    #[serde(default = "default_turn_number")]
    pub turn_number: u32,
    #[serde(default = "default_step")]
    pub step: GameStep,
    pub players: Vec<FixturePlayer>,
}

fn default_turn_number() -> u32 {
    1
}

fn default_step() -> GameStep {
    GameStep::PrecombatMain
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixturePlayer {
    pub id: String,
    pub name: String,
    #[serde(default = "default_life")]
    pub life: i32,
    #[serde(default)]
    pub library: Vec<FixtureCard>,
    #[serde(default)]
    pub hand: Vec<FixtureCard>,
    #[serde(default)]
    pub battlefield: Vec<FixtureCard>,
    #[serde(default)]
    pub graveyard: Vec<FixtureCard>,
    #[serde(default)]
    pub command_zone: Vec<FixtureCard>,
    #[serde(default)]
    pub mana_pool: Vec<String>,
}

fn default_life() -> i32 {
    20
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixtureCard {
    pub id: String,
    pub name: String,
    pub type_line: String,
    #[serde(default)]
    pub controller_id: Option<String>,
    #[serde(default)]
    pub mana_cost: String,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub tapped: bool,
    #[serde(default)]
    pub summoning_sick: bool,
    #[serde(default)]
    pub is_commander: bool,
    #[serde(default)]
    pub is_token: bool,
    pub rules: Option<Vec<Value>>,
    #[serde(default)]
    pub counters: BTreeMap<String, i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixtureExecution {
    pub controller_id: String,
    pub source_definition_id: String,
    #[serde(default)]
    pub decisions: BTreeMap<String, Value>,
    #[serde(default)]
    pub targets: BTreeMap<String, TargetRef>,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FixtureAction {
    ActivateAbility {
        controller_id: String,
        source_definition_id: String,
        #[serde(default)]
        ability_rule_index: Option<usize>,
        #[serde(default)]
        declaration_decisions: BTreeMap<String, Value>,
        #[serde(default)]
        targets: BTreeMap<String, CanonicalTargetSelection>,
    },
    ActivateManaAbility {
        controller_id: String,
        source_definition_id: String,
        #[serde(default)]
        mana_symbols: Option<Vec<String>>,
        #[serde(default)]
        objects: BTreeMap<String, CanonicalTargetSelection>,
        #[serde(default)]
        declaration_decisions: BTreeMap<String, Value>,
    },
    CastSpell {
        controller_id: String,
        source_definition_id: String,
        #[serde(default)]
        use_alternative_cost: bool,
        #[serde(default)]
        alternative_exile_definition_id: Option<String>,
        #[serde(default)]
        declaration_decisions: BTreeMap<String, Value>,
    },
    PlayLand {
        player_id: String,
        source_definition_id: String,
    },
    RunMulligans {
        #[serde(default)]
        free_mulligans: usize,
        #[serde(default)]
        maximum_mulligans: Option<usize>,
    },
    RunPostcombatMain {
        maximum_turn_number: u32,
    },
    ResolveTopStack,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ScriptedDecision {
    ChooseOption { index: usize },
    ChooseNumber { value: i32 },
    ChooseCardName { value: String },
    SelectCards { count: usize, take: SelectionOrder },
    OrderCards { order: SelectionOrder },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionOrder {
    Forward,
    Reverse,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FixtureAssertion {
    ZoneCount {
        player_id: String,
        zone: FixtureZone,
        equals: usize,
    },
    ManaPoolEquals {
        player_id: String,
        symbols: Vec<String>,
    },
    LifeTotal {
        player_id: String,
        equals: i32,
    },
    ZoneContains {
        player_id: String,
        zone: FixtureZone,
        definition_id: String,
        equals: bool,
    },
    LibraryTopIs {
        player_id: String,
        definition_id: String,
    },
    LandPlayAvailable {
        player_id: String,
        source_definition_id: String,
        equals: bool,
    },
    DecisionCount {
        equals: usize,
    },
    EventCount {
        event_kind: String,
        #[serde(default)]
        player_id: Option<String>,
        #[serde(default)]
        step: Option<GameStep>,
        equals: usize,
    },
    PermanentTapped {
        player_id: String,
        definition_id: String,
        equals: bool,
    },
    PermanentController {
        player_id: String,
        definition_id: String,
        equals: String,
    },
    PermanentSummoningSick {
        player_id: String,
        definition_id: String,
        equals: bool,
    },
    PermanentTypeLineContains {
        player_id: String,
        definition_id: String,
        value: String,
        equals: bool,
    },
    PermanentHasKeyword {
        player_id: String,
        definition_id: String,
        keyword: String,
        equals: bool,
    },
    PermanentPowerToughness {
        player_id: String,
        definition_id: String,
        power: i32,
        toughness: i32,
    },
    PermanentColorSymbol {
        player_id: String,
        definition_id: String,
        symbol: String,
        equals: bool,
    },
    PermanentCounter {
        player_id: String,
        definition_id: String,
        counter: String,
        equals: i32,
    },
    PlayerCounter {
        player_id: String,
        counter: String,
        equals: i32,
    },
    ManaAbilityAvailable {
        controller_id: String,
        source_definition_id: String,
        #[serde(default)]
        mana_symbols: Option<Vec<String>>,
        #[serde(default)]
        objects: BTreeMap<String, CanonicalTargetSelection>,
        #[serde(default)]
        declaration_decisions: BTreeMap<String, Value>,
        equals: bool,
    },
    ActivatedAbilityAvailable {
        controller_id: String,
        source_definition_id: String,
        #[serde(default)]
        ability_rule_index: Option<usize>,
        #[serde(default)]
        declaration_decisions: BTreeMap<String, Value>,
        #[serde(default)]
        targets: BTreeMap<String, CanonicalTargetSelection>,
        equals: bool,
    },
    SpellCastAvailable {
        controller_id: String,
        source_definition_id: String,
        #[serde(default)]
        use_alternative_cost: bool,
        #[serde(default)]
        alternative_exile_definition_id: Option<String>,
        #[serde(default)]
        declaration_decisions: BTreeMap<String, Value>,
        equals: bool,
    },
    CanBlock {
        blocker_player_id: String,
        blocker_definition_id: String,
        attacker_player_id: String,
        attacker_definition_id: String,
        equals: bool,
    },
    StackDepth {
        equals: usize,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FixtureZone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Exile,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureResult {
    pub schema_version: &'static str,
    pub fixture_id: String,
    pub status: FixtureStatus,
    pub assertions: Vec<AssertionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FixtureStatus {
    Passed,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionResult {
    pub index: usize,
    pub passed: bool,
    pub description: String,
}

struct ScriptedProvider {
    decisions: VecDeque<ScriptedDecision>,
    calls: usize,
}

impl ScriptedProvider {
    fn next(&mut self, expected: &str) -> Result<ScriptedDecision, EngineError> {
        self.calls += 1;
        self.decisions.pop_front().ok_or_else(|| {
            EngineError::new(format!(
                "script exhausted at decision {} ({expected})",
                self.calls
            ))
        })
    }

    fn select(candidates: &[String], count: usize, order: SelectionOrder) -> Vec<String> {
        match order {
            SelectionOrder::Forward => candidates.iter().take(count).cloned().collect(),
            SelectionOrder::Reverse => candidates.iter().rev().take(count).cloned().collect(),
        }
    }
}

impl DecisionProvider for ScriptedProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let ScriptedDecision::ChooseOption { index } = self.next("chooseOption")? else {
            return Err(EngineError::new(format!(
                "decision {} expected chooseOption",
                request.id
            )));
        };
        if index >= request.options.len() {
            return Err(EngineError::new(format!(
                "decision {} selected option {index}, but only {} options exist",
                request.id,
                request.options.len()
            )));
        }
        Ok(index)
    }

    fn choose_number(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let ScriptedDecision::ChooseNumber { value } = self.next("chooseNumber")? else {
            return Err(EngineError::new(format!(
                "decision {} expected chooseNumber",
                request.id
            )));
        };
        Ok(value)
    }

    fn choose_card_name(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<String, EngineError> {
        let ScriptedDecision::ChooseCardName { value } = self.next("chooseCardName")? else {
            return Err(EngineError::new(format!(
                "decision {} expected chooseCardName",
                request.id
            )));
        };
        Ok(value)
    }

    fn choose_card_instance_ids(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        let decision = self.next("card selection or ordering")?;
        let selected = match (&request.choice, decision) {
            (
                Some(DecisionChoice::CardSelection {
                    candidate_card_instance_ids,
                    ..
                }),
                ScriptedDecision::SelectCards { count, take },
            ) => Self::select(candidate_card_instance_ids, count, take),
            (
                Some(DecisionChoice::CardOrder {
                    card_instance_ids, ..
                }),
                ScriptedDecision::OrderCards { order },
            ) => Self::select(card_instance_ids, card_instance_ids.len(), order),
            _ => {
                return Err(EngineError::new(format!(
                    "decision {} does not match the scripted card decision",
                    request.id
                )));
            }
        };
        validate_decision_card_instance_ids(request, &selected)?;
        Ok(selected)
    }
}

pub fn run_fixture_path(path: &Path) -> FixtureResult {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            return failed_result(path.display().to_string(), format!("read failed: {error}"));
        }
    };
    let fixture: RuntimeFixture = match serde_json::from_str(&raw) {
        Ok(fixture) => fixture,
        Err(error) => {
            return failed_result(
                path.display().to_string(),
                format!("strict fixture decoding failed: {error}"),
            );
        }
    };
    run_fixture(fixture)
}

pub fn run_fixture(fixture: RuntimeFixture) -> FixtureResult {
    let fixture_id = fixture.id.clone();
    if let Err(error) = validate_fixture(&fixture) {
        return failed_result(fixture_id, error);
    }
    let mut provider = ScriptedProvider {
        decisions: fixture.decisions.into(),
        calls: 0,
    };
    let mut rule_sources = fixture
        .actions
        .iter()
        .filter_map(|action| match action {
            FixtureAction::ActivateAbility {
                controller_id,
                source_definition_id,
                ..
            }
            | FixtureAction::ActivateManaAbility {
                controller_id,
                source_definition_id,
                ..
            } => Some((controller_id.clone(), source_definition_id.clone())),
            FixtureAction::CastSpell {
                controller_id,
                source_definition_id,
                ..
            } => Some((controller_id.clone(), source_definition_id.clone())),
            FixtureAction::PlayLand { .. } => None,
            FixtureAction::RunMulligans { .. } => None,
            FixtureAction::RunPostcombatMain { .. } => None,
            FixtureAction::ResolveTopStack => None,
        })
        .collect::<BTreeSet<_>>();
    rule_sources.extend(
        fixture
            .assertions
            .iter()
            .filter_map(|assertion| match assertion {
                FixtureAssertion::ManaAbilityAvailable {
                    controller_id,
                    source_definition_id,
                    ..
                }
                | FixtureAssertion::ActivatedAbilityAvailable {
                    controller_id,
                    source_definition_id,
                    ..
                }
                | FixtureAssertion::SpellCastAvailable {
                    controller_id,
                    source_definition_id,
                    ..
                } => Some((controller_id.clone(), source_definition_id.clone())),
                _ => None,
            }),
    );
    let mut engine = match build_engine(
        &fixture.initial_state,
        fixture.seed,
        &fixture.rule,
        &rule_sources,
        &mut provider,
    ) {
        Ok(engine) => engine,
        Err(error) => return failed_result(fixture_id, error.to_string()),
    };
    if let Some(execution) = fixture.execution {
        let execution = CanonicalRuleExecution {
            controller_id: execution.controller_id,
            source_definition_id: execution.source_definition_id,
            rule: fixture.rule.clone(),
            decisions: execution.decisions,
            targets: execution.targets,
        };
        if let Err(error) = engine.execute_canonical_rule(execution, &mut provider) {
            return failed_result(fixture_id, error.to_string());
        }
    }
    for action in &fixture.actions {
        let result = match action {
            FixtureAction::ActivateAbility {
                controller_id,
                source_definition_id,
                ability_rule_index,
                declaration_decisions,
                targets,
            } => engine.activate_canonical_ability_with_decisions(
                &CanonicalActivatedAbilityActivation {
                    controller_id: controller_id.clone(),
                    source_definition_id: source_definition_id.clone(),
                    ability_rule_index: *ability_rule_index,
                    targets: targets.clone(),
                },
                declaration_decisions,
                &mut provider,
            ),
            FixtureAction::ActivateManaAbility {
                controller_id,
                source_definition_id,
                mana_symbols,
                objects,
                declaration_decisions,
            } => engine.activate_canonical_mana_ability_with_decisions(
                &CanonicalManaAbilityActivation {
                    controller_id: controller_id.clone(),
                    source_definition_id: source_definition_id.clone(),
                    mana_symbols: mana_symbols.clone(),
                },
                objects,
                declaration_decisions,
                &mut provider,
            ),
            FixtureAction::CastSpell {
                controller_id,
                source_definition_id,
                use_alternative_cost,
                alternative_exile_definition_id,
                declaration_decisions,
            } => engine.cast_canonical_spell(
                &CanonicalSpellCast {
                    controller_id: controller_id.clone(),
                    source_definition_id: source_definition_id.clone(),
                    use_alternative_cost: *use_alternative_cost,
                    alternative_exile_definition_id: alternative_exile_definition_id.clone(),
                    declaration_decisions: declaration_decisions.clone(),
                },
                &mut provider,
            ),
            FixtureAction::PlayLand {
                player_id,
                source_definition_id,
            } => engine.play_canonical_land(
                &CanonicalLandPlay {
                    player_id: player_id.clone(),
                    source_definition_id: source_definition_id.clone(),
                },
                &mut provider,
            ),
            FixtureAction::RunMulligans {
                free_mulligans,
                maximum_mulligans,
            } => engine.run_mulligans(&mut provider, *free_mulligans, *maximum_mulligans),
            FixtureAction::RunPostcombatMain {
                maximum_turn_number,
            } => engine
                .run_from_postcombat_main(&mut provider, *maximum_turn_number)
                .map(|_| ()),
            FixtureAction::ResolveTopStack => engine.resolve_canonical_top_stack(&mut provider),
        };
        if let Err(error) = result {
            return failed_result(fixture_id, error.to_string());
        }
    }
    if !provider.decisions.is_empty() {
        return failed_result(
            fixture_id,
            format!(
                "{} scripted decision(s) were not consumed",
                provider.decisions.len()
            ),
        );
    }

    let assertions = fixture
        .assertions
        .iter()
        .enumerate()
        .map(|(index, assertion)| evaluate_assertion(index, assertion, &engine, provider.calls))
        .collect::<Vec<_>>();
    let passed = assertions.iter().all(|assertion| assertion.passed);
    FixtureResult {
        schema_version: RESULT_SCHEMA_VERSION,
        fixture_id,
        status: if passed {
            FixtureStatus::Passed
        } else {
            FixtureStatus::Failed
        },
        assertions,
        error: None,
    }
}

fn validate_fixture(fixture: &RuntimeFixture) -> Result<(), String> {
    if fixture.schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported schemaVersion {:?}; expected {FIXTURE_SCHEMA_VERSION}",
            fixture.schema_version
        ));
    }
    if fixture.id.trim().is_empty() {
        return Err("fixture id must not be empty".to_string());
    }
    if !(2..=4).contains(&fixture.initial_state.players.len()) {
        return Err("initialState must contain two to four players".to_string());
    }
    if fixture.assertions.is_empty() {
        return Err("fixture must contain at least one assertion".to_string());
    }
    if !fixture.rule.is_object() || !fixture.rule.get("kind").is_some_and(Value::is_string) {
        return Err("rule must be a canonical object with a kind".to_string());
    }
    if !deepdeck_engine_runtime::engine::rule_is_executable(&fixture.rule) {
        return Err("rule is not executable by the runtime".to_string());
    }
    let has_availability_assertion = fixture.assertions.iter().any(|assertion| {
        matches!(
            assertion,
            FixtureAssertion::ManaAbilityAvailable { .. }
                | FixtureAssertion::ActivatedAbilityAvailable { .. }
                | FixtureAssertion::SpellCastAvailable { .. }
                | FixtureAssertion::LandPlayAvailable { .. }
                | FixtureAssertion::CanBlock { .. }
        )
    });
    if fixture.execution.is_none() && fixture.actions.is_empty() && !has_availability_assertion {
        return Err(
            "fixture must contain execution, an action, or an availability assertion".to_string(),
        );
    }

    let mut player_ids = BTreeSet::new();
    for player in &fixture.initial_state.players {
        if player.id.trim().is_empty() || !player_ids.insert(player.id.as_str()) {
            return Err("player ids must be non-empty and unique".to_string());
        }
    }
    for player in &fixture.initial_state.players {
        let mut definition_ids = BTreeSet::new();
        for card in player
            .library
            .iter()
            .chain(player.hand.iter())
            .chain(player.battlefield.iter())
            .chain(player.graveyard.iter())
            .chain(player.command_zone.iter())
        {
            if card.id.trim().is_empty() || !definition_ids.insert(card.id.as_str()) {
                return Err(format!(
                    "player {} card ids must be non-empty and unique",
                    player.id
                ));
            }
            if let Some(controller_id) = card.controller_id.as_deref()
                && !player_ids.contains(controller_id)
            {
                return Err(format!(
                    "player {} card {} controllerId {controller_id} does not identify a fixture player",
                    player.id, card.id
                ));
            }
            if card
                .rules
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|rule| !deepdeck_engine_runtime::engine::rule_is_executable(rule))
            {
                return Err(format!(
                    "player {} card {} contains an unexecutable supporting rule",
                    player.id, card.id
                ));
            }
        }
        for card in player
            .library
            .iter()
            .chain(player.hand.iter())
            .chain(player.graveyard.iter())
            .chain(player.command_zone.iter())
        {
            if card.is_token {
                return Err(format!(
                    "player {} token {} may be configured only on the battlefield",
                    player.id, card.id
                ));
            }
            if card.controller_id.is_some() {
                return Err(format!(
                    "player {} card {} may specify controllerId only on the battlefield",
                    player.id, card.id
                ));
            }
        }
    }
    if !player_ids.contains(fixture.initial_state.active_player_id.as_str()) {
        return Err("activePlayerId does not identify a fixture player".to_string());
    }
    if let Some(execution) = &fixture.execution {
        if !player_ids.contains(execution.controller_id.as_str()) {
            return Err("execution.controllerId does not identify a fixture player".to_string());
        }
        let source_count = fixture_source_count(
            &fixture.initial_state,
            &execution.controller_id,
            &execution.source_definition_id,
            false,
        );
        if source_count != 1 {
            return Err(format!(
                "execution source must occur exactly once in its controller zones; found {source_count}"
            ));
        }
    }
    for action in &fixture.actions {
        if let FixtureAction::ActivateAbility {
            controller_id,
            source_definition_id,
            ..
        } = action
        {
            if !player_ids.contains(controller_id.as_str()) {
                return Err(format!(
                    "action controllerId {controller_id} does not identify a fixture player"
                ));
            }
            // Preflight checks fixture identity, not the source's future
            // zone/controller. Earlier actions can move or cast this source;
            // the engine validates its current activation zone at execution.
            let source_count =
                fixture_initial_definition_count(&fixture.initial_state, source_definition_id);
            if source_count != 1 {
                return Err(format!(
                    "action source {source_definition_id} must occur exactly once across initialState card zones; found {source_count}"
                ));
            }
            continue;
        }
        let (controller_id, source_definition_id, battlefield_only) = match action {
            FixtureAction::ActivateAbility { .. } => continue,
            FixtureAction::ActivateManaAbility {
                controller_id,
                source_definition_id,
                ..
            } => (controller_id, source_definition_id, true),
            FixtureAction::CastSpell {
                controller_id,
                source_definition_id,
                ..
            } => (controller_id, source_definition_id, false),
            FixtureAction::PlayLand {
                player_id,
                source_definition_id,
            } => (player_id, source_definition_id, false),
            FixtureAction::RunMulligans { .. } => continue,
            FixtureAction::RunPostcombatMain {
                maximum_turn_number,
            } => {
                if fixture.initial_state.step != GameStep::PostcombatMain {
                    return Err(
                        "runPostcombatMain requires initialState.step postcombatMain".to_string(),
                    );
                }
                if *maximum_turn_number < fixture.initial_state.turn_number {
                    return Err(
                        "runPostcombatMain maximumTurnNumber precedes initialState.turnNumber"
                            .to_string(),
                    );
                }
                continue;
            }
            FixtureAction::ResolveTopStack => continue,
        };
        if !player_ids.contains(controller_id.as_str()) {
            return Err(format!(
                "action controllerId {controller_id} does not identify a fixture player"
            ));
        }
        let source_count = fixture_source_count(
            &fixture.initial_state,
            controller_id,
            source_definition_id,
            battlefield_only,
        );
        if source_count != 1 {
            return Err(format!(
                "action source {source_definition_id} must occur exactly once in its required zone for {controller_id}; found {source_count}"
            ));
        }
    }
    Ok(())
}

fn fixture_initial_definition_count(state: &FixtureState, source_definition_id: &str) -> usize {
    state
        .players
        .iter()
        .flat_map(|player| {
            player
                .library
                .iter()
                .chain(player.hand.iter())
                .chain(player.battlefield.iter())
                .chain(player.graveyard.iter())
                .chain(player.command_zone.iter())
        })
        .filter(|card| card.id == source_definition_id)
        .count()
}

fn fixture_source_count(
    state: &FixtureState,
    controller_id: &str,
    source_definition_id: &str,
    battlefield_only: bool,
) -> usize {
    let battlefield_count = state
        .players
        .iter()
        .flat_map(|owner| {
            owner
                .battlefield
                .iter()
                .map(move |card| (owner.id.as_str(), card))
        })
        .filter(|(owner_id, card)| {
            card.controller_id.as_deref().unwrap_or(owner_id) == controller_id
                && card.id == source_definition_id
        })
        .count();
    let other_zone_count = (!battlefield_only)
        .then(|| {
            state
                .players
                .iter()
                .filter(|player| player.id == controller_id)
                .flat_map(|player| {
                    player
                        .hand
                        .iter()
                        .chain(player.library.iter())
                        .chain(player.graveyard.iter())
                })
                .filter(|card| card.id == source_definition_id)
                .count()
        })
        .unwrap_or(0);
    battlefield_count + other_zone_count
}

fn build_engine(
    state: &FixtureState,
    seed: u64,
    rule: &Value,
    rule_sources: &BTreeSet<(String, String)>,
    provider: &mut ScriptedProvider,
) -> Result<GameEngine, EngineError> {
    let setup = GameSetup {
        players: state
            .players
            .iter()
            .map(|player| PlayerDeck {
                id: player.id.clone(),
                name: player.name.clone(),
                starting_life: player.life,
                cards: player
                    .library
                    .iter()
                    .chain(player.hand.iter())
                    .chain(player.battlefield.iter())
                    .chain(player.graveyard.iter())
                    .chain(player.command_zone.iter())
                    .filter(|card| !card.is_token)
                    .map(|card| {
                        let controller_id = card.controller_id.as_deref().unwrap_or(&player.id);
                        card_definition(
                            card,
                            rule_sources
                                .contains(&(controller_id.to_string(), card.id.clone()))
                                .then_some(rule),
                        )
                    })
                    .collect(),
            })
            .collect(),
        opening_hand_size: 0,
        starting_player: state
            .players
            .iter()
            .position(|player| player.id == state.active_player_id)
            .unwrap_or(0),
    };
    let mut engine = GameEngine::new(setup, seed)?;
    engine.configure_deterministic_position(&DeterministicGamePosition {
        turn_number: state.turn_number,
        active_player_id: state.active_player_id.clone(),
        step: state.step.clone(),
        players: state
            .players
            .iter()
            .map(|player| DeterministicPlayerPosition {
                player_id: player.id.clone(),
                life: player.life,
                battlefield_definition_ids: player
                    .battlefield
                    .iter()
                    .filter(|card| !card.is_token)
                    .map(|card| card.id.clone())
                    .collect(),
                hand_definition_ids: player.hand.iter().map(|card| card.id.clone()).collect(),
                library_size: player.library.len()
                    + player.graveyard.len()
                    + player.command_zone.len(),
                mana_pool: player.mana_pool.clone(),
            })
            .collect(),
    })?;
    for player in &state.players {
        engine.configure_deterministic_graveyard(
            &player.id,
            &player
                .graveyard
                .iter()
                .map(|card| card.id.clone())
                .collect::<Vec<_>>(),
        )?;
        engine.configure_deterministic_command_zone(
            &player.id,
            &player
                .command_zone
                .iter()
                .map(|card| card.id.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    for player in &state.players {
        for (battlefield_index, card) in player.battlefield.iter().enumerate() {
            if !card.is_token {
                continue;
            }
            let controller_id = card.controller_id.as_deref().unwrap_or(&player.id);
            engine.configure_deterministic_battlefield_token(
                &player.id,
                battlefield_index,
                card_definition(
                    card,
                    rule_sources
                        .contains(&(controller_id.to_string(), card.id.clone()))
                        .then_some(rule),
                ),
            )?;
        }
    }
    apply_initial_controller_overrides(&mut engine, state, provider)?;
    for player in &state.players {
        for card in &player.battlefield {
            let controller_id = card.controller_id.as_deref().unwrap_or(&player.id);
            engine.configure_deterministic_permanent_state(
                controller_id,
                &card.id,
                card.tapped,
                card.summoning_sick,
            )?;
            if card.counters.values().any(|count| *count < 0) {
                return Err(EngineError::new(format!(
                    "initial counters on {} cannot be negative",
                    card.id
                )));
            }
            if !card.counters.is_empty() {
                engine.configure_deterministic_permanent_counters(
                    controller_id,
                    &card.id,
                    &card.counters,
                )?;
            }
        }
    }
    Ok(engine)
}

fn apply_initial_controller_overrides(
    engine: &mut GameEngine,
    state: &FixtureState,
    provider: &mut ScriptedProvider,
) -> Result<(), EngineError> {
    const PERMANENT_TARGET_ID: &str = "fixtureInitialPermanent";
    const CONTROLLER_TARGET_ID: &str = "fixtureInitialController";

    for owner in &state.players {
        for card in &owner.battlefield {
            let Some(controller_id) = card.controller_id.as_deref() else {
                continue;
            };
            if controller_id == owner.id {
                continue;
            }
            let instance_id = engine
                .state()
                .players
                .iter()
                .flat_map(|player| player.battlefield.iter())
                .find(|instance| {
                    instance.owner == owner.id && instance.definition.id == card.id
                })
                .map(|instance| instance.instance_id.clone())
                .ok_or_else(|| {
                    EngineError::new(format!(
                        "initial controller override could not find owner {} card {} on the battlefield",
                        owner.id, card.id
                    ))
                })?;
            engine.execute_canonical_rule(
                CanonicalRuleExecution {
                    controller_id: owner.id.clone(),
                    source_definition_id: card.id.clone(),
                    rule: serde_json::json!({
                        "kind": "spellAbility",
                        "source": { "kind": "self" },
                        "effects": [{
                            "kind": "gainControlPermanent",
                            "permanent": {
                                "kind": "chosenTarget",
                                "id": PERMANENT_TARGET_ID
                            },
                            "controller": {
                                "kind": "chosenTarget",
                                "id": CONTROLLER_TARGET_ID
                            }
                        }]
                    }),
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::from([
                        (
                            PERMANENT_TARGET_ID.to_string(),
                            TargetRef::Permanent { instance_id },
                        ),
                        (
                            CONTROLLER_TARGET_ID.to_string(),
                            TargetRef::Player {
                                player_id: controller_id.to_string(),
                            },
                        ),
                    ]),
                },
                provider,
            )?;
        }
    }
    Ok(())
}

fn card_definition(card: &FixtureCard, rule: Option<&Value>) -> CardDefinition {
    let rules = match &card.rules {
        None => rule.cloned().into_iter().collect(),
        Some(rules) => rules.clone(),
    };
    CardDefinition {
        id: card.id.clone(),
        name: card.name.clone(),
        type_line: card.type_line.clone(),
        is_commander: card.is_commander,
        is_token: card.is_token,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: card.mana_cost.clone(),
        power: card.power.clone(),
        toughness: card.toughness.clone(),
        rules,
    }
}

fn evaluate_assertion(
    index: usize,
    assertion: &FixtureAssertion,
    engine: &GameEngine,
    decision_count: usize,
) -> AssertionResult {
    let state = engine.state();
    let (passed, description) = match assertion {
        FixtureAssertion::ZoneCount {
            player_id,
            zone,
            equals,
        } => {
            let actual = state
                .players
                .iter()
                .find(|player| player.id == *player_id)
                .map(|player| match zone {
                    FixtureZone::Library => player.library.len(),
                    FixtureZone::Hand => player.hand.len(),
                    FixtureZone::Battlefield => player.battlefield.len(),
                    FixtureZone::Graveyard => player.graveyard.len(),
                    FixtureZone::Exile => player.exile.len(),
                });
            (
                actual == Some(*equals),
                format!("{player_id} {zone:?} count expected {equals}, actual {actual:?}"),
            )
        }
        FixtureAssertion::ManaPoolEquals { player_id, symbols } => {
            let actual = state
                .players
                .iter()
                .find(|player| player.id == *player_id)
                .map(|player| {
                    player
                        .mana_pool
                        .iter()
                        .map(|mana| mana.symbol.clone())
                        .collect::<Vec<_>>()
                });
            (
                actual.as_ref() == Some(symbols),
                format!("{player_id} mana expected {symbols:?}, actual {actual:?}"),
            )
        }
        FixtureAssertion::LifeTotal { player_id, equals } => {
            let actual = state
                .players
                .iter()
                .find(|player| player.id == *player_id)
                .map(|player| player.life);
            (
                actual == Some(*equals),
                format!("{player_id} life expected {equals}, actual {actual:?}"),
            )
        }
        FixtureAssertion::ZoneContains {
            player_id,
            zone,
            definition_id,
            equals,
        } => {
            let actual = state
                .players
                .iter()
                .find(|player| player.id == *player_id)
                .map(|player| match zone {
                    FixtureZone::Library => &player.library,
                    FixtureZone::Hand => &player.hand,
                    FixtureZone::Battlefield => &player.battlefield,
                    FixtureZone::Graveyard => &player.graveyard,
                    FixtureZone::Exile => &player.exile,
                })
                .map(|cards| {
                    cards
                        .iter()
                        .any(|card| card.definition.id == *definition_id)
                });
            (
                actual == Some(*equals),
                format!(
                    "{player_id} {zone:?} contains {definition_id} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::LibraryTopIs {
            player_id,
            definition_id,
        } => {
            let actual = state
                .players
                .iter()
                .find(|player| player.id == *player_id)
                .and_then(|player| player.library.last())
                .map(|card| card.definition.id.as_str());
            (
                actual == Some(definition_id.as_str()),
                format!("{player_id} library top expected {definition_id}, actual {actual:?}"),
            )
        }
        FixtureAssertion::LandPlayAvailable {
            player_id,
            source_definition_id,
            equals,
        } => {
            let actual = engine
                .canonical_land_play_available(&CanonicalLandPlay {
                    player_id: player_id.clone(),
                    source_definition_id: source_definition_id.clone(),
                })
                .ok();
            (
                actual == Some(*equals),
                format!(
                    "{player_id} land play {source_definition_id} availability expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::DecisionCount { equals } => (
            decision_count == *equals,
            format!("decision count expected {equals}, actual {decision_count}"),
        ),
        FixtureAssertion::EventCount {
            event_kind,
            player_id,
            step,
            equals,
        } => {
            let actual = state
                .events
                .iter()
                .filter(|event| {
                    event.kind == *event_kind
                        && player_id
                            .as_deref()
                            .is_none_or(|expected| event.player_id.as_deref() == Some(expected))
                        && step.as_ref().is_none_or(|expected| event.step == *expected)
                })
                .count();
            (
                actual == *equals,
                format!(
                    "event {event_kind} player {player_id:?} step {step:?} count expected {equals}, actual {actual}"
                ),
            )
        }
        FixtureAssertion::PermanentTapped {
            player_id,
            definition_id,
            equals,
        } => {
            let actual = permanent_by_definition(state, player_id, definition_id)
                .map(|permanent| permanent.tapped);
            (
                actual == Some(*equals),
                format!(
                    "{player_id} permanent {definition_id} tapped expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentController {
            player_id,
            definition_id,
            equals,
        } => {
            let actual = permanent_by_definition(state, player_id, definition_id)
                .map(|permanent| permanent.controller.as_str());
            (
                actual == Some(equals.as_str()),
                format!(
                    "{player_id} permanent {definition_id} controller expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentSummoningSick {
            player_id,
            definition_id,
            equals,
        } => {
            let actual = permanent_by_definition(state, player_id, definition_id)
                .map(|permanent| permanent.summoning_sick);
            (
                actual == Some(*equals),
                format!(
                    "{player_id} permanent {definition_id} summoningSick expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentTypeLineContains {
            player_id,
            definition_id,
            value,
            equals,
        } => {
            let actual =
                permanent_by_definition(state, player_id, definition_id).map(|permanent| {
                    permanent
                        .definition
                        .type_line
                        .split(|character: char| !character.is_alphanumeric())
                        .any(|part| part.eq_ignore_ascii_case(value))
                });
            (
                actual == Some(*equals),
                format!(
                    "{player_id} permanent {definition_id} typeLine contains {value} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentHasKeyword {
            player_id,
            definition_id,
            keyword,
            equals,
        } => {
            let actual = permanent_by_definition(state, player_id, definition_id)
                .map(|permanent| engine.permanent_has_effective_keyword(permanent, keyword));
            (
                actual == Some(*equals),
                format!(
                    "{player_id} permanent {definition_id} has keyword {keyword} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentPowerToughness {
            player_id,
            definition_id,
            power,
            toughness,
        } => {
            let actual =
                permanent_by_definition(state, player_id, definition_id).and_then(|permanent| {
                    GameEngine::effective_power_toughness(state)
                        .get(&permanent.instance_id)
                        .map(|stats| (stats.current_power, stats.current_toughness))
                });
            (
                actual == Some((*power, *toughness)),
                format!(
                    "{player_id} permanent {definition_id} effective power/toughness expected {power}/{toughness}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentColorSymbol {
            player_id,
            definition_id,
            symbol,
            equals,
        } => {
            let actual =
                permanent_by_definition(state, player_id, definition_id).map(|permanent| {
                    permanent
                        .definition
                        .rules
                        .iter()
                        .flat_map(|rule| rule["colorIndicator"].as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .any(|candidate| candidate.eq_ignore_ascii_case(symbol))
                });
            (
                actual == Some(*equals),
                format!(
                    "{player_id} permanent {definition_id} color indicator contains {symbol} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PermanentCounter {
            player_id,
            definition_id,
            counter,
            equals,
        } => {
            let actual = permanent_by_definition(state, player_id, definition_id)
                .map(|permanent| permanent.counters.get(counter).copied().unwrap_or(0));
            (
                actual == Some(*equals),
                format!(
                    "{player_id} permanent {definition_id} counter {counter} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::PlayerCounter {
            player_id,
            counter,
            equals,
        } => {
            let actual = state
                .players
                .iter()
                .find(|player| player.id == *player_id)
                .map(|player| player.counters.get(counter).copied().unwrap_or(0));
            (
                actual == Some(*equals),
                format!(
                    "{player_id} player counter {counter} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::ManaAbilityAvailable {
            controller_id,
            source_definition_id,
            mana_symbols,
            objects,
            declaration_decisions,
            equals,
        } => {
            let actual = engine
                .canonical_mana_ability_available_with_decisions(
                    &CanonicalManaAbilityActivation {
                        controller_id: controller_id.clone(),
                        source_definition_id: source_definition_id.clone(),
                        mana_symbols: mana_symbols.clone(),
                    },
                    objects,
                    declaration_decisions,
                )
                .ok();
            (
                actual == Some(*equals),
                format!(
                    "{controller_id} mana ability {source_definition_id} availability expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::ActivatedAbilityAvailable {
            controller_id,
            source_definition_id,
            ability_rule_index,
            declaration_decisions,
            targets,
            equals,
        } => {
            let actual = engine
                .canonical_activated_ability_available_with_decisions(
                    &CanonicalActivatedAbilityActivation {
                        controller_id: controller_id.clone(),
                        source_definition_id: source_definition_id.clone(),
                        ability_rule_index: *ability_rule_index,
                        targets: targets.clone(),
                    },
                    declaration_decisions,
                )
                .ok();
            (
                actual == Some(*equals),
                format!(
                    "{controller_id} activated ability {source_definition_id} rule={ability_rule_index:?} availability expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::SpellCastAvailable {
            controller_id,
            source_definition_id,
            use_alternative_cost,
            alternative_exile_definition_id,
            declaration_decisions,
            equals,
        } => {
            let actual = engine
                .canonical_spell_cast_available(&CanonicalSpellCast {
                    controller_id: controller_id.clone(),
                    source_definition_id: source_definition_id.clone(),
                    use_alternative_cost: *use_alternative_cost,
                    alternative_exile_definition_id: alternative_exile_definition_id.clone(),
                    declaration_decisions: declaration_decisions.clone(),
                })
                .ok();
            (
                actual == Some(*equals),
                format!(
                    "{controller_id} spell {source_definition_id} alternative={use_alternative_cost} exile={alternative_exile_definition_id:?} availability expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::CanBlock {
            blocker_player_id,
            blocker_definition_id,
            attacker_player_id,
            attacker_definition_id,
            equals,
        } => {
            let actual = permanent_by_definition(state, blocker_player_id, blocker_definition_id)
                .zip(permanent_by_definition(
                    state,
                    attacker_player_id,
                    attacker_definition_id,
                ))
                .map(|(blocker, attacker)| engine.can_block(blocker, attacker));
            (
                actual == Some(*equals),
                format!(
                    "{blocker_player_id} {blocker_definition_id} can block {attacker_player_id} {attacker_definition_id} expected {equals}, actual {actual:?}"
                ),
            )
        }
        FixtureAssertion::StackDepth { equals } => {
            let actual = state.stack.len();
            (
                actual == *equals,
                format!("stack depth expected {equals}, actual {actual}"),
            )
        }
    };
    AssertionResult {
        index,
        passed,
        description,
    }
}

fn permanent_by_definition<'a>(
    state: &'a GameState,
    player_id: &str,
    definition_id: &str,
) -> Option<&'a deepdeck_engine_runtime::engine::CardInstance> {
    let mut matches = state
        .players
        .iter()
        .find(|player| player.id == player_id)?
        .battlefield
        .iter()
        .filter(|card| card.definition.id == definition_id);
    let permanent = matches.next()?;
    matches.next().is_none().then_some(permanent)
}

fn failed_result(fixture_id: String, error: String) -> FixtureResult {
    FixtureResult {
        schema_version: RESULT_SCHEMA_VERSION,
        fixture_id,
        status: FixtureStatus::Failed,
        assertions: Vec::new(),
        error: Some(error),
    }
}
