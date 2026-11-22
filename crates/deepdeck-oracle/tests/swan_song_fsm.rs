use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Counter target enchantment, instant, or sorcery spell. Its controller creates a 2/2 blue Bird creature token with flying.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Counter Token Test".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn swan_song_is_native_and_lowers_to_three_independent_primitives() {
    let parsed = parse(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetSpell",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "spells",
                        "where": {
                            "kind": "or",
                            "operands": [
                                { "kind": "cardTypeContains", "value": "Enchantment" },
                                { "kind": "cardTypeContains", "value": "Instant" },
                                { "kind": "cardTypeContains", "value": "Sorcery" },
                            ],
                        },
                    },
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "counteredSpellController",
                    "value": {
                        "kind": "controllerOf",
                        "object": { "kind": "chosenTarget", "id": "targetSpell" },
                    },
                },
                {
                    "kind": "counterSpell",
                    "spell": { "kind": "chosenTarget", "id": "targetSpell" },
                },
                {
                    "kind": "createTokens",
                    "controller": {
                        "kind": "boundValue",
                        "id": "counteredSpellController",
                    },
                    "quantity": { "kind": "integer", "value": 1 },
                    "token": {
                        "colors": ["Blue"],
                        "types": ["Creature"],
                        "subtypes": ["Bird"],
                        "power": 2,
                        "toughness": 2,
                        "abilities": [{ "kind": "flying" }],
                    },
                },
            ],
        }))
    );
}

#[test]
fn descriptive_token_sibling_reuses_selector_characteristic_and_keyword_automata() {
    let parsed = parse(
        "Counter target artifact spell. Its controller creates a 3/3 red Goblin creature token with haste.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("native rule");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(rule["effects"][2]["token"]["power"], 3);
    assert_eq!(rule["effects"][2]["token"]["toughness"], 3);
    assert_eq!(rule["effects"][2]["token"]["colors"], json!(["Red"]));
    assert_eq!(rule["effects"][2]["token"]["subtypes"], json!(["Goblin"]));
    assert_eq!(
        rule["effects"][2]["token"]["abilities"],
        json!([{ "kind": "haste" }])
    );
}

/// The countered spell's owner as the performer of the ordinary token
/// creation (CR 608.2c): no longer a near miss of the descriptive-token
/// family, read by the shared performer slot instead.
#[test]
fn the_countered_spells_owner_performs_the_token_creation() {
    let parsed = parse(
        "Counter target artifact spell. Its owner creates a 3/3 red Goblin creature token with haste.",
    );
    let ability = &parsed.abilities[0];
    assert_eq!(ability.status, "canonical");
    let effects = &ability.rule.as_ref().expect("a rule")["effects"];
    assert!(
        effects
            .as_array()
            .expect("effects")
            .iter()
            .any(|effect| effect["performer"]["kind"] == "ownerOf"),
        "{effects:#}"
    );
}
