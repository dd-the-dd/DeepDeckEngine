use mtg_engine::engine::{CardDefinition, GameEndReason, PlayerDeck};
use mtg_engine::oracle::{OracleCardParseRequest, parse_oracle_card};
use mtg_engine::punching_bag::{
    PUNCHING_BAG_PLAYER_ID, ProgressivePunchingBagConfig, PunchingBagPosition,
    progressively_find_punching_bag_win,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct DeterministicScenario {
    id: &'static str,
    win_condition: &'static str,
    outcome_reason: GameEndReason,
    expected_winning_turn: u32,
    position: Option<PunchingBagPosition>,
    learner_deck: PlayerDeck,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn arguments() -> Vec<String> {
    std::env::args().collect()
}

fn numeric_argument(name: &str, default: u64) -> u64 {
    arguments()
        .windows(2)
        .find(|pair| pair[0] == name)
        .and_then(|pair| pair[1].parse::<u64>().ok())
        .unwrap_or(default)
}

fn string_argument(name: &str) -> Option<String> {
    arguments()
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn path_argument(name: &str, default: PathBuf) -> PathBuf {
    string_argument(name).map(PathBuf::from).unwrap_or(default)
}

fn inert_card(index: usize) -> CardDefinition {
    CardDefinition {
        id: format!("deterministic-inert-{index}"),
        name: format!("Inert Test Card {index}"),
        type_line: "Sorcery".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{99}".to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn exact_hand_with(winning_card: CardDefinition) -> Vec<CardDefinition> {
    let mut cards = (0..6).map(inert_card).collect::<Vec<_>>();
    cards.push(winning_card);
    cards
}

fn keyword(kind: &str) -> Value {
    json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": kind },
    })
}

fn combat_scenario() -> DeterministicScenario {
    let finisher = CardDefinition {
        id: "deterministic-combat-finisher".to_string(),
        name: "Deterministic Combat Finisher".to_string(),
        type_line: "Creature — Avatar".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: Some("20".to_string()),
        toughness: Some("20".to_string()),
        rules: vec![keyword("haste")],
    };
    DeterministicScenario {
        id: "punching-bag-combat-damage",
        win_condition: "combatDamage",
        outcome_reason: GameEndReason::LifeTotal,
        expected_winning_turn: 1,
        position: None,
        learner_deck: PlayerDeck {
            id: "deterministic-combat-learner".to_string(),
            name: "Deterministic Combat Damage".to_string(),
            starting_life: 20,
            cards: exact_hand_with(finisher),
        },
    }
}

fn direct_damage_scenario() -> DeterministicScenario {
    let burn = CardDefinition {
        id: "deterministic-direct-damage".to_string(),
        name: "Deterministic Direct Damage".to_string(),
        type_line: "Sorcery".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: None,
        toughness: None,
        rules: vec![json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "dealDamageToEachOpponent",
                "amount": { "kind": "integer", "value": 20 },
            }],
        })],
    };
    DeterministicScenario {
        id: "punching-bag-direct-damage",
        win_condition: "directDamage",
        outcome_reason: GameEndReason::LifeTotal,
        expected_winning_turn: 1,
        position: None,
        learner_deck: PlayerDeck {
            id: "deterministic-direct-damage-learner".to_string(),
            name: "Deterministic Direct Damage".to_string(),
            starting_life: 20,
            cards: exact_hand_with(burn),
        },
    }
}

fn empty_library_draw_scenario() -> DeterministicScenario {
    let forced_draw = CardDefinition {
        id: "deterministic-forced-draw".to_string(),
        name: "Deterministic Forced Draw".to_string(),
        type_line: "Sorcery".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: None,
        toughness: None,
        rules: vec![json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetOpponent",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "players",
                        "where": {
                            "kind": "isOpponentOf",
                            "player": { "kind": "abilityController" },
                        },
                    },
                }],
            },
            "effects": [{
                "kind": "drawCards",
                "player": { "kind": "chosenTarget", "id": "targetOpponent" },
                "count": { "kind": "integer", "value": 100 },
            }],
        })],
    };
    DeterministicScenario {
        id: "punching-bag-empty-library-draw",
        win_condition: "drawFromEmptyLibrary",
        outcome_reason: GameEndReason::DrawFromEmptyLibrary,
        expected_winning_turn: 1,
        position: None,
        learner_deck: PlayerDeck {
            id: "deterministic-forced-draw-learner".to_string(),
            name: "Deterministic Empty-Library Draw".to_string(),
            starting_life: 20,
            cards: exact_hand_with(forced_draw),
        },
    }
}

fn commander_damage_scenario() -> DeterministicScenario {
    let commander = CardDefinition {
        id: "deterministic-commander-finisher".to_string(),
        name: "Deterministic Commander Finisher".to_string(),
        type_line: "Legendary Creature — Avatar".to_string(),
        is_commander: true,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: Some("21".to_string()),
        toughness: Some("21".to_string()),
        rules: vec![keyword("haste")],
    };
    let mut cards = (0..7).map(inert_card).collect::<Vec<_>>();
    cards.push(commander);
    DeterministicScenario {
        id: "punching-bag-commander-damage",
        win_condition: "commanderDamage",
        outcome_reason: GameEndReason::CommanderDamage,
        expected_winning_turn: 1,
        position: None,
        learner_deck: PlayerDeck {
            id: "deterministic-commander-learner".to_string(),
            name: "Deterministic Commander Damage".to_string(),
            starting_life: 40,
            cards,
        },
    }
}

fn island(index: usize) -> CardDefinition {
    CardDefinition {
        id: format!("tidecaller-island-{index}"),
        name: "Island".to_string(),
        type_line: "Basic Land — Island".to_string(),
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

fn exhibition_tidecaller_scenario() -> Result<DeterministicScenario, Box<dyn std::error::Error>> {
    let oracle_text = "Opus — Whenever you cast an instant or sorcery spell, target player mills three cards. If five or more mana was spent to cast that spell, that player mills ten cards instead.";
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Exhibition Tidecaller".to_string(),
        type_line: "Creature — Djinn Wizard".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    if parsed.status != "canonical" {
        return Err("Exhibition Tidecaller did not parse canonically".into());
    }
    let tidecaller_rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect::<Vec<_>>();
    if !tidecaller_rules
        .iter()
        .any(|rule| rule["kind"] == "triggeredAbility")
    {
        return Err(format!(
            "Exhibition Tidecaller has no canonical triggered ability: {tidecaller_rules:?}"
        )
        .into());
    }
    let tidecaller = CardDefinition {
        id: "exhibition-tidecaller".to_string(),
        name: "Exhibition Tidecaller".to_string(),
        type_line: "Creature — Djinn Wizard".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{U}".to_string(),
        power: Some("0".to_string()),
        toughness: Some("2".to_string()),
        rules: tidecaller_rules,
    };
    let five_mana_sorcery = CardDefinition {
        id: "tidecaller-five-mana-sorcery".to_string(),
        name: "Five-Mana Exhibition".to_string(),
        type_line: "Sorcery".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{4}{U}".to_string(),
        power: None,
        toughness: None,
        rules: vec![json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "gainLife",
                "player": { "kind": "abilityController" },
                "amount": { "kind": "integer", "value": 1 },
            }],
        })],
    };
    let celestial = CardDefinition {
        id: "punching-bag-celestial".to_string(),
        name: "Punching Bag Celestial".to_string(),
        type_line: "Creature — Celestial".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{10}".to_string(),
        power: Some("10".to_string()),
        toughness: Some("10".to_string()),
        rules: vec![keyword("flying")],
    };
    let islands = (1..=5).map(island).collect::<Vec<_>>();
    let inert_cards = (0..5).map(inert_card).collect::<Vec<_>>();
    let mut cards = islands.clone();
    cards.push(tidecaller);
    cards.push(five_mana_sorcery);
    cards.extend(inert_cards.clone());

    Ok(DeterministicScenario {
        id: "punching-bag-exhibition-tidecaller",
        win_condition: "tidecallerOpusDeckOut",
        outcome_reason: GameEndReason::DrawFromEmptyLibrary,
        expected_winning_turn: 2,
        position: Some(PunchingBagPosition {
            learner_battlefield_definition_ids: vec![
                islands[0].id.clone(),
                islands[1].id.clone(),
                islands[2].id.clone(),
                islands[3].id.clone(),
                "exhibition-tidecaller".to_string(),
            ],
            learner_hand_definition_ids: vec![
                islands[4].id.clone(),
                "tidecaller-five-mana-sorcery".to_string(),
                inert_cards[0].id.clone(),
                inert_cards[1].id.clone(),
                inert_cards[2].id.clone(),
                inert_cards[3].id.clone(),
                inert_cards[4].id.clone(),
            ],
            opponent_battlefield: vec![celestial],
            opponent_library_size: 10,
            opponent_skips_draw_step: false,
        }),
        learner_deck: PlayerDeck {
            id: "deterministic-tidecaller-learner".to_string(),
            name: "Deterministic Exhibition Tidecaller".to_string(),
            starting_life: 20,
            cards,
        },
    })
}

fn scenario_definitions() -> Result<Vec<DeterministicScenario>, Box<dyn std::error::Error>> {
    Ok(vec![
        combat_scenario(),
        direct_damage_scenario(),
        empty_library_draw_scenario(),
        commander_damage_scenario(),
        exhibition_tidecaller_scenario()?,
    ])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seed = numeric_argument("--seed", 20_260_810);
    let maximum_unique_nodes = numeric_argument("--max-nodes", 1_000_000) as usize;
    let requested_scenario = string_argument("--scenario");
    let output = path_argument(
        "--output",
        workspace_root()
            .join(".local-app")
            .join("punching-bag-scenarios.json"),
    );
    let definitions = scenario_definitions()?
        .into_iter()
        .filter(|scenario| {
            requested_scenario
                .as_ref()
                .is_none_or(|requested| scenario.id.eq_ignore_ascii_case(requested))
        })
        .collect::<Vec<_>>();
    if definitions.is_empty() {
        return Err(format!(
            "unknown deterministic scenario {}",
            requested_scenario.as_deref().unwrap_or_default()
        )
        .into());
    }

    let started = Instant::now();
    let mut scenarios = Vec::<Value>::new();
    for (index, definition) in definitions.into_iter().enumerate() {
        let opponent_skips_draw_step = definition
            .position
            .as_ref()
            .map(|position| position.opponent_skips_draw_step)
            .unwrap_or(true);
        let pace_tag = if definition.expected_winning_turn == 1 {
            "turn-one-win"
        } else {
            "next-draw-win"
        };
        let report = progressively_find_punching_bag_win(
            definition.learner_deck.clone(),
            ProgressivePunchingBagConfig {
                seed: seed.wrapping_add(index as u64),
                maximum_segments: 1,
                maximum_turns: 3,
                maximum_unique_nodes,
                maximum_depth: 128,
                maximum_choices_per_node: 4_096,
                position: definition.position.clone(),
            },
        )?;
        if !report.tree.complete
            || report.tree.learner_win_leaves == 0
            || report.winning_segment_index != 0
            || report.winning_witness_source != "exhaustiveTree"
        {
            return Err(format!(
                "{} does not have a complete deterministic winning proof",
                definition.id
            )
            .into());
        }
        if report.winning_outcome.reason != definition.outcome_reason {
            return Err(format!(
                "{} ended for {:?}, expected {:?}",
                definition.id, report.winning_outcome.reason, definition.outcome_reason
            )
            .into());
        }
        if report.winning_turn != definition.expected_winning_turn {
            return Err(format!(
                "{} won on turn {}, expected turn {}",
                definition.id, report.winning_turn, definition.expected_winning_turn
            )
            .into());
        }
        let opponent = report
            .root_state
            .players
            .iter()
            .find(|player| player.id == PUNCHING_BAG_PLAYER_ID)
            .ok_or("punching bag is absent from scenario root")?;
        let learner = report
            .root_state
            .players
            .iter()
            .find(|player| player.id == definition.learner_deck.id)
            .ok_or("learner is absent from scenario root")?;
        if !opponent.hand.is_empty() || learner.hand.len() != 7 {
            return Err(format!(
                "{} violates the known-hand contract: learner={}, opponent={}",
                definition.id,
                learner.hand.len(),
                opponent.hand.len()
            )
            .into());
        }
        let final_segment = report
            .progression
            .last()
            .ok_or("deterministic scenario has no exploration segment")?;
        let minimum_opponent_library_size = final_segment.minimum_opponent_library_size;
        let maximum_observed_mill_count = final_segment.maximum_observed_mill_count;
        let maximum_observed_mana_spent = final_segment.maximum_observed_mana_spent;
        let draw_failed_leaf_count = final_segment.draw_failed_leaf_count;
        let draw_skipped_leaf_count = final_segment.draw_skipped_leaf_count;
        if definition.id == "punching-bag-exhibition-tidecaller" {
            let has_tidecaller = learner
                .battlefield
                .iter()
                .any(|card| card.definition.id == "exhibition-tidecaller");
            let battlefield_islands = learner
                .battlefield
                .iter()
                .filter(|card| card.definition.type_line.contains("Island"))
                .count();
            let has_fifth_island = learner
                .hand
                .iter()
                .any(|card| card.definition.id == "tidecaller-island-5");
            let has_five_mana_sorcery = learner
                .hand
                .iter()
                .any(|card| card.definition.id == "tidecaller-five-mana-sorcery");
            let celestial = opponent
                .battlefield
                .iter()
                .find(|card| card.definition.id == "punching-bag-celestial")
                .ok_or("Tidecaller scenario has no opposing Celestial")?;
            let celestial_has_flying = celestial.definition.rules.iter().any(|rule| {
                rule["kind"] == "flying"
                    || (rule["kind"] == "keywordAbility" && rule["ability"]["kind"] == "flying")
                    || (rule["kind"] == "keyword"
                        && rule["keyword"]
                            .as_str()
                            .is_some_and(|keyword| keyword.eq_ignore_ascii_case("flying")))
            });
            if !has_tidecaller
                || battlefield_islands != 4
                || !has_fifth_island
                || !has_five_mana_sorcery
                || opponent.library.len() != 10
                || celestial.definition.power.as_deref() != Some("10")
                || celestial.definition.toughness.as_deref() != Some("10")
                || !celestial_has_flying
            {
                return Err(
                    "Exhibition Tidecaller root position violates its scenario contract".into(),
                );
            }
            if maximum_observed_mana_spent < 5
                || maximum_observed_mill_count != 10
                || minimum_opponent_library_size != 0
                || draw_failed_leaf_count == 0
                || draw_skipped_leaf_count != 0
            {
                return Err(format!(
                    "Exhibition Tidecaller proof is incomplete: maxManaSpent={maximum_observed_mana_spent}, maxMill={maximum_observed_mill_count}, minLibrary={minimum_opponent_library_size}, drawFailedLeaves={draw_failed_leaf_count}, drawSkippedLeaves={draw_skipped_leaf_count}"
                )
                .into());
            }
        }
        scenarios.push(json!({
            "schemaVersion": "mtg-deterministic-punching-bag-scenario/v1",
            "id": definition.id,
            "winCondition": definition.win_condition,
            "tags": [
                "punching-bag",
                "deterministic",
                "complete-tree",
                "zero-mulligan",
                "known-information",
                pace_tag,
            ],
            "contract": {
                "learnerOpeningHandSize": 7,
                "learnerMulligansTaken": 0,
                "opponentOpeningHandSize": 0,
                "opponentSkipsDrawStep": opponent_skips_draw_step,
                "hiddenOpponentActions": false,
                "maximumWinningTurn": definition.expected_winning_turn,
                "completeTreeRequired": true,
            },
            "learnerDeck": definition.learner_deck,
            "punchingBag": report.punching_bag,
            "proof": {
                "winningTurn": report.winning_turn,
                "winningOutcome": report.winning_outcome,
                "winningLine": report.known_winning_line,
                "learnerWinLeaves": report.tree.learner_win_leaves,
                "totalLeaves": report.tree.total_leaves,
                "exactForFixedRandomTape": report.tree.exact_for_fixed_random_tape,
                "complete": report.tree.complete,
                "maximumObservedManaSpent": maximum_observed_mana_spent,
                "maximumObservedMillCount": maximum_observed_mill_count,
                "minimumOpponentLibrarySize": minimum_opponent_library_size,
                "drawFailedLeafCount": draw_failed_leaf_count,
                "drawSkippedLeafCount": draw_skipped_leaf_count,
            },
            "initialSession": {
                "schemaVersion": "mtg-game-session-snapshot/v1",
                "state": report.root_state,
                "decision": report.root_decision,
            },
            "rootChoices": report.root_choices,
            "tree": report.tree,
        }));
    }

    let payload = json!({
        "schemaVersion": "mtg-deterministic-punching-bag-scenarios/v1",
        "generationSeed": seed,
        "scenarioCount": scenarios.len(),
        "generationElapsedMs": started.elapsed().as_millis(),
        "contract": {
            "allScenariosDeterministic": true,
            "allTreesComplete": true,
            "allScenariosHaveWinningLeaf": true,
            "learnerOpeningHandSize": 7,
            "learnerMulligansTaken": 0,
            "opponentOpeningHandSize": 0,
            "opponentDrawPolicyRecordedPerScenario": true,
        },
        "scenarios": scenarios,
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&payload)?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schemaVersion": payload["schemaVersion"],
            "output": output.canonicalize().unwrap_or(output),
            "scenarioCount": payload["scenarioCount"],
            "generationElapsedMs": payload["generationElapsedMs"],
            "scenarios": payload["scenarios"].as_array().into_iter().flatten().map(|scenario| json!({
                "id": scenario["id"],
                "winCondition": scenario["winCondition"],
                "winningTurn": scenario["proof"]["winningTurn"],
                "totalLeaves": scenario["proof"]["totalLeaves"],
                "learnerWinLeaves": scenario["proof"]["learnerWinLeaves"],
                "complete": scenario["proof"]["complete"],
            })).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}
