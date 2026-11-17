use super::*;

// Real Oracle text of a granted ability, its controlled-permanent sibling and
// the player observer: each parses to the IR the engine matches (CR 120.3).
const SELF_GROWS: &str =
    "Whenever this creature is dealt damage, put that many +1/+1 counters on it.";
const CONTROLLED_NONCOMBAT: &str = "Whenever a creature you control is dealt noncombat damage, put a +1/+1 counter on this creature.";
const OPPONENT_NONCOMBAT: &str =
    "Whenever an opponent is dealt noncombat damage, put a +1/+1 counter on this creature.";

fn creature_with(id: &str, owner: &str, text: Option<&str>) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Test");
    definition.toughness = Some("9".to_string());
    if let Some(text) = text {
        let parsed = parse_oracle_card(OracleCardParseRequest {
            card_name: "Damage watcher".to_string(),
            type_line: "Creature - Test".to_string(),
            mana_cost: Some("{1}{R}".to_string()),
            oracle_text: Some(text.to_string()),
            layout: None,
            faces: Vec::new(),
        });
        assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
        let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
        assert!(rule_is_executable(&rule), "{rule:#}");
        definition.rules = vec![rule];
    }
    test_instance(id, definition, owner)
}

/// Permanents placed directly on the battlefield still need an admitted
/// incarnation before a resolving ability may refer to them.
fn admit_battlefield(engine: &mut GameEngine) {
    for index in 0..engine.state.players.len() {
        let mut battlefield = std::mem::take(&mut engine.state.players[index].battlefield);
        for permanent in &mut battlefield {
            engine.prepare_battlefield_entry_identity(permanent);
        }
        engine.state.players[index].battlefield = battlefield;
    }
}

fn counters(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .and_then(|card| card.counters.get("+1/+1").copied())
        .unwrap_or(0) as i32
}

fn damage(engine: &mut GameEngine, id: &str, amount: i32, combat: bool) {
    engine
        .deal_damage_amount_with_kind(
            TargetRef::Permanent {
                instance_id: id.to_string(),
            },
            amount,
            "damage-source",
            combat,
        )
        .expect("damage resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_damaged_source_triggers_once_with_the_dealt_amount() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature_with("grower", "player-0", Some(SELF_GROWS)),
        creature_with("bystander", "player-0", None),
    ];
    admit_battlefield(&mut engine);
    damage(&mut engine, "bystander", 3, false);
    damage(&mut engine, "grower", 0, false);
    assert!(engine.state.stack.is_empty(), "no trigger without damage");

    damage(&mut engine, "grower", 3, true);
    assert_eq!(engine.state.stack.len(), 1, "one event, one trigger");
    assert_eq!(engine.state.stack[0].decisions["damageAmount"], json!(3));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the trigger resolves");
    assert_eq!(counters(&engine, "grower"), 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn controlled_observers_filter_by_controller_and_by_damage_kind() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature_with("watcher", "player-0", Some(CONTROLLED_NONCOMBAT)),
        creature_with("friend", "player-0", None),
    ];
    engine.state.players[1].battlefield = vec![creature_with("foe", "player-1", None)];
    admit_battlefield(&mut engine);

    damage(&mut engine, "foe", 2, false);
    damage(&mut engine, "friend", 2, true);
    assert!(
        engine.state.stack.is_empty(),
        "opposing creature and combat damage never match"
    );

    damage(&mut engine, "friend", 2, false);
    damage(&mut engine, "watcher", 1, false);
    assert_eq!(
        engine.state.stack.len(),
        2,
        "one trigger per damaged controlled creature, including the source"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_opponent_observer_ignores_combat_damage_and_its_own_controller() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature_with(
        "watcher",
        "player-0",
        Some(OPPONENT_NONCOMBAT),
    )];
    let hit = |engine: &mut GameEngine, player: &str, combat: bool| {
        engine
            .deal_damage_amount_with_kind(
                TargetRef::Player {
                    player_id: player.to_string(),
                },
                2,
                "watcher",
                combat,
            )
            .expect("damage resolves");
    };
    hit(&mut engine, "player-0", false);
    hit(&mut engine, "player-1", true);
    assert!(engine.state.stack.is_empty());

    hit(&mut engine, "player-1", false);
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(engine.state.stack[0].decisions["damageAmount"], json!(2));
}
