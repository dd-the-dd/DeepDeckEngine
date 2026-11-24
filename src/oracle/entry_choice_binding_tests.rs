use serde_json::{Value, json};

use super::bind_entry_choices;
use crate::oracle::contextual_binding::{
    RESOLUTION_COLOR_DECISION, RESOLUTION_CREATURE_TYPE_DECISION,
};
use crate::oracle::model::{AbilitySource, ParsedOracleAbility, RecognizerProvenance};

fn ability(face_id: Option<&str>, rule: Value) -> ParsedOracleAbility {
    ParsedOracleAbility {
        source: AbilitySource {
            face_id: face_id.map(ToOwned::to_owned),
            text: "probe".to_string(),
            line_start: 0,
            line_end: 0,
        },
        ability_type: rule["kind"].as_str().unwrap_or_default().to_string(),
        status: "canonical".to_string(),
        recognizer: RecognizerProvenance::Native,
        rule: Some(rule),
        entities: Vec::new(),
        iterations: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn entry_choice(kind: &str, id: &str) -> Value {
    json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
        "decisions": [{ "id": id, "kind": kind }],
        "replacement": [{ "kind": "storeDecision", "decisionId": id }],
    })
}

fn anthem(reference: &str, resolution_id: &str) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "modifiers": [{
            "kind": "modifyPowerToughness",
            "objects": {
                "kind": "permanents",
                "where": { "kind": reference, "decisionId": resolution_id },
            },
        }],
    })
}

fn bound_id(ability: &ParsedOracleAbility) -> &str {
    ability.rule.as_ref().expect("rule")["modifiers"][0]["objects"]["where"]["decisionId"]
        .as_str()
        .expect("decision id")
}

#[test]
fn a_static_reference_binds_to_the_face_entry_choice() {
    let mut abilities = vec![
        ability(None, entry_choice("chooseColor", "chosenColor")),
        ability(None, anthem("chosenColor", RESOLUTION_COLOR_DECISION)),
    ];
    bind_entry_choices(&mut abilities);
    assert_eq!(bound_id(&abilities[1]), "chosenColor");
}

#[test]
fn the_chosen_type_reads_a_creature_or_basic_land_type_choice() {
    for (kind, id) in [
        ("chooseCreatureType", "chosenCreatureType"),
        ("chooseBasicLandType", "chosenBasicLandType"),
    ] {
        let mut abilities = vec![
            ability(None, entry_choice(kind, id)),
            ability(
                None,
                anthem("chosenCreatureType", RESOLUTION_CREATURE_TYPE_DECISION),
            ),
        ];
        bind_entry_choices(&mut abilities);
        assert_eq!(bound_id(&abilities[1]), id, "{kind}");
    }
}

#[test]
fn another_face_or_a_resolution_producer_keeps_the_resolution_reference() {
    let mut abilities = vec![
        ability(Some("front"), entry_choice("chooseColor", "chosenColor")),
        ability(
            Some("back"),
            anthem("chosenColor", RESOLUTION_COLOR_DECISION),
        ),
    ];
    bind_entry_choices(&mut abilities);
    assert_eq!(bound_id(&abilities[1]), RESOLUTION_COLOR_DECISION);

    let mut own = anthem("chosenColor", RESOLUTION_COLOR_DECISION);
    own["effects"] = json!([{ "kind": "chooseColor", "id": RESOLUTION_COLOR_DECISION }]);
    let mut abilities = vec![
        ability(None, entry_choice("chooseColor", "chosenColor")),
        ability(None, own),
    ];
    bind_entry_choices(&mut abilities);
    assert_eq!(bound_id(&abilities[1]), RESOLUTION_COLOR_DECISION);
}

#[test]
fn mana_of_the_chosen_color_without_an_entry_producer_is_rejected() {
    let mana = json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "addMana",
            "mana": { "kind": "storedColor", "decisionId": RESOLUTION_COLOR_DECISION, "amount": 1 },
        }],
    });
    let mut bound = vec![
        ability(None, entry_choice("chooseColor", "chosenColor")),
        ability(None, mana.clone()),
    ];
    bind_entry_choices(&mut bound);
    assert_eq!(bound[1].status, "canonical");
    assert_eq!(
        bound[1].rule.as_ref().expect("rule")["effects"][0]["mana"]["decisionId"],
        "chosenColor"
    );

    let mut unbound = vec![ability(None, mana)];
    bind_entry_choices(&mut unbound);
    assert_eq!(unbound[0].status, "unsupported");
    assert_eq!(
        unbound[0].diagnostics[0].state.as_deref(),
        Some("binding.chosen-color.entry-source")
    );
}
