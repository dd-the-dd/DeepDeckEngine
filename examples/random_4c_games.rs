use mtg_engine::engine::{
    CardDefinition, GameEngine, GameSetup, PlayerDeck, RandomSimulationRequest,
    simulate_random_games, simulate_traced_random_game,
};
use mtg_engine::model::{PlayableCardInput, compile_playable_card};
use mtg_engine::oracle::{OracleCardFace, OracleCardParseRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_json(path: &Path) -> Value {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn argument(name: &str, default: u64) -> u64 {
    let arguments = std::env::args().collect::<Vec<_>>();
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .and_then(|pair| pair[1].parse::<u64>().ok())
        .unwrap_or(default)
}

fn string_argument(name: &str) -> Option<String> {
    let arguments = std::env::args().collect::<Vec<_>>();
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn optional_string(value: &Value) -> Option<String> {
    value.as_str().map(ToOwned::to_owned)
}

fn request_from_truth(truth: &Value) -> OracleCardParseRequest {
    let context = &truth["context"];
    OracleCardParseRequest {
        card_name: context["name"]
            .as_str()
            .expect("card context name")
            .to_string(),
        faces: context["faces"]
            .as_array()
            .map(|faces| {
                faces
                    .iter()
                    .map(|face| OracleCardFace {
                        id: face["id"].as_str().expect("face id").to_string(),
                        mana_cost: optional_string(&face["manaCost"]),
                        name: face["name"].as_str().expect("face name").to_string(),
                        oracle_text: face["oracleText"]
                            .as_str()
                            .expect("face Oracle text")
                            .to_string(),
                        power: optional_string(&face["power"]),
                        toughness: optional_string(&face["toughness"]),
                        type_line: face["typeLine"]
                            .as_str()
                            .expect("face type line")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        layout: optional_string(&context["layout"]),
        mana_cost: optional_string(&context["manaCost"]),
        oracle_text: optional_string(&context["oracleText"]),
        type_line: context["typeLine"]
            .as_str()
            .expect("card context type line")
            .to_string(),
    }
}

fn definition_from_truth(card_id: &str, truth: &Value) -> (CardDefinition, Value) {
    let context = &truth["context"];
    let primary_face = context["faces"].as_array().and_then(|faces| faces.first());
    let face_id = primary_face.and_then(|face| face["id"].as_str());
    let card_context = primary_face.unwrap_or(context);
    let expected_rules = truth["abilities"]
        .as_array()
        .expect("ground-truth abilities")
        .iter()
        .filter(|ability| {
            face_id.is_none()
                || ability["source"]["faceId"].as_str() == face_id
                || ability["source"]["faceId"].is_null()
        })
        .map(|ability| ability["expectedRule"].clone())
        .collect::<Vec<_>>();
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: face_id.map(ToOwned::to_owned),
        id: card_id.to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: request_from_truth(truth),
        power: card_context["power"].as_str().map(ToOwned::to_owned),
        toughness: card_context["toughness"].as_str().map(ToOwned::to_owned),
    })
    .unwrap_or_else(|error| panic!("{card_id} playable compilation failed: {error}"));
    let truth_matches = compilation.card.rules == expected_rules;
    let audit = json!({
        "cardId": card_id,
        "cardName": compilation.card.name,
        "parserStatus": compilation.parser_status,
        "canonicalAbilityCount": compilation.canonical_ability_count,
        "unsupportedAbilityCount": compilation.unsupported_ability_count,
        "groundTruthMatches": truth_matches,
        "parserDiagnostics": compilation.parser_result.diagnostics,
    });
    (compilation.card, audit)
}

fn load_mainboard() -> (Vec<CardDefinition>, Vec<Value>) {
    let root = workspace_root();
    let manifest = read_json(
        &root
            .join("fixtures")
            .join("oracle-ground-truth")
            .join("decks")
            .join("4c-control.json"),
    );
    let truth_directory = root
        .join("fixtures")
        .join("oracle-ground-truth")
        .join("cards");
    let mut cards = Vec::new();
    let mut parser_audit = Vec::new();
    for entry in manifest["cards"]
        .as_array()
        .expect("ground-truth deck cards")
        .iter()
        .filter(|entry| entry["section"] == "mainboard")
    {
        let card_id = entry["id"].as_str().expect("card ID");
        let truth = read_json(&truth_directory.join(format!("{card_id}.json")));
        let (definition, audit) = definition_from_truth(card_id, &truth);
        parser_audit.push(audit);
        let quantity = entry["quantity"].as_u64().expect("card quantity");
        cards.extend((0..quantity).map(|_| definition.clone()));
    }
    assert_eq!(
        cards.len(),
        60,
        "4c control mainboard should contain 60 cards"
    );
    (cards, parser_audit)
}

fn main() {
    let games = argument("--games", 100) as usize;
    let seed = argument("--seed", 20260724);
    let max_turns = argument("--max-turns", 160) as u32;
    let player_count = argument("--players", 2) as usize;
    let trace_output = string_argument("--trace-output");
    assert!(
        (2..=4).contains(&player_count),
        "--players must be between 2 and 4"
    );
    let (cards, parser_audit) = load_mainboard();
    let setup = GameSetup {
        opening_hand_size: 7,
        players: (0..player_count)
            .map(|index| PlayerDeck {
                cards: cards.clone(),
                id: format!("player-{}", index + 1),
                name: format!("Random client {}", index + 1),
                starting_life: 20,
            })
            .collect(),
        starting_player: 0,
    };
    let coverage_engine =
        GameEngine::new(setup.clone(), seed).expect("ground-truth game setup is valid");
    let unsupported_rules = coverage_engine.state().unsupported_rules.clone();
    let summary = simulate_random_games(RandomSimulationRequest {
        games,
        max_turns,
        seed,
        setup: setup.clone(),
    })
    .expect("random simulations complete");
    let trace_report = trace_output.map(|output| {
        let trace = simulate_traced_random_game(setup.clone(), seed, max_turns)
            .expect("traced random simulation completes");
        let path = PathBuf::from(&output);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));
        }
        fs::write(
            &path,
            serde_json::to_string_pretty(&trace).expect("trace serializes"),
        )
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
        json!({
            "path": path,
            "decisionCount": trace.decisions.len(),
            "eventCount": trace.events.len(),
            "findingCount": trace.findings.len(),
        })
    });
    let minimum_turns = summary
        .games
        .iter()
        .map(|game| game.turns)
        .min()
        .unwrap_or(0);
    let maximum_turns = summary
        .games
        .iter()
        .map(|game| game.turns)
        .max()
        .unwrap_or(0);
    let average_turns = summary
        .games
        .iter()
        .map(|game| game.turns as f64)
        .sum::<f64>()
        / summary.games.len().max(1) as f64;
    let event_counts =
        summary
            .games
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut totals, game| {
                for (kind, count) in &game.event_counts {
                    *totals.entry(kind.clone()).or_default() += count;
                }
                totals
            });
    let canonical_cards = parser_audit
        .iter()
        .filter(|card| card["parserStatus"] == "canonical")
        .count();
    let canonical_abilities = parser_audit
        .iter()
        .map(|card| card["canonicalAbilityCount"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let unsupported_abilities = parser_audit
        .iter()
        .map(|card| card["unsupportedAbilityCount"].as_u64().unwrap_or(0))
        .sum::<u64>();
    let ground_truth_mismatches = parser_audit
        .iter()
        .filter(|card| card["groundTruthMatches"] != true)
        .cloned()
        .collect::<Vec<_>>();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schemaVersion": "mtg-random-simulation-report/v1",
            "deck": "4c control",
            "mode": if player_count == 2 { "duel" } else { "freeForAll" },
            "playerCount": player_count,
            "seed": seed,
            "requestedGames": summary.requested_games,
            "completedGames": summary.completed_games,
            "lifeTotalGames": summary.life_total_games,
            "emptyLibraryGames": summary.empty_library_games,
            "simultaneousGames": summary.simultaneous_games,
            "stalledGames": summary.stalled_games,
            "auditFindingCount": summary.audit_finding_count,
            "events": event_counts,
            "turns": {
                "minimum": minimum_turns,
                "maximum": maximum_turns,
                "average": average_turns,
            },
            "engineCoverage": {
                "unsupportedRuleCount": unsupported_rules.len(),
                "unsupportedRules": unsupported_rules,
            },
            "parserPipeline": {
                "cardCount": parser_audit.len(),
                "canonicalCardCount": canonical_cards,
                "canonicalAbilityCount": canonical_abilities,
                "unsupportedAbilityCount": unsupported_abilities,
                "groundTruthMismatchCount": ground_truth_mismatches.len(),
                "groundTruthMismatches": ground_truth_mismatches,
            },
            "games": summary.games,
            "trace": trace_report,
        }))
        .expect("simulation report serializes")
    );
}
