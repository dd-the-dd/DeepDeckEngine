use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Exile target nonland permanent. You lose 3 life.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Removal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{W}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn anguished_unmaking_is_a_native_sequence_of_two_primitives() {
    let parsed = parse(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetPermanent",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "where": {
                            "kind": "not",
                            "operand": { "kind": "cardTypeContains", "value": "Land" },
                        },
                    },
                }],
            },
            "effects": [
                {
                    "kind": "exilePermanent",
                    "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
                },
                {
                    "kind": "loseLife",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "amount": { "kind": "integer", "value": 3 },
                },
            ],
        }))
    );
    let serialized = serde_json::to_string(&parsed).expect("parse result serializes");
    assert!(!serialized.contains("resolveTriggeredInstruction"));
    assert!(!serialized.contains("exileAndLoseLife"));
}

#[test]
fn action_negative_type_life_direction_and_amount_are_reusable() {
    let parsed = parse("Destroy target nonartifact permanent. You gain two life.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("sibling sequence emits a rule");
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][1]["kind"], "gainLife");
    assert_eq!(rule["effects"][1]["amount"]["value"], 2);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "not",
            "operand": { "kind": "cardTypeContains", "value": "Artifact" },
        })
    );
}

#[test]
fn unknown_life_amount_is_a_committed_numeric_failure() {
    let parsed = parse("Exile target nonland permanent. You lose much life.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert!(ability.entities.iter().any(|entity| {
        entity.raw.eq_ignore_ascii_case("much") && entity.token_type == "unknown"
    }));
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("numeric.atom.start")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("much"));
}

#[test]
fn negative_card_type_without_a_permanent_domain_is_not_approximated() {
    let parsed = parse("Exile target nonland card. You lose 3 life.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("contextual-effect-program")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.zone-change.target.validation")
    );
}
