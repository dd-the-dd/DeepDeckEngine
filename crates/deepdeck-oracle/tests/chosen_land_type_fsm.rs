use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "As this land enters, choose a basic land type. Then you may pay 2 life. If you don't, it enters tapped.\nThis land is the chosen type.";

fn parse_land(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Passage".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn entry_choice_and_static_retention_are_native_and_preserve_primitive_ir() {
    let parsed = parse_land(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "replacementEffect",
            "source": { "kind": "self" },
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": { "kind": "self" },
            },
            "decisions": [
                {
                    "id": "basicLandType",
                    "kind": "chooseBasicLandType",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "options": ["Plains", "Island", "Swamp", "Mountain", "Forest"],
                },
                {
                    "id": "optionalLifePayment",
                    "kind": "chooseWhetherToPay",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "cost": {
                        "kind": "payLife",
                        "amount": { "kind": "integer", "value": 2 },
                    },
                },
            ],
            "replacement": [
                {
                    "kind": "setBasicLandType",
                    "object": { "kind": "self" },
                    "decisionId": "basicLandType",
                },
                {
                    "kind": "conditional",
                    "condition": {
                        "kind": "not",
                        "operand": {
                            "kind": "costWasPaid",
                            "decisionId": "optionalLifePayment",
                        },
                    },
                    "then": [{
                        "kind": "setEnteringState",
                        "object": { "kind": "self" },
                        "tapped": true,
                    }],
                },
            ],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "retainChosenBasicLandType",
                "decisionId": "basicLandType",
            }],
        }))
    );
}

#[test]
fn object_kind_and_life_amount_are_generic_typed_children() {
    let parsed = parse_land(
        "As this permanent enters, choose a basic land type. Then you may pay 3 life. If you don't, it enters tapped.\nThis permanent is the chosen type.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["decisions"][1],
        json!({
            "id": "optionalLifePayment",
            "kind": "chooseWhetherToPay",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "cost": {
                "kind": "payLife",
                "amount": { "kind": "integer", "value": 3 },
            },
        })
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["decisionId"],
        "basicLandType"
    );
}

#[test]
fn claimed_near_misses_fail_at_the_responsible_binding_or_reference_state() {
    for (oracle_text, automaton, state, found) in [
        (
            "As this land enters, choose a basic land type. Then you may pay 2 life. If they don't, it enters tapped.",
            "chosen-land-type-entry",
            "condition.cost-not-paid.player",
            "they",
        ),
        (
            "This land is the named type.",
            "retain-chosen-land-type",
            "static.retain-chosen-land-type.choice-reference",
            "named",
        ),
    ] {
        let parsed = parse_land(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
