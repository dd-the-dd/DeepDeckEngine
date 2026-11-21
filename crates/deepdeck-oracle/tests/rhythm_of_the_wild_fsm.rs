use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Creature spells you control can't be countered.\nNontoken creatures you control have riot. (They enter with your choice of a +1/+1 counter or haste.)";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Static Grammar Fixture".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{R}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn exact_card_is_all_native_with_filtered_atomic_modifiers() {
    let parsed = parse(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));

    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "cantBeCountered",
                "objects": {
                    "kind": "spells",
                    "controller": controller(),
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            }],
        }))
    );

    assert_eq!(
        parsed.abilities[1].rule,
        Some(json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "grantKeyword",
                "objects": {
                    "kind": "permanents",
                    "controller": controller(),
                    "where": {
                        "kind": "and",
                        "operands": [
                            { "kind": "not", "operand": { "kind": "isToken" } },
                            { "kind": "cardTypeContains", "value": "Creature" },
                        ],
                    },
                },
                "keyword": "riot",
            }],
        }))
    );
    assert!(
        !parsed.abilities[1]
            .rule
            .as_ref()
            .unwrap()
            .to_string()
            .contains("They enter")
    );
    let riot_rule = parsed.abilities[1].rule.as_ref().unwrap().to_string();
    assert!(!riot_rule.contains("grantRiot"));
    assert!(!riot_rule.contains("applyRiotChoice"));
}

#[test]
fn artifact_spell_and_permanent_siblings_reuse_the_same_children() {
    let parsed = parse(
        "Artifact spells you control cannot be countered.\nNontoken artifacts you control have riot.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["modifiers"][0]["objects"]["where"]["value"],
        "Artifact"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["objects"]["where"]["operands"]
            [1]["value"],
        "Artifact"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["keyword"],
        "riot"
    );
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_native_state() {
    for (oracle_text, automaton, state) in [
        (
            "Creature spells you control can't be countered twice.",
            "counter-prohibition",
            "counter-prohibition.terminator",
        ),
        (
            "Nontoken creatures you control have riot twice.",
            "static-riot-grant",
            "keyword.riot.terminator",
        ),
        (
            "Nontoken creatures you control have riot. (They enter with your choice of a +2/+2 counter or haste.)",
            "static-riot-grant",
            "counter-name.complete",
        ),
        (
            "Nontoken creatures you control have riot. (They enter with your choice of a +1/+1 counter or flying.)",
            "static-riot-grant",
            "keyword.riot.reminder.alternative",
        ),
    ] {
        let parsed = parse(oracle_text);
        assert_eq!(parsed.status, "unsupported", "{oracle_text}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
    }
}

#[test]
fn different_static_keyword_family_is_not_stolen() {
    // A parameterless keyword grant belongs to the shared static-continuous
    // parent, never to the riot family.
    let parsed = parse("Nontoken creatures you control have flying.");
    assert_eq!(parsed.status, "canonical");
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "grantKeyword",
                "keyword": "flying",
                "objects": {
                    "kind": "permanents",
                    "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "where": {
                        "kind": "and",
                        "operands": [
                            { "kind": "not", "operand": { "kind": "isToken" } },
                            { "kind": "cardTypeContains", "value": "Creature" },
                        ],
                    },
                },
            }],
        }))
    );
    assert!(
        !parsed.abilities[0]
            .rule
            .as_ref()
            .unwrap()
            .to_string()
            .contains("riot")
    );
}
