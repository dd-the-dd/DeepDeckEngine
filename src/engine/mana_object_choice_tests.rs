use super::*;

struct NoAdditionalChoices;
impl DecisionProvider for NoAdditionalChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "an explicit mana declaration needs no further choice",
        ))
    }
}

fn definition(id: &str, creature: bool) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": if creature { "Creature" } else { "Artifact" },
        "power": creature.then_some("2"), "toughness": creature.then_some("2"),
    }))
    .unwrap()
}

fn engine() -> GameEngine {
    let filler = definition("filler", false);
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1515,
    )
    .unwrap();
    engine.state.step = GameStep::PrecombatMain;
    engine
}

#[test]
fn explicit_mana_objects_disambiguate_without_turning_sacrifice_into_a_target() {
    let mut engine = engine();
    let rule = json!({
        "kind": "manaAbility", "source": { "kind": "self" },
        "costs": [{ "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "activationCostPermanent0" } }],
        "declaration": { "kind": "castingDeclaration", "decisions": [{
            "kind": "chooseObjects", "id": "activationCostPermanent0", "quantity": { "kind": "exactly", "value": { "kind": "integer", "value": 1 } },
            "candidates": { "kind": "permanents", "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": { "kind": "cardTypeContains", "value": "Creature" } }
        }] },
        "effects": [{ "kind": "addMana", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "mana": "{G}" }],
    });
    assert!(rule_is_executable(&rule));
    let mut source = definition("mana-source", false);
    source.rules = vec![rule];
    let mut chosen = definition("chosen-creature", true);
    chosen.rules = vec![
        json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": { "kind": "shroud" } }),
    ];
    let mut instance = card_instance("player-1", &chosen, 2);
    instance.controller = "player-0".into();
    engine.state.players[0].battlefield = vec![
        card_instance("player-0", &source, 0),
        card_instance("player-0", &definition("retained-creature", true), 1),
        instance,
    ];
    let activation = CanonicalManaAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: "mana-source".into(),
        mana_symbols: Some(vec!["G".into()]),
    };
    assert!(
        engine
            .canonical_mana_ability_available(&activation)
            .is_err()
    );
    assert!(engine.state.players[0].mana_pool.is_empty());
    let objects = BTreeMap::from([(
        "activationCostPermanent0".into(),
        CanonicalTargetSelection::PermanentDefinition {
            definition_id: "chosen-creature".into(),
        },
    )]);
    assert!(
        engine
            .canonical_mana_ability_available_with_decisions(
                &activation,
                &objects,
                &BTreeMap::new()
            )
            .unwrap()
    );
    engine
        .activate_canonical_mana_ability_with_decisions(
            &activation,
            &objects,
            &BTreeMap::new(),
            &mut NoAdditionalChoices,
        )
        .unwrap();
    assert_eq!(engine.state.players[0].mana_pool.len(), 1);
    assert_eq!(engine.state.players[0].mana_pool[0].symbol, "G");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|c| c.definition.id == "retained-creature")
    );
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|c| c.definition.id == "chosen-creature")
    );
    assert!(engine.state.stack.is_empty());
}

#[test]
fn refused_hand_activation_does_not_discard_the_source_or_pay_a_partial_cost() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility", "source": { "kind": "self" }, "activationZone": "hand",
        "costs": [{ "kind": "discardCard", "card": { "kind": "self" } },
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "activationCostPermanent1" } }],
        "declaration": { "kind": "castingDeclaration", "decisions": [{ "kind": "chooseObjects", "id": "activationCostPermanent1",
            "quantity": { "kind": "exactly", "value": { "kind": "integer", "value": 1 } }, "candidates": { "kind": "permanents",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } }, "where": { "kind": "cardTypeContains", "value": "Creature" } } }] },
        "effects": [{ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "amount": { "kind": "integer", "value": 1 } }],
    });
    let mut source = definition("hand-source", false);
    source.rules = vec![rule];
    engine.state.players[0].hand = vec![card_instance("player-0", &source, 0)];
    let mut protected = definition("protected-creature", true);
    protected.rules = vec![
        json!({ "kind": "staticAbility", "source": { "kind": "self" },
        "activeWhile": { "kind": "inZone", "object": { "kind": "self" }, "zone": { "kind": "battlefield" } },
        "modifiers": [{ "kind": "cantBeSacrificed", "object": { "kind": "self" } }] }),
    ];
    engine.state.players[0].battlefield = vec![card_instance("player-0", &protected, 1)];
    let activation = CanonicalActivatedAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: "hand-source".into(),
        ability_rule_index: Some(0),
        targets: BTreeMap::from([(
            "activationCostPermanent1".into(),
            CanonicalTargetSelection::PermanentDefinition {
                definition_id: "protected-creature".into(),
            },
        )]),
    };
    let before = serde_json::to_value(&engine.state).unwrap();
    assert!(
        !engine
            .canonical_activated_ability_available(&activation)
            .unwrap()
    );
    assert!(
        engine
            .activate_canonical_ability(&activation, &mut NoAdditionalChoices)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn late_hand_payment_error_restores_source_pool_life_and_events() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility", "source": { "kind": "self" }, "activationZone": "hand",
        "costs": [{ "kind": "discardCard", "card": { "kind": "self" } },
            { "kind": "payMana", "manaCost": "{1}{B/P}" },
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "sac" } }],
        "declaration": { "kind": "castingDeclaration", "decisions": [{ "kind": "chooseObjects", "id": "sac",
            "quantity": { "kind": "exactly", "value": { "kind": "integer", "value": 1 } },
            "candidates": { "kind": "permanents", "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": { "kind": "cardTypeContains", "value": "Creature" } } }] },
        "effects": [{ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 1 } }],
    });
    let mut source = definition("late-hand-source", false);
    source.rules = vec![rule];
    engine.state.players[0].hand = vec![card_instance("player-0", &source, 0)];
    engine.state.players[0].battlefield = vec![card_instance(
        "player-0",
        &definition("sacrificed", true),
        1,
    )];
    engine.state.players[0].mana_pool = vec![FloatingMana {
        symbol: "C".into(),
        spend_restriction: None,
    }];
    let mut action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action
                .decisions
                .get("offBattlefieldActivation")
                .and_then(Value::as_bool)
                == Some(true)
        })
        .expect("a hand activation can pay one generic mana and two Phyrexian life");
    // Fault injection at the private payment boundary: the real generated
    // plan first consumes the pool and pays two life, then a stale extra
    // payment fails. The complete engine transaction must unwind all of it.
    action
        .decisions
        .get_mut("manaPayment")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(json!({ "kind": "phyrexianManaPayment", "lifePaid": 100 }));
    let before = serde_json::to_value(&engine.state).unwrap();
    let stack_sequence_before = engine.stack_sequence;
    assert!(
        engine
            .apply_priority_action(&action, &mut NoAdditionalChoices)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert_eq!(engine.stack_sequence, stack_sequence_before);
}

#[test]
fn duplicate_chosen_objects_cannot_pay_two_sacrifices() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility", "source": { "kind": "self" },
        "costs": [
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "first" } },
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "second" } }],
        "effects": []
    });
    let mut source = definition("duplicate-source", false);
    source.rules = vec![rule.clone()];
    let source = card_instance("player-0", &source, 0);
    let object = card_instance("player-0", &definition("artifact-creature", true), 1);
    let targets = BTreeMap::from([
        (
            "first".into(),
            TargetRef::Permanent {
                instance_id: object.instance_id.clone(),
            },
        ),
        (
            "second".into(),
            TargetRef::Permanent {
                instance_id: object.instance_id.clone(),
            },
        ),
    ]);
    engine.state.players[0].battlefield = vec![source.clone(), object];
    let action = LegalAction {
        id: "duplicate-costs".into(),
        kind: ActionKind::ActivateAbility,
        player_id: "player-0".into(),
        label: "duplicate".into(),
        card_instance_id: Some(source.instance_id.clone()),
        payment_sources: vec![],
        decisions: BTreeMap::new(),
        targets: targets.clone(),
        target_order: vec![],
        attacker_id: None,
        blocker_id: None,
    };
    let before = serde_json::to_value(&engine.state).unwrap();
    assert!(!engine.activated_sacrifice_costs_are_payable(
        &rule,
        "player-0",
        &source.instance_id,
        &targets
    ));
    assert!(
        engine
            .prepare_activated_cost_payment(&action, &rule)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}
