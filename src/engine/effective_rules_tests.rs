//! The effective rule layer: functioning printed rules plus layer-6 grants,
//! with printed indices preserved and no grant for objects off the battlefield.
use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: type_line.into(),
        mana_cost: String::new(),
        power: Some("2".into()),
        toughness: Some("2".into()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", vec![]); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        7121,
    )
    .unwrap()
}

fn add(engine: &mut GameEngine, id: &str, type_line: &str, rules: Vec<Value>) -> CardInstance {
    let card = card_instance(
        "player-0",
        &definition(id, type_line, rules),
        engine.state.players[0].battlefield.len() + 100,
    );
    engine.state.players[0].battlefield.push(card.clone());
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine.state.players[0].battlefield.last().unwrap().clone()
}

fn upkeep_rule(amount: i64) -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"stepBegan","step":"upkeep",
                 "player":{"kind":"controllerOf","object":{"kind":"self"}}},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},
                    "amount":{"kind":"integer","value":amount}}]})
}

fn static_rule(modifier: Value) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[modifier]})
}

fn creatures_you_control() -> Value {
    json!({"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},
        "where":{"kind":"cardTypeContains","value":"Creature"}})
}

#[test]
fn granted_triggered_ability_joins_the_effective_layer_only_on_the_battlefield() {
    let mut e = engine();
    let creature = add(&mut e, "grantee", "Creature", vec![upkeep_rule(1)]);
    add(
        &mut e,
        "grantor",
        "Artifact",
        vec![static_rule(json!({"kind":"grantTriggeredAbility",
            "objects":creatures_you_control(),"ability":upkeep_rule(5)}))],
    );
    let live = e.state.players[0].battlefield[0].clone();
    let rules = e.effective_rules(&live).iter().cloned().collect::<Vec<_>>();
    assert_eq!(rules, vec![upkeep_rule(1), upkeep_rule(5)]);
    // The printed layer is unchanged and keeps its own indices.
    assert_eq!(live.definition.printed_rules(), &[upkeep_rule(1)]);
    assert_eq!(
        e.effective_rules(&live)
            .printed()
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
        vec![0]
    );
    // A card in another zone has no layer-6 grant from a permanent selector.
    e.state.players[0].battlefield.remove(0);
    e.state.players[0].graveyard.push(creature.clone());
    let rules = e
        .effective_rules(&creature)
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(rules, vec![upkeep_rule(1)]);
}

#[test]
fn ability_loss_removes_printed_and_granted_rules_but_not_the_printed_layer() {
    let mut e = engine();
    add(&mut e, "grantee", "Creature", vec![upkeep_rule(1)]);
    add(
        &mut e,
        "grantor",
        "Artifact",
        vec![static_rule(json!({"kind":"grantTriggeredAbility",
            "objects":creatures_you_control(),"ability":upkeep_rule(5)}))],
    );
    let live = e.state.players[0].battlefield[0].clone();
    e.state.rule_modifiers.push(json!({
        "kind": "loseAllAbilities",
        "cardInstanceId": live.instance_id,
        "expiresAfterTurn": e.state.turn_number,
    }));
    assert_eq!(e.effective_rules(&live).iter().count(), 0);
    assert_eq!(e.effective_rules(&live).printed().count(), 0);
    assert_eq!(live.definition.printed_rules().len(), 1);
}

#[test]
fn metadata_gates_filter_printed_rules_by_printed_index() {
    let mut e = engine();
    let mut gated = upkeep_rule(2);
    gated["minimumClassLevel"] = json!(2);
    add(
        &mut e,
        "class",
        "Enchantment - Class",
        vec![upkeep_rule(1), gated],
    );
    let live = e.state.players[0].battlefield[0].clone();
    assert_eq!(
        e.effective_rules(&live)
            .printed()
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
        vec![0]
    );
    e.state.players[0].battlefield[0]
        .counters
        .insert("classLevel".to_string(), 2);
    let leveled = e.state.players[0].battlefield[0].clone();
    assert_eq!(
        e.effective_rules(&leveled)
            .printed()
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn step_trigger_reads_a_grant_exactly_once() {
    let mut e = engine();
    add(&mut e, "grantee", "Creature", vec![]);
    add(
        &mut e,
        "grantor",
        "Artifact",
        vec![static_rule(json!({"kind":"grantTriggeredAbility",
            "objects":creatures_you_control(),"ability":upkeep_rule(5)}))],
    );
    e.with_quantity_transaction(|e| {
        e.enqueue_step_triggers("upkeep");
        e.check_quantity_failure()
    })
    .unwrap();
    assert_eq!(e.state.stack.len(), 1);
}
