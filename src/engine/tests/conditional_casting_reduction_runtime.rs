use super::*;

// Real Oracle text: parser -> IR -> engine proof that a conditional own
// reduction is evaluated while the total casting cost is determined.
const ORACLE_TEXT: &str = "This spell costs {1} less to cast if you control a Kithkin.";

fn spell_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Conditional Reducer".to_string(),
        type_line: "Creature — Kithkin Wizard".to_string(),
        mana_cost: Some("{3}{W}".to_string()),
        oracle_text: Some(ORACLE_TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let mut definition = test_definition("conditional-reducer", "Creature — Kithkin Wizard");
    definition.mana_cost = "{3}{W}".to_string();
    definition.rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical reduction"))
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn reduction_applies_only_while_the_controller_controls_a_matching_permanent() {
    let definition = spell_definition();
    for (battlefield_type, expected) in [
        ("Creature — Kithkin", "{2}{W}"),
        ("Creature — Elf", "{3}{W}"),
    ] {
        let mut engine = test_engine(2);
        let permanent = test_instance(
            "ally",
            test_definition("ally", battlefield_type),
            "player-0",
        );
        engine.state.players[0].battlefield = vec![permanent];
        let costs = engine
            .checked_casting_mana_costs(&definition, "player-0", &BTreeMap::new(), None)
            .expect("total cost is determined");
        assert_eq!(costs.concat(), expected, "{battlefield_type}");
    }
}
