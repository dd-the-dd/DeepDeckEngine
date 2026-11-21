use super::super::*;

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("unexpected counterability-copy choice"))
    }
}
fn position(intrinsic: bool) -> (GameEngine, StackObject) {
    let def: CardDefinition = serde_json::from_value(
        json!({"id":"spell","name":"spell","typeLine":"Instant","manaCost":"{0}","rules":[]}),
    )
    .unwrap();
    let engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![def.clone(); 4],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        707,
    )
    .unwrap();
    let mut card = card_instance("p0", &def, 20);
    if intrinsic {
        card.definition.rules.push(json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"stack"}},"modifiers":[{"kind":"cantBeCountered","object":{"kind":"self"}}]}));
    }
    let decisions = BTreeMap::from([
        ("xValue".into(), json!(3)),
        ("chosenModes".into(), json!(["mode2"])),
        ("kickerPaid".into(), json!(true)),
        ("castSourceZone".into(), json!("exile")),
        (
            "manaPayment".into(),
            json!([{"kind":"manaPaymentMetadata","cantBeCountered":true}]),
        ),
    ]);
    let object = StackObject {
        id: "original".into(),
        controller: "p0".into(),
        card,
        cant_be_countered: true,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    };
    (engine, object)
}
#[test]
fn ordinary_copy_keeps_copiable_rules_and_choices_but_not_paid_protection() {
    for intrinsic in [false, true] {
        let (mut engine, original) = position(intrinsic);
        let copied = engine.copy_stack_object_value(&original, &original.id);
        let copy = engine.state.stack.iter().find(|s| s.id == copied).unwrap();
        assert_eq!(copy.cant_be_countered, intrinsic);
        assert_eq!(
            copy.decisions, original.decisions,
            "cast choices and cost relationships must remain copied"
        );
        assert!(original.cant_be_countered);
        assert_eq!(engine.stack_object_cant_be_countered(copy), intrinsic);
    }
}
#[test]
fn resolving_copy_uses_same_origin_rule_and_live_continuous_protection_service() {
    for intrinsic in [false, true] {
        let (mut engine, original) = position(intrinsic);
        engine.execute_effect(&json!({"kind":"copyResolvingSpellIfCastFromZone","zone":"exile","optional":false,"chooseNewTargets":false}),None,&original,&mut BTreeMap::new(),&mut BTreeMap::new(),&mut NoChoices).unwrap();
        let copy = engine.state.stack.last().unwrap();
        assert_eq!(copy.cant_be_countered, intrinsic);
        assert_eq!(copy.decisions, original.decisions);
    }
    let (mut engine, original) = position(false);
    let copied = engine.copy_stack_object_value(&original, &original.id);
    let mut source = card_instance("p0", &original.card.definition, 22);
    source.definition.rules = vec![
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"cantBeCountered","objects":{"kind":"spells","controller":{"kind":"controllerOf","object":{"kind":"self"}}}}]}),
    ];
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source);
    assert!(engine.stack_object_cant_be_countered(
        engine.state.stack.iter().find(|s| s.id == copied).unwrap()
    ));
    engine.state.players[0].battlefield.clear();
    assert!(!engine.stack_object_cant_be_countered(
        engine.state.stack.iter().find(|s| s.id == copied).unwrap()
    ));
}
