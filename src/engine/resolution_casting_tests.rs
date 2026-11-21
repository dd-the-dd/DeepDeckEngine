use super::*;

fn definition(mana_cost: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id": "generic-cast-predicate", "name": "Generic Cast Predicate",
        "typeLine": "Instant", "manaCost": mana_cost,
    }))
    .unwrap()
}

#[test]
fn resulting_spell_mana_value_uses_its_own_declaration_x() {
    let predicate = json!({
        "kind": "compare", "operator": "<",
        "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
        "right": { "kind": "integer", "value": 4 },
    });
    let candidate = definition("{X}{U}");
    assert!(resulting_spell_matches(
        &candidate,
        &BTreeMap::from([("xValue".into(), json!(0))]),
        &predicate
    ));
    assert!(!resulting_spell_matches(
        &candidate,
        &BTreeMap::from([("xValue".into(), json!(3))]),
        &predicate
    ));
    assert!(resulting_spell_matches(
        &definition(""),
        &BTreeMap::new(),
        &predicate
    ));
}

#[test]
fn resulting_spell_predicates_compose_and_keep_strict_boundary() {
    let predicate = json!({
        "kind": "and", "operands": [
            { "kind": "cardTypeContains", "value": "Instant" },
            { "kind": "not", "operand": {
                "kind": "compare", "operator": ">=",
                "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                "right": { "kind": "integer", "value": 3 },
            } },
        ],
    });
    assert!(resulting_spell_matches(
        &definition("{1}{U}"),
        &BTreeMap::new(),
        &predicate
    ));
    assert!(!resulting_spell_matches(
        &definition("{2}{U}"),
        &BTreeMap::new(),
        &predicate
    ));
}
