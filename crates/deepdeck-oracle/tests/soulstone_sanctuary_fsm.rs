use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "{T}: Add {C}.\n{4}: This land becomes a 3/3 creature with vigilance and all creature types. It's still a land.";

fn parse(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn soulstone_sanctuary_is_fully_native_and_composes_atomic_primitives() {
    let parsed = parse("Soulstone Sanctuary", "Land", EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "payMana", "manaCost": "{4}" }],
            "declaration": { "kind": "castingDeclaration", "decisions": [] },
            "effects": [
                {
                    "kind": "addCardType",
                    "object": { "kind": "self" },
                    "cardType": "Creature",
                    "retainExistingTypes": true,
                    "duration": { "kind": "permanent" }
                },
                {
                    "kind": "modifySubtypeSet",
                    "object": { "kind": "self" },
                    "subtypeClass": "creature",
                    "mode": "all",
                    "duration": { "kind": "permanent" }
                },
                {
                    "kind": "grantKeyword",
                    "object": { "kind": "self" },
                    "keyword": "vigilance",
                    "duration": { "kind": "permanent" }
                },
                {
                    "kind": "setBasePowerToughness",
                    "object": { "kind": "self" },
                    "power": { "kind": "integer", "value": 3 },
                    "toughness": { "kind": "integer", "value": 3 },
                    "duration": { "kind": "permanent" }
                }
            ]
        }))
    );
}

#[test]
fn sibling_source_type_and_keyword_reuse_the_same_children() {
    let parsed = parse(
        "Test Reliquary",
        "Artifact",
        "{2}: This artifact becomes a 2/2 creature with flying and all creature types. It's still an artifact.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let effects = ability.rule.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap();
    assert_eq!(effects[1]["kind"], "modifySubtypeSet");
    assert_eq!(effects[2]["keyword"], "flying");
    assert_eq!(
        effects[3]["power"],
        json!({ "kind": "integer", "value": 2 })
    );
}

#[test]
fn near_miss_reports_the_precise_all_creature_types_state() {
    let parsed = parse(
        "Test Sanctuary",
        "Land",
        "{4}: This land becomes a 3/3 creature with vigilance and all artifact types. It's still a land.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    // `all artifact types` is not a `with` item of the frame; the
    // coordinated remainder stays an explicit failure.
    assert!(!ability.diagnostics.is_empty());
}

#[test]
fn non_creature_source_state_change_is_not_stolen() {
    let parsed = parse(
        "Test Sanctuary",
        "Land",
        "{4}: This land becomes tapped. It's still a land.",
    );

    // The shared program rejects it; the animation family never claims it.
    let ability = &parsed.abilities[0];
    assert_eq!(parsed.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("becomes"));
}
