use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse_spell(card_name: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("ability-loss-destroy-parser-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

fn spell_rule(parsed: &OracleCardParseResult) -> &Value {
    parsed
        .abilities
        .iter()
        .find_map(|ability| ability.rule.as_ref())
        .expect("the spell should produce primitive IR")
}

#[test]
fn day_of_black_sun_binds_one_set_then_composes_ability_loss_and_destruction() {
    let parsed = parse_spell(
        "Day of Black Sun",
        "{X}{B}{B}",
        "Each creature with mana value X or less loses all abilities until end of turn. Destroy those creatures.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = spell_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"],
        json!([{
            "id": "xValue",
            "kind": "chooseNumber",
            "minimum": 0,
        }])
    );
    let effects = rule["effects"].as_array().expect("ordered effects");
    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0]["kind"], "bind");
    assert_eq!(effects[0]["id"], "affectedPermanents");
    assert_eq!(effects[0]["value"]["kind"], "eachPermanent");
    assert_eq!(
        effects[0]["value"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Creature" },
                {
                    "kind": "compare",
                    "operator": "<=",
                    "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                    "right": { "kind": "decisionResult", "decisionId": "xValue" },
                },
            ],
        })
    );
    assert_eq!(effects[1]["kind"], "forEachObject");
    assert_eq!(
        effects[1]["objects"],
        json!({ "kind": "boundObjects", "binding": "affectedPermanents" })
    );
    assert_eq!(effects[1]["effects"][0]["kind"], "installLoseAllAbilities");
    assert_eq!(
        effects[2],
        json!({
            "kind": "destroyPermanent",
            "permanent": { "kind": "boundObjects", "binding": "affectedPermanents" },
        })
    );
    let serialized = rule.to_string();
    assert!(!serialized.contains("dayOfBlackSun"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn literal_threshold_and_another_permanent_type_reuse_the_same_sequence() {
    let parsed = parse_spell(
        "Test Eclipse",
        "{2}{B}",
        "Each artifact with mana value three or less loses all abilities until end of turn. Destroy those artifacts.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = spell_rule(&parsed);
    assert!(rule.get("declaration").is_none());
    assert_eq!(
        rule["effects"][0]["value"]["where"]["operands"][1]["right"],
        json!({ "kind": "integer", "value": 3 })
    );
    assert_eq!(
        rule["effects"][0]["value"]["where"]["operands"][0],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
}

#[test]
fn mismatched_those_reference_is_rejected() {
    let parsed = parse_spell(
        "Test Eclipse",
        "{X}{B}",
        "Each creature with mana value X or less loses all abilities until end of turn. Destroy those artifacts.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.rule.is_none())
    );
}

#[test]
fn unsupported_duration_is_not_approximated() {
    let parsed = parse_spell(
        "Test Eclipse",
        "{X}{B}",
        "Each creature with mana value X or less loses all abilities for as long as Test Eclipse remains on the stack. Destroy those creatures.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.rule.is_none())
    );
}
