//! Real Oracle text -> primitive IR -> engine behavior for the league
//! reference cohort: quantified opponent count comparisons, `Activate only
//! if`, battlefield returns of zone collections, exiled-object returns, `all`
//! permanent returns, the sacrificed source as `it`, declared opponent life
//! subjects and the combat-damage event player.
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn permanent(id: &str, type_line: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), owner)
}

fn stack_object(
    card: CardInstance,
    rule: &Value,
    targets: BTreeMap<String, TargetRef>,
    decisions: BTreeMap<String, Value>,
) -> StackObject {
    StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions,
        targets,
    }
}

fn resolve(engine: &mut GameEngine, rule: &Value, stack: &StackObject) {
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the parsed program resolves");
}

fn target_id(rule: &Value) -> String {
    rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string()
}

fn lands(owner: &str, count: usize) -> Vec<CardInstance> {
    (0..count)
        .map(|index| {
            permanent(
                &format!("{owner}-land-{index}"),
                "Basic Land - Plains",
                owner,
            )
        })
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_opponent_count_comparison_reads_both_live_counts() {
    let rules = parsed_rules(
        "Land Tax",
        "Enchantment",
        "At the beginning of your upkeep, if an opponent controls more lands than you, you may search your library for up to three basic land cards, reveal them, put them into your hand, then shuffle.",
    );
    let condition = &rules[0]["condition"];
    let evaluate = |mine: usize, theirs: usize| {
        let mut engine = test_engine(3);
        engine.state.players[0].battlefield = lands("player-0", mine);
        engine.state.players[1].battlefield = lands("player-1", theirs);
        engine.state.players[2].battlefield = lands("player-2", 1);
        engine.condition_value_with_source(condition, "player-0", &BTreeMap::new(), None)
    };
    assert!(evaluate(2, 3), "one opponent has more lands");
    assert!(!evaluate(3, 3), "equal counts are not more");
    assert!(!evaluate(4, 3), "no opponent has more lands");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_activation_condition_gates_the_activation() {
    let rules = parsed_rules(
        "Weathered Wayfarer",
        "Creature — Human Nomad Cleric",
        "{W}, {T}: Search your library for a land card, reveal it, put it into your hand, then shuffle. Activate only if an opponent controls more lands than you.",
    );
    let condition = &rules[0]["activationCondition"];
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = lands("player-1", 2);
    engine.state.players[0].battlefield = lands("player-0", 2);
    assert!(!engine.condition_value_with_source(condition, "player-0", &BTreeMap::new(), None));
    engine.state.players[0].battlefield = lands("player-0", 1);
    assert!(engine.condition_value_with_source(condition, "player-0", &BTreeMap::new(), None));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_zone_collection_returns_to_the_battlefield_tapped() {
    let rules = parsed_rules(
        "Splendid Reclamation",
        "Sorcery",
        "Return all land cards from your graveyard to the battlefield tapped.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.clear();
    engine.state.players[0].graveyard = vec![
        permanent("dead-land", "Basic Land - Forest", "player-0"),
        permanent("dead-bear", "Creature - Bear", "player-0"),
    ];
    let stack = stack_object(
        permanent("reclamation", "Sorcery", "player-0"),
        &rules[0],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    resolve(&mut engine, &rules[0], &stack);
    let battlefield = &engine.state.players[0].battlefield;
    assert_eq!(battlefield.len(), 1);
    assert_eq!(battlefield[0].instance_id, "dead-land");
    assert!(battlefield[0].tapped);
    assert_eq!(
        engine.state.players[0].graveyard.len(),
        1,
        "the creature card stays"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_exiled_object_returns_under_its_owners_control() {
    let rules = parsed_rules(
        "Probe Blinker",
        "Creature — Spirit",
        "{3}: Exile another target creature you control, then return it to the battlefield under its owner's control.",
    );
    let rule = &rules[0];
    let mut engine = test_engine(2);
    let mut borrowed = permanent("borrowed", "Creature - Bear", "player-1");
    borrowed.controller = "player-0".to_string();
    engine.state.players[0].battlefield = vec![borrowed];
    let stack = stack_object(
        permanent("blinker", "Creature - Spirit", "player-0"),
        rule,
        BTreeMap::from([(
            target_id(rule),
            TargetRef::Permanent {
                instance_id: "borrowed".to_string(),
            },
        )]),
        BTreeMap::new(),
    );
    resolve(&mut engine, rule, &stack);
    assert!(engine.state.players[0].battlefield.is_empty());
    assert!(engine.state.players[0].exile.is_empty() && engine.state.players[1].exile.is_empty());
    let returned = &engine.state.players[1].battlefield;
    assert_eq!(returned.len(), 1, "the owner controls the returned object");
    assert_eq!(returned[0].controller, "player-1");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_all_attacking_creatures_return_to_their_owners_hands() {
    let rules = parsed_rules(
        "Aetherize",
        "Instant",
        "Return all attacking creatures to their owner's hand.",
    );
    let mut engine = test_engine(2);
    let mut attacker = permanent("attacker", "Creature - Bear", "player-1");
    attacker.flags.insert("attacking".to_string(), true);
    let bystander = permanent("bystander", "Creature - Bear", "player-1");
    engine.state.players[1].battlefield = vec![attacker, bystander];
    engine.state.players[1].hand.clear();
    let stack = stack_object(
        permanent("aetherize", "Instant", "player-0"),
        &rules[0],
        BTreeMap::new(),
        BTreeMap::new(),
    );
    resolve(&mut engine, &rules[0], &stack);
    assert_eq!(
        engine.state.players[1]
            .battlefield
            .iter()
            .map(|card| card.instance_id.as_str())
            .collect::<Vec<_>>(),
        ["bystander"]
    );
    assert_eq!(engine.state.players[1].hand.len(), 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_sacrificed_source_deals_the_damage() {
    let rules = parsed_rules(
        "Pyrite Spellbomb",
        "Artifact",
        "{R}, Sacrifice this artifact: It deals 2 damage to any target.",
    );
    let rule = &rules[0];
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard = vec![permanent("spellbomb", "Artifact", "player-0")];
    let stack = stack_object(
        permanent("spellbomb", "Artifact", "player-0"),
        rule,
        BTreeMap::from([(
            target_id(rule),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
        BTreeMap::new(),
    );
    resolve(&mut engine, rule, &stack);
    assert_eq!(engine.state.players[1].life, 18);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_declared_opponent_loses_life() {
    let rules = parsed_rules(
        "Probe Drain",
        "Instant",
        "Target opponent loses 2 life. Draw a card.",
    );
    let rule = &rules[0];
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "isOpponentOf"
    );
    let mut engine = test_engine(2);
    let stack = stack_object(
        permanent("drain", "Instant", "player-0"),
        rule,
        BTreeMap::from([(
            target_id(rule),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
        BTreeMap::new(),
    );
    resolve(&mut engine, rule, &stack);
    assert_eq!(engine.state.players[1].life, 18);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_combat_damage_event_player_discards() {
    let rules = parsed_rules(
        "Probe Specter",
        "Creature — Specter",
        "Whenever this creature deals combat damage to a player, that player discards a card.",
    );
    let rule = &rules[0];
    let mut engine = test_engine(3);
    let before = engine.state.players[2].hand.len();
    let stack = stack_object(
        permanent("specter", "Creature - Specter", "player-0"),
        rule,
        BTreeMap::new(),
        BTreeMap::from([(
            "triggeringPlayerId".to_string(),
            Value::String("player-2".to_string()),
        )]),
    );
    resolve(&mut engine, rule, &stack);
    assert_eq!(engine.state.players[2].hand.len(), before - 1);
    assert_eq!(
        engine.state.players[1].hand.len(),
        before,
        "only the damaged player"
    );
}
