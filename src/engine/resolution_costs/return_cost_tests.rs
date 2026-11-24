//! Runtime proofs of the resolution `returnPermanentToOwnersHand` cost
//! (`sacrifice it unless you return a land you control to its owner's
//! hand`, CR 118.3, 118.12) and of measured counter removal for outcomes.
use super::*;

fn position() -> (GameEngine, StackObject) {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"base", "name":"Base", "typeLine":"Land", "manaCost":"", "rules":[]
    }))
    .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 12],
                })
                .collect(),
        },
        77123,
    )
    .unwrap();
    let source = card_instance("p0", &definition, 700);
    engine.state.players[0].battlefield.push(source.clone());
    let stack = StackObject {
        id: "stack:return-cost-source".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn island(id: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id":id, "name":id, "typeLine":"Basic Land — Island", "manaCost":"", "rules":[]
    }))
    .unwrap()
}

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "a return cost commitment makes no other choice",
        ))
    }
}

#[test]
fn return_cost_decodes_only_as_a_chosen_controlled_permanent() {
    let land = json!({"kind":"cardTypeContains","value":"Land"});
    assert!(cost_program_supported(
        &json!({"kind":"returnPermanentToOwnersHand","where":land, "excludeSource":true})
    ));
    // The activation form names one permanent; it has no resolution payment.
    assert!(!cost_program_supported(&json!({
        "kind":"returnPermanentToOwnersHand",
        "permanent":{"kind":"self"},
        "where":land,
    })));
    assert!(!cost_program_supported(
        &json!({"kind":"returnPermanentToOwnersHand"})
    ));
}

#[test]
fn return_cost_offers_the_payers_matching_permanents_and_returns_the_choice() {
    let (mut engine, mut source) = position();
    let mine = card_instance("p0", &island("mine"), 701);
    let mine_id = mine.instance_id.clone();
    engine.state.players[0].battlefield.push(mine);
    engine.state.players[1]
        .battlefield
        .push(card_instance("p1", &island("theirs"), 702));
    // Fixture objects enter with admitted incarnations, as a game would give
    // them; the ability's source copy adopts its permanent's.
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine
        .adopt_fixture_ability_source_identity(&mut source.card)
        .unwrap();
    let cost = json!({
        "kind":"returnPermanentToOwnersHand",
        "where":{"kind":"subtypeContains","value":"Island"},
        "excludeSource":true,
    });
    let options = engine
        .prepare_card_cost_options(&cost, "p0", &source, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    // Only the payer's own Island: the source is excluded and the
    // opponent's Island is not the payer's to return.
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].decisions["costReturnPermanent"], json!(mine_id));
    assert!(options[0].targets.is_empty());
    let paid = engine
        .commit_card_cost_payment(&cost, &options[0], &source, &mut NoChoices)
        .unwrap();
    assert!(paid);
    assert!(engine.permanent_position(&mine_id).is_none());
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == mine_id)
    );
}

#[test]
fn return_cost_without_a_matching_permanent_is_unpayable() {
    let (engine, source) = position();
    let cost = json!({
        "kind":"returnPermanentToOwnersHand",
        "where":{"kind":"subtypeContains","value":"Island"},
        "excludeSource":true,
    });
    assert!(
        engine
            .prepare_card_cost_options(&cost, "p0", &source, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn counter_removal_is_a_measured_optional_instruction() {
    let action = json!({
        "kind":"removeCounters",
        "permanent":{"kind":"self"},
        "counter":"+1/+1",
        "count":1,
    });
    assert!(crate::engine::optional_action::optional_action_witness_supported(&action));
    // A removal without a named object or kind has no witness.
    assert!(
        !crate::engine::optional_action::optional_action_witness_supported(
            &json!({"kind":"removeCounters","count":1})
        )
    );
}
