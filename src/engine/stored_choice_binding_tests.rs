use super::*;

fn definition_with(rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: "probe".to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{2}".to_string(),
        name: "probe".to_string(),
        power: None,
        rules,
        toughness: None,
        type_line: "Artifact".to_string(),
    }
}

#[test]
fn each_reference_kind_becomes_its_characteristic_predicate() {
    for (kind, chosen, expected) in [
        (
            "chosenColor",
            "R",
            json!({ "kind": "colorContains", "value": "red" }),
        ),
        (
            "chosenColor",
            "Blue",
            json!({ "kind": "colorContains", "value": "blue" }),
        ),
        (
            "chosenCreatureType",
            "Elf",
            json!({ "kind": "subtypeContains", "value": "Elf" }),
        ),
        (
            "chosenCardType",
            "Artifact",
            json!({ "kind": "cardTypeContains", "value": "Artifact" }),
        ),
    ] {
        let mut definition = definition_with(vec![json!({
            "kind": "staticAbility",
            "modifiers": [{ "where": { "kind": kind, "decisionId": "stored" } }],
        })]);
        bind_stored_choice(&mut definition, "stored", chosen);
        assert_eq!(
            definition.rules[0]["modifiers"][0]["where"], expected,
            "{kind}"
        );
    }
}

#[test]
fn producer_other_decisions_and_choice_slots_are_untouched() {
    let producer = json!({
        "kind": "replacementEffect",
        "replacement": [{ "where": { "kind": "chosenColor", "decisionId": "stored" } }],
    });
    let subtype_gain = json!({
        "kind": "staticAbility",
        "modifiers": [{
            "kind": "addChosenSubtype",
            "choice": { "kind": "chosenCreatureType", "decisionId": "stored" },
        }],
    });
    let other = json!({
        "kind": "staticAbility",
        "modifiers": [{ "where": { "kind": "chosenColor", "decisionId": "another" } }],
    });
    let mut definition =
        definition_with(vec![producer.clone(), subtype_gain.clone(), other.clone()]);
    bind_stored_choice(&mut definition, "stored", "G");
    assert_eq!(definition.rules, vec![producer, subtype_gain, other]);
}

#[test]
fn an_unknown_color_value_is_not_guessed() {
    let rule = json!({
        "kind": "staticAbility",
        "modifiers": [{ "where": { "kind": "chosenColor", "decisionId": "stored" } }],
    });
    let mut definition = definition_with(vec![rule.clone()]);
    bind_stored_choice(&mut definition, "stored", "purple");
    assert_eq!(definition.rules, vec![rule]);
}
