//! Damage family of the generic event model (docs/oracle-event-model.md):
//! one record per damage dealt to one recipient, one matcher, historical
//! kinds read as patterns, and the aggregated `deals damage` pattern.
use super::*;

fn parsed_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {:?}", parsed.abilities);
    parsed
        .abilities
        .iter()
        .map(|ability| {
            let rule = ability.rule.clone().expect("canonical rule");
            assert!(rule_is_executable(&rule), "{text}: {rule}");
            rule
        })
        .collect()
}

fn permanent(id: &str, type_line: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    let mut instance = test_instance(id, definition, owner);
    instance.attached_to = None;
    instance
}

fn creature_target(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn player_target(id: &str) -> TargetRef {
    TargetRef::Player {
        player_id: id.to_string(),
    }
}

fn damage(engine: &mut GameEngine, target: TargetRef, amount: i32, source: &str, combat: bool) {
    engine
        .deal_damage_amount_with_kind_and_origin(
            target,
            amount,
            source,
            combat,
            QuantityOrigin::Effect,
            None,
        )
        .expect("damage resolves");
}

fn resolve_stack(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("trigger resolves");
    }
}

fn setup(rules: Vec<Value>) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![permanent(
        "nishoba",
        "Creature — Cat Beast",
        rules,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        permanent("bear-a", "Creature — Bear", Vec::new(), "player-1"),
        permanent("bear-b", "Creature — Bear", Vec::new(), "player-1"),
    ];
    engine
}

#[test]
fn deals_damage_without_recipient_triggers_once_per_simultaneous_event_with_the_total() {
    // Phantom Nishoba's lifelink-like trigger.
    let rules = parsed_rules(
        "Phantom Nishoba",
        "Creature — Cat Beast Spirit",
        "Whenever this creature deals damage, you gain that much life.",
    );
    assert_eq!(
        rules[0]["event"],
        json!({"kind": "eventPattern", "lemma": "dealDamage", "source": {"kind": "self"}})
    );
    // One instruction damaging two creatures and a player is one event.
    let mut engine = setup(rules.clone());
    let scope = engine.begin_event_batch();
    damage(&mut engine, creature_target("bear-a"), 1, "nishoba", false);
    damage(&mut engine, creature_target("bear-b"), 1, "nishoba", false);
    damage(&mut engine, player_target("player-1"), 3, "nishoba", false);
    engine.end_event_batch(scope);
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].decisions["damageAmount"], 5);
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, 25);
    // Separate events trigger separately.
    let mut engine = setup(rules);
    damage(&mut engine, creature_target("bear-a"), 1, "nishoba", false);
    damage(&mut engine, player_target("player-1"), 2, "nishoba", false);
    assert_eq!(engine.state.stack.len(), 2);
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, 23);
}

#[test]
fn the_damage_kind_and_source_relation_are_pattern_attributes() {
    // Sibling and near miss: combat only, and another source.
    let rules = parsed_rules(
        "Combat Probe",
        "Creature — Cat",
        "Whenever this creature deals combat damage, you gain that much life.",
    );
    assert_eq!(rules[0]["event"]["combatOnly"], true);
    let mut engine = setup(rules);
    damage(&mut engine, player_target("player-1"), 2, "nishoba", false);
    damage(&mut engine, player_target("player-0"), 2, "bear-a", true);
    assert!(engine.state.stack.is_empty());
    damage(&mut engine, player_target("player-1"), 2, "nishoba", true);
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn an_attached_source_composes_with_the_aggregated_pattern() {
    // Armadillo Cloak's trigger: the enchanted creature's damage event.
    let rules = parsed_rules(
        "Armadillo Cloak",
        "Enchantment — Aura",
        "Whenever enchanted creature deals damage, you gain that much life.",
    );
    assert_eq!(
        rules[0]["event"]["source"],
        json!({"kind": "attachedPermanent", "attachment": {"kind": "self"}})
    );
    let mut engine = setup(Vec::new());
    let mut cloak = permanent("cloak", "Enchantment — Aura", rules, "player-0");
    cloak.attached_to = Some("nishoba".to_string());
    engine.state.players[0].battlefield.push(cloak);
    let scope = engine.begin_event_batch();
    damage(&mut engine, creature_target("bear-a"), 2, "nishoba", true);
    damage(&mut engine, player_target("player-1"), 4, "nishoba", true);
    engine.end_event_batch(scope);
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].card.instance_id, "cloak");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, 26);
}

#[test]
fn historical_damage_kinds_keep_their_bindings_on_the_matcher() {
    // `permanentDealtDamage` (self) and `controlledPermanentDealtDamage`.
    let dealt = parsed_rules(
        "Dealt Probe",
        "Creature — Bear",
        "Whenever this creature is dealt damage, you gain 1 life.",
    );
    assert_eq!(dealt[0]["event"]["kind"], "permanentDealtDamage");
    let controlled = parsed_rules(
        "Controlled Probe",
        "Enchantment",
        "Whenever a creature you control is dealt damage, you gain 1 life.",
    );
    assert_eq!(
        controlled[0]["event"]["kind"],
        "controlledPermanentDealtDamage"
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("bear", "Creature — Bear", dealt, "player-0"),
        permanent("watcher", "Enchantment", controlled, "player-0"),
    ];
    engine.state.players[1].battlefield = vec![permanent(
        "enemy",
        "Creature — Bear",
        Vec::new(),
        "player-1",
    )];
    damage(&mut engine, creature_target("enemy"), 1, "bear", false);
    assert!(engine.state.stack.is_empty(), "an opponent's creature");
    damage(&mut engine, creature_target("bear"), 1, "enemy", false);
    // The controlled observer, then the damaged permanent's own trigger.
    let sources = engine
        .state
        .stack
        .iter()
        .map(|object| object.card.instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(sources, ["watcher", "bear"]);
    for object in &engine.state.stack {
        assert_eq!(object.decisions["triggeringPermanentId"], "bear");
        assert_eq!(object.decisions["damageAmount"], 1);
    }
}

#[test]
fn combat_damage_to_a_player_reads_the_historical_kind_as_a_pattern() {
    let rules = parsed_rules(
        "Ninja Probe",
        "Creature — Ninja",
        "Whenever this creature deals combat damage to a player, draw a card.",
    );
    assert_eq!(rules[0]["event"]["kind"], "combatDamageToPlayer");
    let mut engine = setup(Vec::new());
    engine.state.players[0].battlefield =
        vec![permanent("ninja", "Creature — Ninja", rules, "player-0")];
    engine.enqueue_combat_damage_to_player_triggers("bear-a", "player-0", 2);
    assert!(engine.state.stack.is_empty(), "another source");
    engine.enqueue_combat_damage_to_player_triggers("ninja", "player-1", 2);
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPlayerId"],
        "player-1"
    );
}

#[test]
fn every_copy_of_an_aggregated_damage_trigger_sees_the_total() {
    // A trigger multiplier publishes two copies; later damage of the same
    // event grows both (CR 603.2d).
    let rules = parsed_rules(
        "Phantom Nishoba",
        "Creature — Cat Beast Spirit",
        "Whenever this creature deals damage, you gain that much life.",
    );
    let mut engine = setup(rules);
    let mut doubler = permanent("doubler", "Artifact", Vec::new(), "player-0");
    doubler.definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": {"kind": "self"},
        "activeWhile": {"kind": "inZone", "object": {"kind": "self"}, "zone": {"kind": "battlefield"}},
        "modifiers": [{
            "kind": "multiplyTriggeredAbility",
            "sources": {
                "kind": "permanents",
                "controller": {"kind": "controllerOf", "object": {"kind": "self"}},
                "where": {"kind": "cardTypeContains", "value": "Creature"},
            },
            "additionalTriggers": 1,
        }],
    })];
    engine.state.players[0].battlefield.push(doubler);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let scope = engine.begin_event_batch();
    damage(&mut engine, creature_target("bear-a"), 1, "nishoba", false);
    damage(&mut engine, player_target("player-1"), 3, "nishoba", false);
    engine.end_event_batch(scope);
    assert_eq!(engine.state.stack.len(), 2);
    for object in &engine.state.stack {
        assert_eq!(object.decisions["damageAmount"], 4);
    }
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, 28);
}
