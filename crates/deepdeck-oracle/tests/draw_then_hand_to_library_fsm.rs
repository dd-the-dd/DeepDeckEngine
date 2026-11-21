use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse_instant(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Brainstorm".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn own_zone(kind: &str) -> Value {
    json!({
        "kind": "cardsInZone",
        "where": null,
        "zone": { "kind": kind, "player": controller() },
    })
}

/// CR 609.3: a fixed `put N cards` instruction moves as many as possible,
/// so the selection count is clamped by the cards actually in that zone.
fn clamped_count(value: i64, zone: &str) -> Value {
    json!({
        "kind": "minimum",
        "operands": [
            { "kind": "integer", "value": value },
            { "kind": "countObjects", "objects": own_zone(zone) },
        ],
    })
}

fn ordered_top_of_library_move(selection: &str, order: &str) -> [Value; 2] {
    [
        json!({
            "kind": "chooseOrder",
            "id": order,
            "player": controller(),
            "objects": { "kind": "decisionResult", "decisionId": selection },
        }),
        json!({
            "kind": "moveCards",
            "cards": { "kind": "decisionResult", "decisionId": order },
            "to": { "kind": "library", "player": controller(), "position": "top" },
            "order": { "kind": "decisionOrder", "decisionId": order },
        }),
    ]
}

#[test]
fn brainstorm_is_native_and_lowers_to_independent_primitives() {
    let parsed = parse_instant(
        "Draw three cards, then put two cards from your hand on top of your library in any order.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the native sequence emits a rule");
    let [order, mv] = ordered_top_of_library_move("chosenZoneCards", "remainderOrder");
    assert_eq!(
        rule["effects"],
        json!([
            {
                "kind": "drawCards",
                "player": controller(),
                "count": { "kind": "integer", "value": 3 },
            },
            {
                "kind": "chooseCards",
                "id": "chosenZoneCards",
                "player": controller(),
                "from": own_zone("hand"),
                "count": clamped_count(2, "hand"),
            },
            order,
            mv,
        ])
    );
    assert!(!rule.to_string().contains("drawThen"));
}

#[test]
fn fixed_and_variable_counts_reuse_numeric_and_collection_automata() {
    for (oracle_text, draw_count, move_count) in [
        (
            "Draw two cards, then put one card from your hand on top of your library in any order.",
            json!({ "kind": "integer", "value": 2 }),
            clamped_count(1, "hand"),
        ),
        (
            "Draw X cards, then put X cards from your hand on top of your library in any order.",
            json!({ "kind": "decisionResult", "decisionId": "xValue" }),
            json!({ "kind": "decisionResult", "decisionId": "xValue" }),
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let rule = parsed.abilities[0].rule.as_ref().unwrap();
        assert_eq!(rule["effects"][0]["count"], draw_count, "{oracle_text}");
        let selection = &rule["effects"][1];
        assert_eq!(selection["kind"], "chooseCards", "{oracle_text}");
        if oracle_text.contains(" X ") {
            assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseNumber");
            assert_eq!(selection["minimum"], move_count, "{oracle_text}");
            assert_eq!(selection["maximum"], move_count, "{oracle_text}");
            assert_eq!(
                selection["candidates"]["zone"]["kind"], "hand",
                "{oracle_text}"
            );
        } else {
            assert_eq!(selection["count"], move_count, "{oracle_text}");
            assert_eq!(selection["from"], own_zone("hand"), "{oracle_text}");
        }
        assert_eq!(rule["effects"][3]["kind"], "moveCards", "{oracle_text}");
        assert_eq!(rule["effects"][3]["to"]["position"], "top", "{oracle_text}");
    }
}

#[test]
fn promoted_near_misses_fail_in_the_responsible_child_automaton() {
    for (oracle_text, state, found) in [
        (
            "Draw three cards, then put two cards from your sideboard on top of your library in any order.",
            "zone.possessive.kind",
            "sideboard",
        ),
        (
            "Draw one cards, then put one card from your hand on top of your library in any order.",
            "collection.card-noun",
            "cards",
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_ne!(ability.status, "canonical", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.code, "oracle_machine_failure");
        assert_eq!(diagnostic.automaton.as_deref(), Some("effect-sequence"));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }

    // The graveyard is a legitimate source zone for the shared card-move
    // composition, so the former near-miss is now canonical.
    let graveyard = parse_instant(
        "Draw three cards, then put two cards from your graveyard on top of your library in any order.",
    );
    assert_eq!(graveyard.status, "canonical");
    let rule = graveyard.abilities[0].rule.as_ref().unwrap();
    assert_eq!(rule["effects"][1]["from"], own_zone("graveyard"));
    assert_eq!(rule["effects"][1]["count"], clamped_count(2, "graveyard"));
    assert_eq!(
        rule["effects"].as_array().unwrap()[2..4],
        ordered_top_of_library_move("chosenZoneCards", "remainderOrder")[..]
    );
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_instant(
        "Draw three cards, then put two cards from your hand on top of your library in any order.",
    );
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
