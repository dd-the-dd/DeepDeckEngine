use super::super::*;

struct RefuseChoices;
impl DecisionProvider for RefuseChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("partition-test-provider-error"))
    }
}

fn position() -> (GameEngine, StackObject) {
    let definition = CardDefinition {
        id: "partition-fixture".into(),
        name: "Partition Fixture".into(),
        type_line: "Instant".into(),
        mana_cost: "{0}".into(),
        power: None,
        toughness: None,
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    };
    let engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 12],
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        },
        1971,
    )
    .unwrap();
    let stack = StackObject {
        id: "partition-stack".into(),
        controller: "player-0".into(),
        card: card_instance("player-0", &definition, 77),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

#[test]
fn validation_distinguishes_unhandled_from_recognized_invalid() {
    assert_eq!(
        super::effect_domain_supported(&json!({"kind":"unknownPartitionKind"})),
        None
    );
    assert_eq!(
        super::effect_domain_supported(&json!({"kind":"chooseCards"})),
        Some(false)
    );
    // Earlier accepted hooks own these, not the extracted dispatcher.
    assert_eq!(
        super::effect_domain_supported(&json!({"kind":"gainLife"})),
        None
    );
    assert!(effect_supported(&json!({
        "kind":"gainLife", "player":{"kind":"abilityController"},
        "amount":{"kind":"integer","value":2}
    })));
}

#[test]
fn handled_early_noop_does_not_fall_through_to_unknown_effect() {
    let (mut engine, stack) = position();
    let before = serde_json::to_value(&engine.state).unwrap();
    let handled = engine
        .execute_effect_domain(
            &json!({"kind":"forEachObject","objects":{"kind":"boundObjects","binding":"absent"}}),
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RefuseChoices,
        )
        .unwrap();
    assert!(handled);
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn unknown_execution_retains_the_root_error_boundary() {
    let (mut engine, stack) = position();
    let effect = json!({"kind":"unknownPartitionKind"});
    assert!(
        !engine
            .execute_effect_domain(
                &effect,
                None,
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut RefuseChoices,
            )
            .unwrap()
    );
    let error = engine
        .execute_effect(
            &effect,
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RefuseChoices,
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "unsupported effect reached execution: unknownPartitionKind"
    );
}

#[test]
fn nested_control_keeps_root_recursion_and_lazy_branches() {
    let (mut engine, stack) = position();
    let effect = json!({
        "kind":"ifTargetWasChosen", "target":{"kind":"chosenTarget","id":"absent"},
        "then":[{"kind":"unknownPartitionKind"}],
        "else":[{"kind":"gainLife","player":{"kind":"abilityController"},
                 "amount":{"kind":"integer","value":2}}]
    });
    engine
        .execute_effect(
            &effect,
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RefuseChoices,
        )
        .unwrap();
    assert_eq!(engine.state.players[0].life, 22);
    assert_eq!(engine.state.players[1].life, 20);
}

#[test]
fn provider_error_propagates_through_the_domain_without_consuming_a_branch() {
    let (mut engine, stack) = position();
    let before = serde_json::to_value(&engine.state).unwrap();
    let effect = json!({
        "kind":"chooseOneEffect", "player":{"kind":"abilityController"},
        "options":[
            {"label":"first","effects":[{"kind":"unknownPartitionKind"}]},
            {"label":"second","effects":[{"kind":"unknownPartitionKind"}]}
        ]
    });
    let error = engine
        .execute_effect(
            &effect,
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RefuseChoices,
        )
        .unwrap_err();
    assert!(error.to_string().contains("partition-test-provider-error"));
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}
