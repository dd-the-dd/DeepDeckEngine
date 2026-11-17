use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parse(
    name: &str,
    type_line: &str,
    mana_cost: &str,
    text: &str,
) -> deepdeck_oracle_engine_contracts::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn unmask_compiles_to_alternative_cost_and_shared_hand_operations() {
    let result = parse(
        "Unmask",
        "Sorcery",
        "{3}{B}",
        "You may exile a black card from your hand rather than pay this spell's mana cost.\n\
         Target player reveals their hand. You choose a nonland card from it. That player discards that card.",
    );

    assert_eq!(
        result.status,
        "canonical",
        "{:?}",
        result
            .abilities
            .iter()
            .flat_map(|ability| ability.diagnostics.iter())
            .collect::<Vec<_>>()
    );
    assert_eq!(result.abilities.len(), 2);
    assert_eq!(
        result.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
        "alternativeCost"
    );
    let spell = result.abilities[1].rule.as_ref().unwrap();
    assert_eq!(
        spell["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["revealHand", "chooseCards", "discardCards"]
    );
    assert!(
        result
            .abilities
            .iter()
            .filter_map(|ability| ability.rule.as_ref())
            .all(rule_is_executable)
    );
}
