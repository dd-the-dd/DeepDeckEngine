use mtg_engine::engine::{
    ActionKind, CardDefinition, DecisionChoice, DecisionProvider, DeterministicGamePosition,
    DeterministicPlayerPosition, EngineDecisionRequest, EngineError, GameEngine, GameMode,
    GameSetup, GameState, GameStep, LegalAction, PlayerDeck, TargetRef, rule_is_executable,
};
use mtg_engine::oracle::{OracleCardParseRequest, parse_oracle_card};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const OMNATH_ID: &str = "omnath";
const OPPONENT_IDS: [&str; 3] = ["opponent-1", "opponent-2", "opponent-3"];
const LAND_COUNT: usize = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpellTiming {
    Instant,
    Sorcery,
}

#[derive(Clone)]
struct ScenarioSpec {
    id: &'static str,
    method: &'static str,
    target_scope: &'static str,
    spell: CardDefinition,
    timing: SpellTiming,
    land_types: Vec<&'static str>,
    required_cast_targets: BTreeMap<&'static str, &'static str>,
    resolution_mode: Option<&'static str>,
    resolution_targets: Vec<&'static str>,
    expected_eliminated: Vec<&'static str>,
    opponent_life: i32,
    opponent_library_sizes: [usize; 3],
    awakening: bool,
    maximum_turn_number: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionTrace {
    decision_id: String,
    action_id: String,
    label: String,
    turn_number: u32,
    step: GameStep,
    active_player_id: String,
    mana_before: usize,
}

struct OmnathProvider {
    spell_name: String,
    timing: SpellTiming,
    required_cast_targets: BTreeMap<String, String>,
    resolution_mode: Option<String>,
    resolution_targets: VecDeque<String>,
    awakening: bool,
    skip_upkeep_player: Option<String>,
    cube_used: bool,
    spell_cast: bool,
    action_trace: Vec<ActionTrace>,
    upkeep_land_taps: BTreeMap<String, usize>,
    first_decision: Option<EngineDecisionRequest>,
    first_action_index: Option<usize>,
}

impl OmnathProvider {
    fn new(spec: &ScenarioSpec, skip_upkeep_player: Option<&str>) -> Self {
        Self {
            spell_name: spec.spell.name.clone(),
            timing: spec.timing,
            required_cast_targets: spec
                .required_cast_targets
                .iter()
                .map(|(key, player)| ((*key).to_string(), (*player).to_string()))
                .collect(),
            resolution_mode: spec.resolution_mode.map(ToOwned::to_owned),
            resolution_targets: spec
                .resolution_targets
                .iter()
                .map(|player| (*player).to_string())
                .collect(),
            awakening: spec.awakening,
            skip_upkeep_player: skip_upkeep_player.map(ToOwned::to_owned),
            cube_used: false,
            spell_cast: false,
            action_trace: Vec::new(),
            upkeep_land_taps: BTreeMap::new(),
            first_decision: None,
            first_action_index: None,
        }
    }

    fn player<'a>(
        state: &'a GameState,
        player_id: &str,
    ) -> Option<&'a mtg_engine::engine::EnginePlayer> {
        state.players.iter().find(|player| player.id == player_id)
    }

    fn card_is_land(state: &GameState, instance_id: &str) -> bool {
        state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .find(|card| card.instance_id == instance_id)
            .is_some_and(|card| {
                card.definition
                    .type_line
                    .split(|character: char| !character.is_alphabetic())
                    .any(|word| word.eq_ignore_ascii_case("Land"))
            })
    }

    fn action_contains_string(action: &LegalAction, expected: &str) -> bool {
        fn contains(value: &Value, expected: &str) -> bool {
            match value {
                Value::String(value) => value == expected,
                Value::Array(values) => values.iter().any(|value| contains(value, expected)),
                Value::Object(values) => values.values().any(|value| contains(value, expected)),
                _ => false,
            }
        }
        contains(&json!(action), expected)
    }

    fn cast_targets_match(&self, action: &LegalAction) -> bool {
        self.required_cast_targets
            .iter()
            .all(|(decision_id, player_id)| {
                action.targets.get(decision_id)
                    == Some(&TargetRef::Player {
                        player_id: player_id.clone(),
                    })
            })
    }

    fn should_tap_lands(&self, state: &GameState) -> bool {
        if self.spell_cast {
            return false;
        }
        let active_player_id = &state.players[state.active_player].id;
        match state.step {
            GameStep::PostcombatMain => !self.cube_used,
            GameStep::Upkeep if self.awakening => {
                if self.skip_upkeep_player.as_deref() == Some(active_player_id.as_str()) {
                    return false;
                }
                if active_player_id == OMNATH_ID {
                    !state.stack.is_empty()
                } else {
                    state.stack.is_empty()
                }
            }
            GameStep::PrecombatMain => active_player_id == OMNATH_ID,
            _ => false,
        }
    }

    fn select_priority_action(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Option<usize> {
        if request.player_id != OMNATH_ID {
            return request
                .options
                .iter()
                .position(|action| action.kind == ActionKind::PassPriority);
        }

        if self.should_tap_lands(state)
            && let Some((index, _)) = request.options.iter().enumerate().find(|(_, action)| {
                action.kind == ActionKind::ActivateAbility
                    && action.decisions.get("manaAbility").and_then(Value::as_bool) == Some(true)
                    && action
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|instance_id| Self::card_is_land(state, instance_id))
            })
        {
            if state.step == GameStep::Upkeep {
                let active = state.players[state.active_player].id.clone();
                *self.upkeep_land_taps.entry(active).or_default() += 1;
            }
            return Some(index);
        }

        if !self.cube_used
            && state.step == GameStep::PostcombatMain
            && let Some(index) = request.options.iter().position(|action| {
                action.kind == ActionKind::ActivateAbility
                    && action.label.to_ascii_lowercase().contains("doubling cube")
            })
        {
            self.cube_used = true;
            return Some(index);
        }

        let can_cast_now = match self.timing {
            SpellTiming::Instant | SpellTiming::Sorcery => state.step == GameStep::PrecombatMain,
        };
        if self.cube_used && !self.spell_cast && can_cast_now {
            let best = request
                .options
                .iter()
                .enumerate()
                .filter(|(_, action)| {
                    action.kind == ActionKind::CastSpell
                        && action
                            .label
                            .to_ascii_lowercase()
                            .contains(&self.spell_name.to_ascii_lowercase())
                        && self.cast_targets_match(action)
                })
                .max_by_key(|(_, action)| {
                    action
                        .decisions
                        .get("xValue")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                });
            if let Some((index, _)) = best {
                self.spell_cast = true;
                return Some(index);
            }
        }

        request
            .options
            .iter()
            .position(|action| action.kind == ActionKind::PassPriority)
    }

    fn trace_action(&mut self, state: &GameState, request: &EngineDecisionRequest, index: usize) {
        let Some(action) = request.options.get(index) else {
            return;
        };
        if self.first_decision.is_none() && request.player_id == OMNATH_ID {
            self.first_decision = Some(request.clone());
            self.first_action_index = Some(index);
        }
        let active_player_id = state.players[state.active_player].id.clone();
        let mana_before = Self::player(state, OMNATH_ID)
            .map(|player| player.mana_pool.len())
            .unwrap_or(0);
        self.action_trace.push(ActionTrace {
            decision_id: request.id.clone(),
            action_id: action.id.clone(),
            label: action.label.clone(),
            turn_number: state.turn_number,
            step: state.step.clone(),
            active_player_id,
            mana_before,
        });
    }
}

impl DecisionProvider for OmnathProvider {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let index = if request.kind == mtg_engine::engine::DecisionKind::Priority {
            self.select_priority_action(state, request)
        } else if request.id.contains("chooseOption:") {
            match request.choice.as_ref() {
                Some(DecisionChoice::OptionSelection { options, .. }) => {
                    let mode = self
                        .resolution_mode
                        .as_deref()
                        .and_then(|expected| options.iter().position(|option| option == expected));
                    if mode.is_some() {
                        mode
                    } else {
                        let expected = self.resolution_targets.front().cloned();
                        let selected = expected.as_deref().and_then(|expected| {
                            options.iter().position(|option| option == expected)
                        });
                        if selected.is_some() {
                            self.resolution_targets.pop_front();
                        }
                        selected
                    }
                }
                _ => None,
            }
        } else if request.id.contains("choosePlayer:") {
            let expected = self.resolution_targets.front().cloned();
            let selected = expected
                .as_deref()
                .and_then(|expected| match request.choice.as_ref() {
                    Some(DecisionChoice::OptionSelection { options, .. }) => {
                        options.iter().position(|option| option == expected)
                    }
                    _ => request
                        .options
                        .iter()
                        .position(|action| Self::action_contains_string(action, expected)),
                });
            if selected.is_some() {
                self.resolution_targets.pop_front();
            }
            selected
        } else if let Some(expected) = self.resolution_targets.front().cloned() {
            let selected = request.options.iter().position(|action| {
                action.targets.values().any(|target| {
                    target
                        == &TargetRef::Player {
                            player_id: expected.clone(),
                        }
                }) || Self::action_contains_string(action, &expected)
            });
            if selected.is_some() {
                self.resolution_targets.pop_front();
            }
            selected.or_else(|| {
                request
                    .options
                    .iter()
                    .position(|action| action.id.to_ascii_lowercase().contains("finish"))
            })
        } else {
            request
                .options
                .iter()
                .position(|action| action.id.to_ascii_lowercase().contains("finish"))
                .or_else(|| {
                    request
                        .options
                        .iter()
                        .position(|action| action.kind == ActionKind::PassPriority)
                })
                .or(Some(0))
        }
        .ok_or_else(|| {
            EngineError::new(format!(
                "no scripted option for {}; choice={:?}; options={:?}",
                request.id,
                request.choice,
                request
                    .options
                    .iter()
                    .map(|action| (
                        &action.id,
                        &action.label,
                        &action.decisions,
                        &action.targets
                    ))
                    .collect::<Vec<_>>()
            ))
        })?;
        self.trace_action(state, request, index);
        Ok(index)
    }

    fn choose_number(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        let Some(DecisionChoice::NumberSelection { maximum, .. }) = request.choice.as_ref() else {
            return Err(EngineError::new(format!(
                "{} is not a number selection",
                request.id
            )));
        };
        Ok(*maximum)
    }

    fn requests_explicit_priority_pass(&self, player_id: &str) -> bool {
        player_id == OMNATH_ID
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn parsed_card(
    id: &str,
    name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle_text: &str,
    is_commander: bool,
) -> Result<CardDefinition, Box<dyn std::error::Error>> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    if parsed.status != "canonical"
        || parsed.abilities.iter().any(|ability| {
            ability.status != "canonical"
                || ability
                    .rule
                    .as_ref()
                    .is_none_or(|rule| !rule_is_executable(rule))
        })
    {
        return Err(format!("{name} is not fully canonical and executable: {parsed:?}").into());
    }
    Ok(CardDefinition {
        id: id.to_string(),
        name: name.to_string(),
        type_line: type_line.to_string(),
        is_commander,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: mana_cost.to_string(),
        power: type_line.contains("Creature").then(|| "4".to_string()),
        toughness: type_line.contains("Creature").then(|| "4".to_string()),
        rules: parsed
            .abilities
            .into_iter()
            .filter_map(|ability| ability.rule)
            .collect(),
    })
}

fn basic_land(name: &str) -> CardDefinition {
    CardDefinition {
        id: format!("omnath-land-{}", name.to_ascii_lowercase()),
        name: name.to_string(),
        type_line: format!("Basic Land — {name}"),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn inert_card(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: if type_line.contains("Land") {
            String::new()
        } else {
            "{99}".to_string()
        },
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn core_cards(awakening: bool) -> Result<Vec<CardDefinition>, Box<dyn std::error::Error>> {
    let mut cards = vec![
        parsed_card(
            "omnath-locus-of-all",
            "Omnath, Locus of All",
            "Legendary Creature — Phyrexian Elemental",
            "{W}{U}{B/P}{R}{G}",
            "If you would lose unspent mana, that mana becomes black instead.",
            true,
        )?,
        parsed_card(
            "mana-reflection",
            "Mana Reflection",
            "Enchantment",
            "{4}{G}{G}",
            "If you tap a permanent for mana, it produces twice as much of that mana instead.",
            false,
        )?,
        parsed_card(
            "doubling-cube",
            "Doubling Cube",
            "Artifact",
            "{2}",
            "{3}, {T}: Double the amount of each type of unspent mana you have.",
            false,
        )?,
    ];
    if awakening {
        cards.push(parsed_card(
            "awakening",
            "Awakening",
            "Enchantment",
            "{2}{G}{G}",
            "At the beginning of each upkeep, untap all creatures and lands.",
            false,
        )?);
    }
    Ok(cards)
}

fn scenario_specs() -> Result<Vec<ScenarioSpec>, Box<dyn std::error::Error>> {
    Ok(vec![
        ScenarioSpec {
            id: "omnath-cube-draw-one",
            method: "forcedDraw",
            target_scope: "oneOpponent",
            spell: parsed_card(
                "blue-suns-zenith",
                "Blue Sun's Zenith",
                "Instant",
                "{X}{U}{U}{U}",
                "Target player draws X cards. Shuffle Blue Sun's Zenith into its owner's library.",
                false,
            )?,
            timing: SpellTiming::Instant,
            land_types: vec!["Island"; LAND_COUNT],
            required_cast_targets: BTreeMap::from([("targetPlayer", "opponent-1")]),
            resolution_mode: None,
            resolution_targets: Vec::new(),
            expected_eliminated: vec!["opponent-1"],
            opponent_life: 40,
            opponent_library_sizes: [20, 100, 100],
            awakening: false,
            maximum_turn_number: 6,
        },
        ScenarioSpec {
            id: "omnath-cube-mill-one",
            method: "mill",
            target_scope: "oneOpponent",
            spell: parsed_card(
                "drown-in-dreams",
                "Drown in Dreams",
                "Instant",
                "{X}{2}{U}",
                "Choose one. If you control a commander as you cast this spell, you may choose both instead.\n• Target player draws X cards.\n• Target player mills twice X cards.",
                false,
            )?,
            timing: SpellTiming::Instant,
            land_types: vec!["Island"; LAND_COUNT],
            required_cast_targets: BTreeMap::new(),
            resolution_mode: Some("mill"),
            resolution_targets: vec!["opponent-1"],
            expected_eliminated: vec!["opponent-1"],
            opponent_life: 40,
            opponent_library_sizes: [40, 100, 100],
            awakening: false,
            maximum_turn_number: 7,
        },
        ScenarioSpec {
            id: "omnath-cube-damage-one",
            method: "damage",
            target_scope: "oneOpponent",
            spell: parsed_card(
                "invoke-the-firemind",
                "Invoke the Firemind",
                "Sorcery",
                "{X}{U}{U}{R}",
                "Choose one —\n• Draw X cards.\n• Invoke the Firemind deals X damage to any target.",
                false,
            )?,
            timing: SpellTiming::Sorcery,
            land_types: [vec!["Island"; 4], vec!["Mountain"; 3]].concat(),
            required_cast_targets: BTreeMap::new(),
            resolution_mode: Some("damage"),
            resolution_targets: vec!["opponent-1"],
            expected_eliminated: vec!["opponent-1"],
            opponent_life: 40,
            opponent_library_sizes: [100, 100, 100],
            awakening: false,
            maximum_turn_number: 6,
        },
        ScenarioSpec {
            id: "omnath-cube-draw-damage-two",
            method: "drawAndDamage",
            target_scope: "twoOpponents",
            spell: parsed_card(
                "explosion",
                "Explosion",
                "Instant",
                "{X}{U}{U}{R}{R}",
                "Explosion deals X damage to any target. Target player draws X cards.",
                false,
            )?,
            timing: SpellTiming::Instant,
            land_types: [vec!["Island"; 4], vec!["Mountain"; 3]].concat(),
            required_cast_targets: BTreeMap::from([
                ("damageTarget", "opponent-1"),
                ("drawTarget", "opponent-2"),
            ]),
            resolution_mode: None,
            resolution_targets: Vec::new(),
            expected_eliminated: vec!["opponent-1", "opponent-2"],
            opponent_life: 40,
            opponent_library_sizes: [100, 20, 100],
            awakening: false,
            maximum_turn_number: 6,
        },
        ScenarioSpec {
            id: "omnath-cube-mill-three",
            method: "mill",
            target_scope: "eachOpponent",
            spell: parsed_card(
                "mind-grind",
                "Mind Grind",
                "Sorcery",
                "{X}{U}{B}",
                "Each opponent reveals cards from the top of their library until they reveal X land cards, then puts all cards revealed this way into their graveyard. X can't be 0.",
                false,
            )?,
            timing: SpellTiming::Sorcery,
            land_types: [vec!["Island"; 4], vec!["Swamp"; 3]].concat(),
            required_cast_targets: BTreeMap::new(),
            resolution_mode: None,
            resolution_targets: Vec::new(),
            expected_eliminated: OPPONENT_IDS.to_vec(),
            opponent_life: 40,
            opponent_library_sizes: [20, 20, 20],
            awakening: false,
            maximum_turn_number: 9,
        },
        ScenarioSpec {
            id: "omnath-cube-damage-three",
            method: "lifeLoss",
            target_scope: "eachOpponent",
            spell: parsed_card(
                "debt-to-the-deathless",
                "Debt to the Deathless",
                "Sorcery",
                "{X}{W}{W}{B}{B}",
                "Each opponent loses two times X life. You gain life equal to the life lost this way.",
                false,
            )?,
            timing: SpellTiming::Sorcery,
            land_types: [vec!["Plains"; 3], vec!["Swamp"; 4]].concat(),
            required_cast_targets: BTreeMap::new(),
            resolution_mode: None,
            resolution_targets: Vec::new(),
            expected_eliminated: OPPONENT_IDS.to_vec(),
            opponent_life: 40,
            opponent_library_sizes: [100, 100, 100],
            awakening: false,
            maximum_turn_number: 6,
        },
        ScenarioSpec {
            id: "omnath-awakening-every-upkeep-damage-three",
            method: "lifeLoss",
            target_scope: "eachOpponent",
            spell: parsed_card(
                "awakening-debt-to-the-deathless",
                "Debt to the Deathless",
                "Sorcery",
                "{X}{W}{W}{B}{B}",
                "Each opponent loses two times X life. You gain life equal to the life lost this way.",
                false,
            )?,
            timing: SpellTiming::Sorcery,
            land_types: [vec!["Plains"; 3], vec!["Swamp"; 4]].concat(),
            required_cast_targets: BTreeMap::new(),
            resolution_mode: None,
            resolution_targets: Vec::new(),
            expected_eliminated: OPPONENT_IDS.to_vec(),
            opponent_life: 150,
            opponent_library_sizes: [100, 100, 100],
            awakening: true,
            maximum_turn_number: 8,
        },
    ])
}

fn build_engine(spec: &ScenarioSpec, seed: u64) -> Result<GameEngine, Box<dyn std::error::Error>> {
    let mut omnath_cards = core_cards(spec.awakening)?;
    omnath_cards.push(spec.spell.clone());
    for land_type in &spec.land_types {
        omnath_cards.push(basic_land(land_type));
    }
    omnath_cards
        .extend((0..120).map(|index| inert_card(&format!("omnath-inert-{index}"), "Sorcery")));

    let mut players = vec![PlayerDeck {
        id: OMNATH_ID.to_string(),
        name: "Omnath".to_string(),
        starting_life: 40,
        cards: omnath_cards,
    }];
    for opponent_id in OPPONENT_IDS {
        players.push(PlayerDeck {
            id: opponent_id.to_string(),
            name: opponent_id.to_string(),
            starting_life: spec.opponent_life,
            cards: (0..120)
                .map(|index| {
                    inert_card(
                        &format!("{opponent_id}-known-land-{index}"),
                        "Basic Land — Wastes",
                    )
                })
                .collect(),
        });
    }
    let starting_player = if spec.awakening { 1 } else { 3 };
    let mut engine = GameEngine::new_with_mode(
        GameSetup {
            players,
            opening_hand_size: 0,
            starting_player,
        },
        seed,
        GameMode::Free,
    )?;
    let active_player_id = if spec.awakening {
        "opponent-1"
    } else {
        "opponent-3"
    };
    let mut battlefield_definition_ids = vec![
        "omnath-locus-of-all".to_string(),
        "mana-reflection".to_string(),
        "doubling-cube".to_string(),
    ];
    if spec.awakening {
        battlefield_definition_ids.push("awakening".to_string());
    }
    battlefield_definition_ids.extend(
        spec.land_types
            .iter()
            .map(|land_type| format!("omnath-land-{}", land_type.to_ascii_lowercase())),
    );
    let opponent_positions =
        OPPONENT_IDS
            .iter()
            .enumerate()
            .map(|(index, player_id)| DeterministicPlayerPosition {
                player_id: (*player_id).to_string(),
                life: spec.opponent_life,
                battlefield_definition_ids: Vec::new(),
                hand_definition_ids: Vec::new(),
                library_size: spec.opponent_library_sizes[index],
                mana_pool: Vec::new(),
            });
    engine.configure_deterministic_position(&DeterministicGamePosition {
        turn_number: 5,
        active_player_id: active_player_id.to_string(),
        step: GameStep::PostcombatMain,
        players: std::iter::once(DeterministicPlayerPosition {
            player_id: OMNATH_ID.to_string(),
            life: 40,
            battlefield_definition_ids,
            hand_definition_ids: vec![spec.spell.id.clone()],
            library_size: 100,
            mana_pool: Vec::new(),
        })
        .chain(opponent_positions)
        .collect(),
    })?;
    Ok(engine)
}

fn eliminated_players(state: &GameState) -> BTreeSet<String> {
    state
        .players
        .iter()
        .filter(|player| player.id != OMNATH_ID && player.has_lost)
        .map(|player| player.id.clone())
        .collect()
}

fn run_scenario(
    spec: &ScenarioSpec,
    seed: u64,
    skip_upkeep_player: Option<&str>,
) -> Result<(GameState, GameState, OmnathProvider), Box<dyn std::error::Error>> {
    let mut engine = build_engine(spec, seed)?;
    let initial_state = engine.state().clone();
    let mut provider = OmnathProvider::new(spec, skip_upkeep_player);
    engine.run_from_postcombat_main(&mut provider, spec.maximum_turn_number)?;
    Ok((initial_state, engine.state().clone(), provider))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seed = 20_260_810_u64;
    let output = workspace_root()
        .join(".local-app")
        .join("omnath-mana-scenarios.json");
    let started = Instant::now();
    let mut scenarios = Vec::<Value>::new();

    for (index, spec) in scenario_specs()?.into_iter().enumerate() {
        let scenario_seed = seed.wrapping_add(index as u64);
        let (initial_state, final_state, provider) = run_scenario(&spec, scenario_seed, None)?;
        let actual_eliminated = eliminated_players(&final_state);
        let expected_eliminated = spec
            .expected_eliminated
            .iter()
            .map(|player| (*player).to_string())
            .collect::<BTreeSet<_>>();
        if !expected_eliminated.is_subset(&actual_eliminated) {
            let player_diagnostics = final_state
                .players
                .iter()
                .map(|player| {
                    (
                        player.id.as_str(),
                        player.life,
                        player.library.len(),
                        player.hand.len(),
                        player.has_lost,
                    )
                })
                .collect::<Vec<_>>();
            return Err(format!(
                "{} eliminated {:?}, expected at least {:?}; players={player_diagnostics:?}; actions={:?}; events={:?}",
                spec.id,
                actual_eliminated,
                expected_eliminated,
                provider.action_trace,
                final_state
                    .events
                    .iter()
                    .map(|event| event.kind.as_str())
                    .collect::<Vec<_>>(),
            )
            .into());
        }
        let cube_event = final_state
            .events
            .iter()
            .find(|event| event.kind == "manaPoolMultiplied")
            .ok_or_else(|| format!("{} never resolved Doubling Cube", spec.id))?;
        if cube_event.detail["productionFactor"] != 2 || cube_event.detail["resultingMana"] != 33 {
            return Err(format!(
                "{} did not triple the eleven mana remaining after the Cube cost: {}",
                spec.id, cube_event.detail
            )
            .into());
        }
        let spell_event = final_state
            .events
            .iter()
            .find(|event| {
                event.kind == "spellCast"
                    && event
                        .card_instance_id
                        .as_deref()
                        .is_some_and(|instance_id| {
                            initial_state.players[0].hand.iter().any(|card| {
                                card.instance_id == instance_id
                                    && card.definition.id == spec.spell.id
                            })
                        })
            })
            .ok_or_else(|| format!("{} never cast its X spell", spec.id))?;
        let x_value = spell_event.detail["decisions"]["xValue"]
            .as_i64()
            .ok_or_else(|| format!("{} cast has no X value", spec.id))?;
        let retained_mana_before_own_turn = provider
            .action_trace
            .iter()
            .find(|action| action.active_player_id == OMNATH_ID)
            .map(|action| action.mana_before)
            .unwrap_or(0);
        if retained_mana_before_own_turn < 33 {
            return Err(format!(
                "{} did not retain the Cubed mana into Omnath's turn",
                spec.id
            )
            .into());
        }

        let mut counterfactuals = Vec::<Value>::new();
        if spec.awakening {
            for upkeep_player in ["opponent-2", "opponent-3", OMNATH_ID] {
                if provider.upkeep_land_taps.get(upkeep_player).copied() != Some(LAND_COUNT) {
                    return Err(format!(
                        "{} tapped lands {:?} times during {upkeep_player}'s upkeep, expected {LAND_COUNT}",
                        spec.id,
                        provider.upkeep_land_taps.get(upkeep_player)
                    )
                    .into());
                }
                let (_, counterfactual_state, counterfactual_provider) =
                    run_scenario(&spec, scenario_seed, Some(upkeep_player))?;
                let counterfactual_eliminated = eliminated_players(&counterfactual_state);
                let counterfactual_x = counterfactual_state
                    .events
                    .iter()
                    .find(|event| event.kind == "spellCast")
                    .and_then(|event| event.detail["decisions"]["xValue"].as_i64())
                    .unwrap_or(0);
                if !counterfactual_eliminated.is_empty() {
                    return Err(format!(
                        "{} remains lethal while skipping {upkeep_player}'s upkeep: {:?}; x={counterfactual_x}; taps={:?}; life={:?}",
                        spec.id,
                        counterfactual_eliminated,
                        counterfactual_provider.upkeep_land_taps,
                        counterfactual_state
                            .players
                            .iter()
                            .map(|player| (&player.id, player.life, player.has_lost))
                            .collect::<Vec<_>>(),
                    )
                    .into());
                }
                counterfactuals.push(json!({
                    "skippedUpkeepPlayerId": upkeep_player,
                    "xValue": counterfactual_x,
                    "eliminatedPlayers": counterfactual_eliminated,
                    "upkeepLandTaps": counterfactual_provider.upkeep_land_taps,
                    "lethal": false,
                }));
            }
        }

        scenarios.push(json!({
            "schemaVersion": "mtg-deterministic-omnath-mana-scenario/v1",
            "id": spec.id,
            "method": spec.method,
            "targetScope": spec.target_scope,
            "contract": {
                "playerCount": 4,
                "opponentCount": 3,
                "startingStep": "postcombatMain",
                "startingActivePlayerId": if spec.awakening { "opponent-1" } else { "opponent-3" },
                "opponentOpeningHands": 0,
                "hiddenOpponentActions": false,
                "usesOmnathManaRetention": true,
                "usesManaReflection": true,
                "usesDoublingCube": true,
                "usesAwakening": spec.awakening,
            },
            "spell": spec.spell,
            "initialSession": {
                "schemaVersion": "mtg-game-session-snapshot/v1",
                "state": initial_state,
                "decision": provider.first_decision,
            },
            "proof": {
                "firstActionIndex": provider.first_action_index,
                "xValue": x_value,
                "cubeProductionFactor": cube_event.detail["productionFactor"],
                "manaAfterCube": cube_event.detail["resultingMana"],
                "retainedManaObservedOnOmnathTurn": retained_mana_before_own_turn,
                "expectedEliminatedPlayers": expected_eliminated,
                "actualEliminatedPlayers": actual_eliminated,
                "winner": final_state.outcome.as_ref().and_then(|outcome| outcome.winner.clone()),
                "upkeepLandTaps": provider.upkeep_land_taps,
                "actionTrace": provider.action_trace,
                "counterfactuals": counterfactuals,
            },
        }));
    }

    let payload = json!({
        "schemaVersion": "mtg-deterministic-omnath-mana-scenarios/v1",
        "generationSeed": seed,
        "generationElapsedMs": started.elapsed().as_millis(),
        "scenarioCount": scenarios.len(),
        "scenarios": scenarios,
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&payload)?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "output": output.canonicalize().unwrap_or(output),
            "scenarioCount": payload["scenarioCount"],
            "generationElapsedMs": payload["generationElapsedMs"],
            "scenarios": payload["scenarios"].as_array().into_iter().flatten().map(|scenario| json!({
                "id": scenario["id"],
                "method": scenario["method"],
                "targetScope": scenario["targetScope"],
                "xValue": scenario["proof"]["xValue"],
                "eliminatedPlayers": scenario["proof"]["actualEliminatedPlayers"],
            })).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
