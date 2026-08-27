use mtg_engine::model::{PlayableCardInput, compile_named_token_definition, compile_playable_card};
use mtg_engine::oracle::{OracleCardFace, OracleCardParseRequest, parse_oracle_card};
use std::path::Path;

fn abrade_request() -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Abrade".to_string(),
        faces: Vec::new(),
        layout: None,
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(
            "Choose one —\n• Abrade deals 3 damage to target creature.\n• Destroy target artifact."
                .to_string(),
        ),
        type_line: "Instant".to_string(),
    }
}

fn emeritus_request() -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Emeritus of Ideation // Ancestral Recall".to_string(),
        faces: vec![
            OracleCardFace {
                id: "emeritus-of-ideation".to_string(),
                mana_cost: Some("{3}{U}{U}".to_string()),
                name: "Emeritus of Ideation".to_string(),
                oracle_text: "Flying, ward {2}\nThis creature enters prepared.\nWhenever this creature attacks, you may exile eight cards from your graveyard. If you do, this creature becomes prepared.".to_string(),
                power: Some("5".to_string()),
                toughness: Some("5".to_string()),
                type_line: "Creature - Human Wizard".to_string(),
            },
            OracleCardFace {
                id: "ancestral-recall".to_string(),
                mana_cost: Some("{U}".to_string()),
                name: "Ancestral Recall".to_string(),
                oracle_text: "Target player draws three cards.".to_string(),
                power: None,
                toughness: None,
                type_line: "Instant".to_string(),
            },
        ],
        layout: Some("prepare".to_string()),
        mana_cost: Some("{3}{U}{U} // {U}".to_string()),
        oracle_text: None,
        type_line: "Creature - Human Wizard // Instant".to_string(),
    }
}

fn sephiroth_request() -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Sephiroth, Fabled SOLDIER // Sephiroth, One-Winged Angel".to_string(),
        faces: vec![
            OracleCardFace {
                id: "sephiroth-front".to_string(),
                mana_cost: Some("{2}{B}".to_string()),
                name: "Sephiroth, Fabled SOLDIER".to_string(),
                oracle_text: "Whenever Sephiroth enters or attacks, you may sacrifice another creature. If you do, draw a card.\nWhenever another creature dies, target opponent loses 1 life and you gain 1 life. If this is the fourth time this ability has resolved this turn, transform Sephiroth.".to_string(),
                power: Some("3".to_string()),
                toughness: Some("3".to_string()),
                type_line: "Legendary Creature - Human Avatar Soldier".to_string(),
            },
            OracleCardFace {
                id: "sephiroth-back".to_string(),
                mana_cost: Some(String::new()),
                name: "Sephiroth, One-Winged Angel".to_string(),
                oracle_text: "Flying\nSuper Nova - As this creature transforms into Sephiroth, One-Winged Angel, you get an emblem with \"Whenever a creature dies, target opponent loses 1 life and you gain 1 life.\"\nWhenever Sephiroth attacks, you may sacrifice any number of other creatures. If you do, draw that many cards.".to_string(),
                power: Some("5".to_string()),
                toughness: Some("5".to_string()),
                type_line: "Legendary Creature - Angel Nightmare Avatar".to_string(),
            },
        ],
        layout: Some("transform".to_string()),
        mana_cost: None,
        oracle_text: None,
        type_line: "Legendary Creature - Human Avatar Soldier // Legendary Creature - Angel Nightmare Avatar".to_string(),
    }
}

fn room_request() -> OracleCardParseRequest {
    let reminder = "(You may cast either half. That door unlocks on the battlefield. As a sorcery, you may pay the mana cost of a locked door to unlock it.)";
    OracleCardParseRequest {
        card_name: "Defiled Crypt // Cadaver Lab".to_string(),
        faces: vec![
            OracleCardFace {
                id: "defiled-crypt".to_string(),
                mana_cost: Some("{3}{B}".to_string()),
                name: "Defiled Crypt".to_string(),
                oracle_text: format!(
                    "Whenever one or more cards leave your graveyard, create a 2/2 black Horror enchantment creature token. This ability triggers only once each turn.\n{reminder}"
                ),
                power: None,
                toughness: None,
                type_line: "Enchantment - Room".to_string(),
            },
            OracleCardFace {
                id: "cadaver-lab".to_string(),
                mana_cost: Some("{B}".to_string()),
                name: "Cadaver Lab".to_string(),
                oracle_text: format!(
                    "When you unlock this door, return target creature card from your graveyard to your hand.\n{reminder}"
                ),
                power: None,
                toughness: None,
                type_line: "Enchantment - Room".to_string(),
            },
        ],
        layout: Some("split".to_string()),
        mana_cost: Some("{3}{B} // {B}".to_string()),
        oracle_text: None,
        type_line: "Enchantment - Room // Enchantment - Room".to_string(),
    }
}

fn expansion_explosion_request() -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Expansion // Explosion".to_string(),
        faces: vec![
            OracleCardFace {
                id: "expansion".to_string(),
                mana_cost: Some("{U/R}{U/R}".to_string()),
                name: "Expansion".to_string(),
                oracle_text: "Copy target instant or sorcery spell with mana value 4 or less. You may choose new targets for the copy.".to_string(),
                power: None,
                toughness: None,
                type_line: "Instant".to_string(),
            },
            OracleCardFace {
                id: "explosion".to_string(),
                mana_cost: Some("{X}{U}{U}{R}{R}".to_string()),
                name: "Explosion".to_string(),
                oracle_text: "Explosion deals X damage to any target. Target player draws X cards.".to_string(),
                power: None,
                toughness: None,
                type_line: "Instant".to_string(),
            },
        ],
        layout: Some("split".to_string()),
        mana_cost: Some("{U/R}{U/R} // {X}{U}{U}{R}{R}".to_string()),
        oracle_text: None,
        type_line: "Instant // Instant".to_string(),
    }
}

/// Feature: The playable-card adapter consumes production parser rules rather than a truth fixture.
#[test]
fn playable_card_compilation_uses_production_oracle_output() {
    let request = abrade_request();
    let parsed = parse_oracle_card(request.clone());
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: None,
        id: "abrade".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: request,
        power: None,
        toughness: None,
    })
    .expect("Abrade has a playable face");

    assert_eq!(compilation.parser_status, "canonical");
    assert_eq!(compilation.engine_status, "executable");
    assert_eq!(compilation.canonical_ability_count, 1);
    assert_eq!(compilation.unsupported_ability_count, 0);
    assert_eq!(compilation.executable_rule_count, 1);
    assert_eq!(compilation.unexecutable_rule_count, 0);
    assert_eq!(
        compilation.card.rules,
        parsed
            .abilities
            .into_iter()
            .filter_map(|ability| ability.rule)
            .collect::<Vec<_>>(),
    );
}

/// Feature: The playable adapter links a prepare spell without merging it into the permanent cast.
#[test]
fn playable_card_compilation_preserves_prepare_face_locality() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: Some("emeritus-of-ideation".to_string()),
        id: "emeritus-of-ideation".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: emeritus_request(),
        power: Some("5".to_string()),
        toughness: Some("5".to_string()),
    })
    .expect("Emeritus has a playable permanent face");

    assert_eq!(compilation.parser_status, "canonical");
    assert!(
        compilation
            .card
            .rules
            .iter()
            .all(|rule| rule["kind"] != "spellAbility")
    );
    let prepare = compilation
        .card
        .rules
        .iter()
        .find(|rule| rule["kind"] == "prepareSpell")
        .expect("linked prepare spell");
    assert_eq!(prepare["spell"]["name"], "Ancestral Recall");
    assert_eq!(prepare["spell"]["rules"][0]["kind"], "spellAbility");
}

/// Feature: Transform cards retain both faces while exposing which rules belong to each face.
#[test]
fn playable_card_compilation_preserves_transform_face_locality() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: Some("sephiroth-front".to_string()),
        id: "sephiroth".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: sephiroth_request(),
        power: Some("3".to_string()),
        toughness: Some("3".to_string()),
    })
    .expect("Sephiroth has a playable front face");

    assert_eq!(compilation.parser_status, "canonical");
    assert_eq!(compilation.engine_status, "executable");
    assert!(
        compilation
            .card
            .rules
            .iter()
            .any(|rule| rule["activeFaceIndex"] == 0)
    );
    assert!(
        compilation
            .card
            .rules
            .iter()
            .any(|rule| rule["activeFaceIndex"] == 1)
    );
    let marker = compilation
        .card
        .rules
        .iter()
        .find(|rule| rule["transformFaces"].is_array())
        .expect("transform face marker");
    assert_eq!(marker["transformFaces"][0]["power"], "3");
    assert_eq!(marker["transformFaces"][1]["power"], "5");
}

/// Feature: Both Room doors remain executable when Scryfall supplies separate face documents.
#[test]
fn playable_card_compilation_preserves_both_room_doors() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: Some("defiled-crypt".to_string()),
        id: "defiled-crypt".to_string(),
        is_game_piece: false,
        is_sideboard: true,
        is_token: false,
        oracle: room_request(),
        power: None,
        toughness: None,
    })
    .expect("Room has two playable doors");

    assert_eq!(compilation.parser_status, "canonical");
    assert_eq!(compilation.engine_status, "executable");
    assert!(
        compilation
            .card
            .rules
            .iter()
            .any(|rule| rule["roomDoorIndex"] == 0)
    );
    assert!(
        compilation
            .card
            .rules
            .iter()
            .any(|rule| rule["roomDoorIndex"] == 1)
    );
}

/// Feature: A split card exposes both castable halves without merging their costs or effects.
#[test]
fn playable_card_compilation_preserves_both_split_spells() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: None,
        id: "expansion-explosion".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: expansion_explosion_request(),
        power: None,
        toughness: None,
    })
    .expect("Expansion // Explosion has two playable halves");

    assert_eq!(compilation.parser_status, "canonical");
    assert_eq!(compilation.engine_status, "executable");
    let marker = compilation
        .card
        .rules
        .iter()
        .find(|rule| rule["splitFaces"].is_array())
        .expect("split face marker");
    assert_eq!(marker["splitFaces"][0]["name"], "Expansion");
    assert_eq!(marker["splitFaces"][0]["manaCost"], "{U/R}{U/R}");
    assert_eq!(
        marker["splitFaces"][0]["rules"][0]["effects"][0]["operation"],
        "expansion"
    );
    assert_eq!(marker["splitFaces"][1]["name"], "Explosion");
    assert_eq!(marker["splitFaces"][1]["manaCost"], "{X}{U}{U}{R}{R}");
    assert_eq!(
        marker["splitFaces"][1]["rules"][0]["effects"][0]["operation"],
        "explosion"
    );
}

/// Feature: Unsupported Oracle text never leaks a partial unique rule into the playable model.
#[test]
fn playable_card_compilation_suppresses_fully_unsupported_parser_output() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: None,
        id: "unknown-card".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: OracleCardParseRequest {
            card_name: "Unknown Card".to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: Some("{1}".to_string()),
            oracle_text: Some("Perform an entirely unknown cosmic procedure.".to_string()),
            type_line: "Sorcery".to_string(),
        },
        power: None,
        toughness: None,
    })
    .expect("single-face card compiles to a core card");

    assert_eq!(compilation.parser_status, "unsupported");
    assert_eq!(compilation.engine_status, "incomplete");
    assert_eq!(compilation.canonical_ability_count, 0);
    assert_eq!(compilation.unsupported_ability_count, 1);
    assert!(compilation.card.rules.is_empty());
}

#[test]
fn playable_card_compilation_keeps_canonical_rules_from_partial_talon_parse() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: None,
        id: "talon-gates-of-madara".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: OracleCardParseRequest {
            card_name: "Talon Gates of Madara".to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: None,
            oracle_text: Some(
                "When Talon Gates of Madara enters the battlefield, up to one target creature phases out.\n{T}: Add {C}.\n{1}, {T}: Add one mana of any color.\n{4}: Put Talon Gates of Madara from your hand onto the battlefield."
                    .to_string(),
            ),
            type_line: "Land - Gate".to_string(),
        },
        power: None,
        toughness: None,
    })
    .expect("Talon compiles with partial parser coverage");

    assert_eq!(compilation.parser_status, "unsupported");
    assert_eq!(compilation.engine_status, "incomplete");
    assert_eq!(compilation.canonical_ability_count, 2);
    assert_eq!(compilation.unsupported_ability_count, 2);
    assert_eq!(compilation.card.rules.len(), 2);
    assert!(
        compilation
            .card
            .rules
            .iter()
            .all(|rule| rule["kind"] == "manaAbility")
    );
    assert_eq!(compilation.card.rules[1]["costs"][0]["manaCost"], "{1}");
}

#[test]
fn playable_card_compilation_reports_offspring_as_executable() {
    let compilation = compile_playable_card(PlayableCardInput {
        face_id: None,
        id: "agate-instigator".to_string(),
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        oracle: OracleCardParseRequest {
            card_name: "Agate Instigator".to_string(),
            faces: Vec::new(),
            layout: None,
            mana_cost: Some("{1}{R}".to_string()),
            oracle_text: Some(
                "Offspring {1}{R} (You may pay an additional {1}{R} as you cast this spell. If you do, when this creature enters, create a 1/1 token copy of it.)"
                    .to_string(),
            ),
            type_line: "Creature - Lizard Rogue".to_string(),
        },
        power: Some("1".to_string()),
        toughness: Some("3".to_string()),
    })
    .expect("Agate Instigator compiles");

    assert_eq!(compilation.parser_status, "canonical");
    assert_eq!(compilation.engine_status, "executable");
    assert_eq!(compilation.canonical_ability_count, 1);
    assert_eq!(compilation.executable_rule_count, 1);
    assert_eq!(compilation.unexecutable_rule_count, 0);
}

#[test]
fn named_tokens_compile_their_catalog_oracle_text_instead_of_using_name_overrides() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("card-catalog")
        .join("named-tokens.json");
    // This integration-test process owns its environment and initializes the
    // catalog OnceLock only after selecting its deterministic fixture.
    unsafe {
        std::env::set_var("MTG_CARD_CATALOG_PATH", fixture);
    }
    let treasure = compile_named_token_definition("Treasure")
        .expect("token catalog loads")
        .expect("Treasure token exists");
    assert!(treasure.type_line.contains("Artifact"));
    assert_eq!(treasure.rules.len(), 1);
    assert_eq!(treasure.rules[0]["kind"], "manaAbility");
    assert_eq!(treasure.rules[0]["effects"][0]["kind"], "addMana");

    let clue = compile_named_token_definition("Clue")
        .expect("token catalog loads")
        .expect("Clue token exists");
    assert_eq!(clue.rules.len(), 1);
    assert_eq!(clue.rules[0]["kind"], "activatedAbility");
    assert_eq!(clue.rules[0]["effects"][0]["kind"], "drawCards");
    assert_eq!(clue.rules[0]["effects"][0]["count"]["value"], 1);

    let blood = compile_named_token_definition("Blood")
        .expect("token catalog loads")
        .expect("Blood token exists");
    assert!(blood.type_line.contains("Blood"));
    assert_eq!(blood.rules.len(), 1);
    assert_eq!(blood.rules[0]["kind"], "activatedAbility");
    assert!(blood.rules[0]["costs"].as_array().is_some_and(|costs| {
        costs.iter().any(|cost| cost["kind"] == "discardCard")
            && costs
                .iter()
                .any(|cost| cost["kind"] == "sacrificePermanent")
    }));
    assert_eq!(blood.rules[0]["effects"][0]["kind"], "drawCards");
}
