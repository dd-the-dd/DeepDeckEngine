//! Typed object references in consumer slots: the event's object as a damage
//! source with last-known information, as an attach recipient, and as the
//! card a card move acts on; the one-object adapter's CR 400.7 identity rule.
use super::*;

fn definition(id: &str, type_line: &str, power: Option<&str>, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "typeLine": type_line,
        "manaCost": "",
        "power": power,
        "toughness": power,
        "rules": rules,
    }))
    .unwrap()
}

fn observed() -> Value {
    json!({"kind":"observedEventObject","event":"permanentEntered"})
}

/// `Whenever a creature you control enters, EFFECTS` on a watcher the
/// first player controls.
fn watcher_rule(effects: Vec<Value>) -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"permanentEntered","where":null,
            "player":{"kind":"controllerOf","object":{"kind":"self"}}},
        "effects":effects})
}

fn engine_with_watcher(watcher_type: &str, effects: Vec<Value>) -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("Player{n}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Land", None, Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        4077,
    )
    .unwrap();
    let mut watcher = card_instance(
        "p0",
        &definition("watcher", watcher_type, None, vec![watcher_rule(effects)]),
        1,
    );
    engine.prepare_battlefield_entry_identity(&mut watcher);
    engine.state.players[0].battlefield.push(watcher);
    engine
}

fn enter_creature(engine: &mut GameEngine, type_line: &str, power: &str) -> String {
    let card = card_instance(
        "p0",
        &definition("entrant", type_line, Some(power), Vec::new()),
        90,
    );
    let id = card.instance_id.clone();
    let installed = engine.install_prepared_battlefield_entries(vec![(0, card)]);
    for source in installed {
        engine.publish_prepared_enter_battlefield_triggers(&source);
    }
    id
}

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "unexpected decision in a typed-reference scenario",
        ))
    }
}

#[test]
fn an_entering_dragon_deals_damage_as_it_last_existed_after_it_left() {
    // Scourge of Valkas: `it deals X damage`, here with X = its power, read
    // after the entering Dragon was exiled in response (CR 608.2h).
    for recipient in [
        json!({"kind":"opponentsOf","player":{"kind":"abilityController"}}),
        json!({"kind":"triggeringPlayer"}),
    ] {
        let mut engine = engine_with_watcher(
            "Enchantment",
            vec![json!({"kind":"dealDamage", "source": observed(),
                "amount":{"kind":"powerOf","object":observed()},
                "recipient": recipient.clone()})],
        );
        let dragon = enter_creature(&mut engine, "Creature — Dragon", "5");
        assert_eq!(engine.state.stack.len(), 1);
        engine.exile_permanent(&dragon).unwrap();
        assert!(engine.permanent_position(&dragon).is_none());
        engine.resolve_top_stack(&mut NoChoices).unwrap();
        let lives = engine
            .state
            .players
            .iter()
            .map(|player| player.life)
            .collect::<Vec<_>>();
        let expected = if value_kind(&recipient) == Some("opponentsOf") {
            [20, 15]
        } else {
            [15, 20]
        };
        assert_eq!(lives, expected, "recipient {recipient}");
    }
}

#[test]
fn a_live_entrant_is_the_damage_source_while_it_is_the_observed_incarnation() {
    let mut engine = engine_with_watcher(
        "Enchantment",
        vec![
            json!({"kind":"dealDamage", "source": observed(), "amount": 2,
            "recipient":{"kind":"opponentsOf","player":{"kind":"abilityController"}}}),
        ],
    );
    let entrant = enter_creature(&mut engine, "Creature — Dragon", "4");
    let stack = engine.state.stack[0].clone();
    let source = engine
        .resolve_effect_damage_source(Some(&observed()), &stack, &BTreeMap::new())
        .unwrap()
        .unwrap();
    assert_eq!(source.instance_id, entrant);
    assert!(source.profile.is_none());
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.state.players[1].life, 18);
}

#[test]
fn a_declared_damage_source_that_is_gone_deals_nothing_but_a_bound_one_is_an_error() {
    let engine = engine_with_watcher("Enchantment", Vec::new());
    let stack = StackObject {
        id: "typed-source".into(),
        controller: "p0".into(),
        card: engine.state.players[0].battlefield[0].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let gone_target = json!({"kind":"chosenTarget","id":"missing"});
    assert!(
        engine
            .resolve_effect_damage_source(Some(&gone_target), &stack, &BTreeMap::new())
            .unwrap()
            .is_none()
    );
    let bindings = BTreeMap::from([(
        "createdTokens".to_string(),
        RuntimeBinding::Objects(vec!["not-on-the-battlefield".to_string()]),
    )]);
    let bound = json!({"kind":"singleBoundObject","binding":"createdTokens"});
    assert!(
        engine
            .resolve_effect_damage_source(Some(&bound), &stack, &bindings)
            .is_err()
    );
}

#[test]
fn an_equipment_attaches_itself_to_the_observed_entrant() {
    // Inventor's Goggles: `attach this Equipment to it`.
    let mut engine = engine_with_watcher(
        "Artifact — Equipment",
        vec![json!({"kind":"attachPermanent","attachment":{"kind":"self"},"to":observed()})],
    );
    let entrant = enter_creature(&mut engine, "Creature — Human Artificer", "1");
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    let watcher = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.id == "watcher")
        .unwrap();
    assert_eq!(watcher.attached_to.as_deref(), Some(entrant.as_str()));
}

#[test]
fn the_observed_entrant_card_moves_to_its_owners_hand_through_its_lifecycle() {
    // `return it to its owner's hand` with `it` the event's object.
    let mut engine = engine_with_watcher(
        "Enchantment",
        vec![json!({"kind":"moveCards",
            "cards":{"kind":"referencedObject","object":observed()},
            "to":{"kind":"hand","player":{"kind":"ownerOf","object":{"kind":"candidate"}}}})],
    );
    let entrant = enter_creature(&mut engine, "Creature — Human", "2");
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert!(engine.permanent_position(&entrant).is_none());
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == entrant)
    );
}

#[test]
fn a_captured_bound_object_is_found_only_while_it_is_the_same_object() {
    // Greasefang: the returned Vehicle, captured by the delayed trigger, is
    // returned to hand only while it is still that permanent (CR 400.7).
    let mut engine = engine_with_watcher("Enchantment", Vec::new());
    let mut vehicle = card_instance(
        "p0",
        &definition("vehicle", "Artifact — Vehicle", Some("3"), Vec::new()),
        70,
    );
    engine.prepare_battlefield_entry_identity(&mut vehicle);
    let vehicle_id = vehicle.instance_id.clone();
    engine.state.players[0].battlefield.push(vehicle);
    let mut decisions = BTreeMap::new();
    decisions.insert("movedOntoBattlefield".to_string(), json!([vehicle_id]));
    decisions.insert(
        captured_binding::captured_identities_key("movedOntoBattlefield"),
        engine.captured_object_identities(std::slice::from_ref(&vehicle_id)),
    );
    let stack = StackObject {
        id: "delayed-return".into(),
        controller: "p0".into(),
        card: engine.state.players[0].battlefield[0].clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    };
    let reference = json!({"kind":"referencedObject",
        "object":{"kind":"singleBoundObject","binding":"movedOntoBattlefield"}});
    assert!(object_expression_supported(&reference));
    assert_eq!(
        engine.referenced_object_ids(&reference, &stack, &BTreeMap::new(), &BTreeMap::new()),
        Some(vec![vehicle_id.clone()])
    );
    engine.exile_permanent(&vehicle_id).unwrap();
    assert_eq!(
        engine.referenced_object_ids(&reference, &stack, &BTreeMap::new(), &BTreeMap::new()),
        Some(Vec::new())
    );
}

#[test]
fn the_source_card_is_not_found_after_it_left_the_battlefield_under_the_ability() {
    let mut engine = engine_with_watcher("Enchantment", Vec::new());
    let source = engine.state.players[0].battlefield[0].clone();
    let source_id = source.instance_id.clone();
    engine.state.stack.push(StackObject {
        id: "source-ability".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    let reference = json!({"kind":"referencedObject","object":{"kind":"self"}});
    let before = engine.state.stack[0].clone();
    assert_eq!(
        engine.referenced_object_ids(&reference, &before, &BTreeMap::new(), &BTreeMap::new()),
        Some(vec![source_id.clone()])
    );
    engine.exile_permanent(&source_id).unwrap();
    let after = engine.state.stack[0].clone();
    assert_eq!(
        engine.referenced_object_ids(&reference, &after, &BTreeMap::new(), &BTreeMap::new()),
        Some(Vec::new())
    );
}

#[test]
fn the_adapter_refuses_players_and_unknown_references() {
    for object in [
        json!({"kind":"abilityController"}),
        json!({"kind":"unimplementedObject"}),
    ] {
        assert!(!object_expression_supported(
            &json!({"kind":"referencedObject","object":object})
        ));
    }
    assert!(!object_expression_supported(
        &json!({"kind":"referencedObject","object":{"kind":"self"},"extra":true})
    ));
}
