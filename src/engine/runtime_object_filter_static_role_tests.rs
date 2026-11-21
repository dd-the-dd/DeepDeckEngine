use super::*;

#[test]
fn static_color_preparation_preserves_the_existing_numeric_consumer_domain() {
    let definition: CardDefinition = serde_json::from_value(json!({"id":"source","name":"source",
        "typeLine":"Creature","power":"2","toughness":"2","rules":[]}))
    .unwrap();
    let engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("P{n}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 4],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        7934,
    )
    .unwrap();
    let card = card_instance("p0", &definition, 90);
    let context = StackObject {
        id: "readonly-query".into(),
        controller: "p0".into(),
        card: card.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let numeric = json!({"kind":"compare","left":{"kind":"powerOf","object":{"kind":"candidate"}},
        "operator":">=","right":{"kind":"decisionResult","decisionId":"historical-source-binding"}});
    assert_eq!(
        engine
            .prepare_static_permanent_object_filter(&numeric, &card)
            .unwrap(),
        numeric
    );
    // The runtime/cost entry point still rejects a missing numeric producer;
    // the new preparation role does not weaken its established checked policy.
    assert!(
        engine
            .prepare_permanent_runtime_object_filter(
                &numeric,
                Some(&card),
                &context,
                &BTreeMap::new(),
                &BTreeMap::new()
            )
            .is_err()
    );
}
