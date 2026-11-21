use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

fn parse_spell(text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Library Spell".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn top_cards_partition_is_native_and_primitive() {
    for (text, expected_effects) in [
        (
            "Look at the top five cards of your library. Put two of them into your hand and the rest on the bottom of your library in any order.",
            5,
        ),
        (
            "Look at the top three cards of your library. Put one of them into your hand and the rest on the bottom of your library in a random order.",
            4,
        ),
    ] {
        let parsed = parse_spell(text);
        assert_eq!(parsed.status, "canonical");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
            .as_array()
            .unwrap();
        assert_eq!(effects.len(), expected_effects);
        assert_eq!(effects[0]["kind"], "lookAtTopCards");
        assert_eq!(effects[1]["kind"], "chooseCards");
        assert_eq!(effects[2]["kind"], "moveCards");
        assert!(
            !parsed.abilities[0]
                .rule
                .as_ref()
                .unwrap()
                .to_string()
                .contains("resolveSpellInstruction")
        );
    }
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_machine_state() {
    for (text, state, found) in [
        (
            "Look at the top some cards of your library. Put two of them into your hand and the rest on the bottom of your library in any order.",
            "numeric.atom.start",
            "some",
        ),
        (
            "Look at the top five cards of your library. Put two of it into your hand and the rest on the bottom of your library in any order.",
            "reference.collection.selected-from-binding",
            "it",
        ),
        (
            "Look at the top five cards of your library. Put two of them into your sideboard and the rest on the bottom of your library in any order.",
            "zone.possessive.kind",
            "sideboard",
        ),
    ] {
        let parsed = parse_spell(text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.diagnostics[0].state.as_deref(), Some(state));
        assert_eq!(ability.diagnostics[0].found.as_deref(), Some(found));
    }
    // The chosen subset's destination is the shared owned-zone child: a
    // graveyard destination composes without a new partition recipe.
    let parsed = parse_spell(
        "Look at the top five cards of your library. Put two of them into your graveyard and the rest on the bottom of your library in any order.",
    );
    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0].rule.as_ref().expect("partition rule");
    let you = serde_json::json!({ "kind": "controllerOf", "object": { "kind": "self" } });
    assert_eq!(rule["effects"][1]["kind"], "chooseCards");
    assert_eq!(
        rule["effects"][1]["count"]["operands"][0],
        serde_json::json!({ "kind": "integer", "value": 2 })
    );
    assert_eq!(
        rule["effects"][2],
        serde_json::json!({
            "kind": "moveCards",
            "cards": { "kind": "decisionResult", "decisionId": "cardsForHand" },
            "to": { "kind": "graveyard", "player": you.clone() },
        })
    );
    assert_eq!(
        rule["effects"][4]["to"],
        serde_json::json!({ "kind": "library", "player": you, "position": "bottom" })
    );
}
