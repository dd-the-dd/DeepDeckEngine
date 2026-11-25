use deepdeck_oracle::oracle::{
    OracleCardFace, OracleCardParseRequest, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const BACK_NAME: &str = "Test Transformed Face";
const BACK_TYPE: &str = "Artifact Creature — Phyrexian";

/// A transform layout has exactly two printed parts; the action under test is
/// on the front face and the back face has no rules text.
fn parse(
    card_name: &str,
    type_line: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    let face = |id: &str, name: &str, type_line: &str, oracle_text: &str| OracleCardFace {
        id: id.to_string(),
        name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: oracle_text.to_string(),
        power: None,
        toughness: None,
        loyalty: None,
    };
    parse_oracle_card(OracleCardParseRequest {
        card_name: format!("{card_name} // {BACK_NAME}"),
        type_line: format!("{type_line} // {BACK_TYPE}"),
        mana_cost: None,
        oracle_text: None,
        layout: Some("transform".to_string()),
        faces: vec![
            face("front", card_name, type_line, oracle_text),
            face("back", BACK_NAME, BACK_TYPE, ""),
        ],
    })
}

#[test]
fn incubator_style_transform_action_is_native_and_atomic() {
    let parsed = parse(
        "Incubator Token",
        "Token Artifact — Incubator",
        "{2}: Transform this token.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "payMana", "manaCost": "{2}" }],
            "declaration": { "kind": "castingDeclaration", "decisions": [] },
            "effects": [{
                "kind": "transformPermanent",
                "object": { "kind": "self" }
            }]
        }))
    );
}

#[test]
fn current_card_name_uses_the_same_source_reference_automaton() {
    let parsed = parse(
        "Test Shifter",
        "Creature — Shapeshifter",
        "{3}: Transform Test Shifter.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["object"],
        json!({ "kind": "self" })
    );
}

#[test]
fn a_target_operand_is_transformed_by_the_shared_operand_reader() {
    // `Transform target permanent.` now reads its operand through the shared
    // permanent operand grammar: one declared target, transformed on
    // resolution (CR 701.27a).
    let parsed = parse(
        "Test Shifter",
        "Creature — Shapeshifter",
        "{3}: Transform target permanent.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    let rule = parsed.abilities[0].rule.as_ref().expect("a rule");
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseTargets");
    assert_eq!(rule["effects"][0]["kind"], "transformPermanent");
    assert_eq!(rule["effects"][0]["object"]["kind"], "chosenTarget");
}
