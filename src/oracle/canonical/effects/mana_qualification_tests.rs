use crate::card_catalog::named_token_printing;
use crate::oracle::canonical::compile_named_token_native_rule;

#[test]
fn catalog_ability_roles_follow_typed_execution_and_target_facts() {
    for (name, expected_kind) in [
        ("treasure", "manaAbility"),
        ("blood", "activatedAbility"),
        ("clue", "activatedAbility"),
        ("food", "activatedAbility"),
        ("map", "activatedAbility"),
    ] {
        let printing = named_token_printing(name).unwrap().unwrap();
        let rule =
            compile_named_token_native_rule(printing.oracle_text.as_deref().unwrap()).unwrap();
        assert_eq!(rule["kind"], expected_kind, "{name}");
        if name == "blood" {
            // A choice used to pay a cost is not an effect target.
            assert_eq!(rule["declaration"]["decisions"][0]["id"], "discardCost1");
            assert_eq!(rule["costs"][2]["kind"], "discardCard");
        }
        if name == "map" {
            assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPermanent");
            assert_eq!(rule["activationCondition"]["kind"], "sorceryTiming");
        }
    }
}

#[test]
fn unnamed_mana_output_quantity_and_timing_are_preserved() {
    let rule = compile_named_token_native_rule(
        "Sacrifice this token: Add two mana of any color. Activate only as a sorcery.",
    )
    .unwrap();
    assert_eq!(rule["kind"], "manaAbility");
    assert_eq!(
        rule["effects"][0]["mana"]["amount"],
        serde_json::json!({"kind":"integer","value":2})
    );
    assert_eq!(rule["costs"][0]["kind"], "sacrificePermanent");
    assert_eq!(rule["activationCondition"]["kind"], "sorceryTiming");
    assert!(rule.get("declaration").is_none());
    assert!(
        compile_named_token_native_rule(
            "Sacrifice this token: Add two mana of any color. Draw a card."
        )
        .is_none()
    );
}
