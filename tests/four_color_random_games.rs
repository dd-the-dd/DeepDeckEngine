use mtg_engine::engine::{
    CardDefinition, GameEngine, GameSetup, PlayerDeck, RandomSimulationRequest,
    simulate_random_games,
};
use mtg_engine::model::{PlayableCardInput, compile_playable_card};
use mtg_engine::oracle::{OracleCardFace, OracleCardParseRequest};
use serde_json::{Value, json};
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

fn definition_from_truth(card_id: &str, truth: &Value) -> CardDefinition {
    let context = &truth["context"];
    let primary_face = context["faces"].as_array().and_then(|faces| faces.first());
    let face_id = primary_face.and_then(|face| face["id"].as_str());
    let card_context = primary_face.unwrap_or(context);
    let mut expected_rules = truth["abilities"]
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
    if context["layout"] == "prepare"
        && let Some(spell_face) = context["faces"].as_array().and_then(|faces| faces.get(1))
    {
        let spell_face_id = spell_face["id"].as_str().expect("prepare spell face ID");
        let spell_rules = truth["abilities"]
            .as_array()
            .expect("ground-truth abilities")
            .iter()
            .filter(|ability| ability["source"]["faceId"].as_str() == Some(spell_face_id))
            .map(|ability| ability["expectedRule"].clone())
            .collect::<Vec<_>>();
        expected_rules.push(json!({
            "kind": "prepareSpell",
            "spell": {
                "id": spell_face_id,
                "name": spell_face["name"],
                "typeLine": spell_face["typeLine"],
                "manaCost": spell_face["manaCost"],
                "rules": spell_rules,
            }
        }));
    }
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

    assert_eq!(
        compilation.card.rules, expected_rules,
        "{card_id} production parser rules diverged from reviewed truth"
    );
    compilation.card
}

fn mainboard() -> Vec<CardDefinition> {
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
    for entry in manifest["cards"]
        .as_array()
        .expect("ground-truth deck cards")
        .iter()
        .filter(|entry| entry["section"] == "mainboard")
    {
        let card_id = entry["id"].as_str().expect("card ID");
        let definition = definition_from_truth(
            card_id,
            &read_json(&truth_directory.join(format!("{card_id}.json"))),
        );
        let quantity = entry["quantity"].as_u64().expect("card quantity");
        cards.extend((0..quantity).map(|_| definition.clone()));
    }
    assert_eq!(cards.len(), 60);
    cards
}

/// Feature: The reviewed 4c deck completes seeded random games without diagnostic stalls.
#[test]
fn reviewed_four_color_deck_completes_random_games() {
    let cards = mainboard();
    let summary = simulate_random_games(RandomSimulationRequest {
        games: 12,
        max_turns: 160,
        seed: 20260724,
        setup: GameSetup {
            opening_hand_size: 7,
            starting_player: 0,
            players: vec![
                PlayerDeck {
                    id: "player-one".to_string(),
                    name: "Random AI 1".to_string(),
                    starting_life: 20,
                    cards: cards.clone(),
                },
                PlayerDeck {
                    id: "player-two".to_string(),
                    name: "Random AI 2".to_string(),
                    starting_life: 20,
                    cards,
                },
            ],
        },
    })
    .expect("reviewed deck simulations run");

    assert_eq!(summary.completed_games, 12);
    assert_eq!(summary.stalled_games, 0);
    assert_eq!(
        summary.life_total_games + summary.empty_library_games,
        summary.completed_games
    );
    assert!(
        summary
            .games
            .iter()
            .map(|game| game.event_counts.get("spellCast").copied().unwrap_or(0))
            .sum::<usize>()
            > 0
    );
    assert!(
        summary
            .games
            .iter()
            .map(|game| game.event_counts.get("combatDamage").copied().unwrap_or(0))
            .sum::<usize>()
            > 0
    );
}

/// Feature: Common canonical primitive families in the reviewed deck are executable.
#[test]
fn reviewed_four_color_deck_has_no_unsupported_engine_rules() {
    let cards = mainboard();
    let engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 7,
            starting_player: 0,
            players: vec![
                PlayerDeck {
                    id: "player-one".to_string(),
                    name: "Coverage player 1".to_string(),
                    starting_life: 20,
                    cards: cards.clone(),
                },
                PlayerDeck {
                    id: "player-two".to_string(),
                    name: "Coverage player 2".to_string(),
                    starting_life: 20,
                    cards,
                },
            ],
        },
        20260725,
    )
    .expect("reviewed deck creates an engine state");

    assert_eq!(
        engine.state().unsupported_rules,
        Vec::new(),
        "remaining unsupported rules: {:#?}",
        engine.state().unsupported_rules,
    );
}
