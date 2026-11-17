use super::*;

#[test]
fn cascade_definition_uses_only_existing_primitives_and_captured_threshold() {
    let rule = cascade_trigger_rule(5);
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0]["kind"], "exileFromTopUntil");
    assert_eq!(effects[1]["kind"], "castOneCard");
    assert_eq!(effects[2]["kind"], "moveCards");
    assert!(effects.iter().all(super::super::effect_supported));
    assert_eq!(
        effects[0]["stopWhere"]["operands"][1],
        effects[1]["spellWhere"]
    );
    assert_eq!(effects[1]["spellWhere"]["right"]["value"], 5);
    assert_eq!(effects[1]["normalCostsApply"], false);
    assert_eq!(effects[1]["optional"], true);
    assert_eq!(effects[2]["from"]["kind"], "exile");
    assert_eq!(effects[2]["order"]["kind"], "random");
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    assert!(!rule.to_string().contains("cascadeCastLowerManaValue"));
}

#[test]
fn cast_one_card_rejects_unsupported_resulting_spell_predicates() {
    let rule = cascade_trigger_rule(3);
    let valid = rule["effects"][1].clone();
    assert!(super::super::effect_supported(&valid));
    for predicate in [
        json!({ "kind": "unimplementedSpellCondition" }),
        json!({
            "kind": "compare", "operator": "<",
            "left": { "kind": "manaValueOf", "object": { "kind": "self" } },
            "right": { "kind": "integer", "value": 3 },
        }),
    ] {
        let mut invalid = valid.clone();
        invalid["spellWhere"] = predicate;
        assert!(!super::super::effect_supported(&invalid));
    }
}
