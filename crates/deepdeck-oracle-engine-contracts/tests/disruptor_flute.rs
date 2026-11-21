use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn chosen_name_choice_tax_and_activation_prohibition_are_executable_primitives() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Disruptor Flute".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "Flash\nAs this artifact enters, choose a card name.\nSpells with the chosen name cost {3} more to cast.\nActivated abilities of sources with the chosen name can't be activated unless they're mana abilities."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 4);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability.rule.as_ref().is_some_and(rule_is_executable)
    }));

    let choice = parsed.abilities[1].rule.as_ref().unwrap();
    let tax = parsed.abilities[2].rule.as_ref().unwrap();
    let prohibition = parsed.abilities[3].rule.as_ref().unwrap();
    assert_eq!(choice["decisions"][0]["id"], "chosenCardName");
    assert_eq!(
        tax["modifiers"][0]["where"]["value"]["decisionId"],
        "chosenCardName"
    );
    assert_eq!(
        prohibition["modifiers"][0]["abilities"]["sourceWhere"]["value"]["decisionId"],
        "chosenCardName"
    );
    assert_eq!(
        prohibition["modifiers"][0]["abilities"]["where"]["operand"]["kind"],
        "isManaAbility"
    );
}
