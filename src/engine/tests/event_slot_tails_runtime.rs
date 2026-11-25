//! Event-slot qualifiers and any-controller object events
//! (docs/oracle-event-model.md): each trailing qualifier of an event frame
//! is a field of the same matcher pattern, and the subject's controller
//! relation is an optional typed field of its object query.
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

fn object(id: &str, type_line: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    let mut instance = test_instance(id, definition, owner);
    instance.attached_to = None;
    instance
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn stack_sources(engine: &GameEngine) -> Vec<String> {
    engine
        .state
        .stack
        .iter()
        .map(|object| object.card.instance_id.clone())
        .collect()
}

fn damage(engine: &mut GameEngine, target: TargetRef, amount: i32, source: &str) {
    engine
        .deal_damage_amount_with_kind_and_origin(
            target,
            amount,
            source,
            false,
            QuantityOrigin::Effect,
            None,
        )
        .expect("damage resolves");
}

fn permanent_target(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn resolve_stack(engine: &mut GameEngine) {
    admit(engine);
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("trigger resolves");
    }
}

#[test]
fn a_damaged_creature_of_any_controller_is_the_event_object() {
    // Repercussion's event: the subject query has no controller relation.
    let rules = parsed_rules(
        "Repercussion",
        "Enchantment",
        "Whenever a creature is dealt damage, this enchantment deals that much damage to that creature's controller.",
    );
    let event = &rules[0]["event"];
    assert_eq!(event["kind"], "eventPattern", "{event}");
    assert_eq!(event["lemma"], "dealDamage");
    assert_eq!(event["recipient"]["kind"], "permanent");
    assert!(event["recipient"].get("controller").is_none(), "{event}");
    assert!(event.get("source").is_none(), "any source (CR 120.3)");

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![object("repercussion", "Enchantment", rules, "player-0")];
    engine.state.players[1].battlefield = vec![
        object("bear", "Creature — Bear", Vec::new(), "player-1"),
        object("shock", "Artifact", Vec::new(), "player-1"),
    ];
    admit(&mut engine);
    // A player and a noncreature permanent are near misses.
    damage(
        &mut engine,
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        2,
        "shock",
    );
    assert!(engine.state.stack.is_empty(), "a damaged player");
    // An opponent's creature triggers the observer of the other player,
    // with the damaged creature and the amount bound.
    damage(&mut engine, permanent_target("bear"), 1, "shock");
    assert_eq!(stack_sources(&engine), ["repercussion"]);
    let decisions = &engine.state.stack[0].decisions;
    assert_eq!(decisions["triggeringPermanentId"], "bear");
    assert_eq!(decisions["damageAmount"], 1);
    let life_before = engine.state.players[1].life;
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[1].life, life_before - 1);
}

#[test]
fn an_opponents_damaged_creature_during_your_turn_composes_without_new_code() {
    // An unseen combination: the opponent relation of the subject query,
    // the passive damage frame and the turn-owner tail.
    let rules = parsed_rules(
        "Composition Probe",
        "Enchantment",
        "Whenever a creature an opponent controls is dealt damage during your turn, you gain 1 life.",
    );
    let event = &rules[0]["event"];
    assert_eq!(event["recipient"]["controller"]["kind"], "opponentsOf");
    assert_eq!(rules[0]["eventCondition"]["kind"], "duringControllerTurn");

    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.players[0].battlefield = vec![
        object("probe", "Enchantment", rules, "player-0"),
        object("own-bear", "Creature — Bear", Vec::new(), "player-0"),
    ];
    engine.state.players[1].battlefield =
        vec![object("bear", "Creature — Bear", Vec::new(), "player-1")];
    admit(&mut engine);
    damage(&mut engine, permanent_target("own-bear"), 1, "bear");
    assert!(
        engine.state.stack.is_empty(),
        "the controller's own creature"
    );
    damage(&mut engine, permanent_target("bear"), 1, "own-bear");
    assert_eq!(stack_sources(&engine), ["probe"]);
    engine.state.stack.clear();
    // The opponent's turn is a near miss of the event-time condition.
    engine.state.active_player = 1;
    damage(&mut engine, permanent_target("bear"), 1, "own-bear");
    assert!(engine.state.stack.is_empty(), "an opponent's turn");
}

#[test]
fn leaving_without_dying_is_a_departure_that_is_not_a_death() {
    // Imperial Cosmographer's event.
    let rules = parsed_rules(
        "Imperial Cosmographer",
        "Creature — Human Advisor",
        "Whenever another creature you control leaves the battlefield without dying, put two +1/+1 counters on this creature.",
    );
    let event = &rules[0]["event"];
    assert_eq!(event["lemma"], "move", "{event}");
    assert_eq!(event["from"], "battlefield");
    assert_eq!(event["withoutDying"], true);
    assert!(event.get("to").is_none(), "any destination but a graveyard");
    assert_eq!(event["subject"]["object"]["excludeSource"], true);

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        object(
            "cosmographer",
            "Creature — Human Advisor",
            rules,
            "player-0",
        ),
        object("dying", "Creature — Bear", Vec::new(), "player-0"),
        object("bounced", "Creature — Bear", Vec::new(), "player-0"),
    ];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("dying", "destroyed")
        .expect("the creature dies");
    assert!(engine.state.stack.is_empty(), "a death is a near miss");
    engine
        .return_permanent_to_hand("bounced")
        .expect("the creature returns to its owner's hand");
    assert_eq!(stack_sources(&engine), ["cosmographer"]);
}

#[test]
fn a_graveyard_arrival_excludes_its_written_origin() {
    // Disa the Restless's event with a plain body.
    let rules = parsed_rules(
        "Origin Probe",
        "Enchantment",
        "Whenever a creature card is put into your graveyard from anywhere other than the battlefield, draw a card.",
    );
    let event = &rules[0]["event"];
    assert_eq!(event["fromNot"], "battlefield", "{event}");
    assert!(event.get("from").is_none());

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        object("probe", "Enchantment", rules, "player-0"),
        object("dying", "Creature — Bear", Vec::new(), "player-0"),
    ];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("dying", "destroyed")
        .expect("the creature dies");
    assert!(engine.state.stack.is_empty(), "from the battlefield");
    let discarded = object("discarded", "Creature — Bear", Vec::new(), "player-0");
    engine.enqueue_cards_entered_graveyard_triggers_from("player-0", &[discarded], "hand");
    assert_eq!(stack_sources(&engine), ["probe"]);
}

#[test]
fn the_sacrificed_source_is_its_own_sacrifice_event() {
    // Ordeal of Heliod's sacrifice trigger.
    let rules = parsed_rules(
        "Ordeal of Heliod",
        "Enchantment — Aura",
        "When you sacrifice this Aura, you gain 10 life.",
    );
    assert_eq!(
        rules[0]["event"],
        json!({"kind": "permanentSacrificed", "object": {"kind": "self"}})
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        // Unattached fixtures: the observer's own type line is not read.
        object("ordeal", "Enchantment", rules, "player-0"),
        object("other", "Enchantment", Vec::new(), "player-0"),
    ];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("other", "sacrificed")
        .expect("another Aura is sacrificed");
    assert!(engine.state.stack.is_empty(), "another permanent");
    engine
        .move_permanent_to_graveyard("ordeal", "destroyed")
        .expect("the source is destroyed");
    assert!(
        engine.state.stack.is_empty(),
        "a destruction is no sacrifice"
    );
}

#[test]
fn a_sacrificed_source_triggers_as_it_leaves() {
    let rules = parsed_rules(
        "Ordeal of Heliod",
        "Enchantment — Aura",
        "When you sacrifice this Aura, you gain 10 life.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![object("ordeal", "Enchantment", rules, "player-0")];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("ordeal", "sacrificed")
        .expect("the source is sacrificed");
    assert_eq!(stack_sources(&engine), ["ordeal"]);
}

#[test]
fn a_third_person_draw_ordinal_counts_that_players_draws() {
    // Faerie Mastermind's event.
    let rules = parsed_rules(
        "Faerie Mastermind",
        "Creature — Faerie Rogue",
        "Whenever an opponent draws their second card each turn, you draw a card.",
    );
    assert_eq!(
        rules[0]["event"],
        json!({"kind": "cardDrawn", "opponentOfSourceController": true, "drawOrdinal": 2})
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![object(
        "mastermind",
        "Creature — Faerie Rogue",
        rules,
        "player-0",
    )];
    for index in 0..4 {
        engine.state.players[0].library.push(object(
            &format!("own-{index}"),
            "Instant",
            Vec::new(),
            "player-0",
        ));
        engine.state.players[1].library.push(object(
            &format!("their-{index}"),
            "Instant",
            Vec::new(),
            "player-1",
        ));
    }
    admit(&mut engine);
    engine.draw_cards("player-0", 2).expect("own draws");
    assert!(engine.state.stack.is_empty(), "the controller's own draws");
    engine.draw_cards("player-1", 1).expect("first draw");
    assert!(engine.state.stack.is_empty(), "the first draw");
    engine.draw_cards("player-1", 1).expect("second draw");
    assert_eq!(stack_sources(&engine), ["mastermind"]);
    engine.draw_cards("player-1", 1).expect("third draw");
    assert_eq!(engine.state.stack.len(), 1, "only the second draw");
}
