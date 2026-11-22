//! Generic combat event model (docs/oracle-event-model.md): one record per
//! combat action, one matcher for every pattern. Real Oracle texts go through
//! the parser, the emitted `eventPattern` IR and the runtime matcher.
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

fn creature(
    id: &str,
    type_line: &str,
    mana_cost: &str,
    rules: Vec<Value>,
    owner: &str,
) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition.rules = rules;
    test_instance(id, definition, owner)
}

fn vanilla(id: &str, owner: &str) -> CardInstance {
    creature(id, "Creature — Soldier", "{0}", Vec::new(), owner)
}

/// The attack declaration as the combat step performs it: assignments, the
/// per-attacker log, the declaration log, then trigger collection.
fn declare_attack(engine: &mut GameEngine, attacker_ids: &[&str]) {
    let attacker_ids = attacker_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let active = engine.state.players[engine.state.active_player].id.clone();
    for attacker_id in &attacker_ids {
        let (player_index, card_index) = engine.permanent_position(attacker_id).expect("attacker");
        engine.state.players[player_index].battlefield[card_index]
            .flags
            .insert("attacking".to_string(), true);
        engine.state.combat.attackers.push(DeclaredAttacker {
            attacker_id: attacker_id.clone(),
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        });
        engine.record_event(
            "attackerDeclared",
            Some(active.clone()),
            Some(attacker_id.clone()),
            json!({
                "defendingPlayerId": "player-1",
                "objectIdentity": engine.object_identity_value(attacker_id),
            }),
        );
    }
    engine.record_event(
        "attackDeclared",
        Some(active),
        None,
        json!({"attackerIds": attacker_ids}),
    );
    engine.enqueue_declared_attacker_triggers(&attacker_ids);
}

/// One block pair as the declare-blockers step performs it.
fn declare_block(engine: &mut GameEngine, attacker_id: &str, blocker_id: &str) {
    engine.state.combat.blockers.push(DeclaredBlocker {
        attacker_id: attacker_id.to_string(),
        blocker_id: blocker_id.to_string(),
    });
    engine.record_event(
        "blockerDeclared",
        Some("player-1".to_string()),
        Some(blocker_id.to_string()),
        json!({
            "attackerId": attacker_id,
            "objectIdentity": engine.object_identity_value(blocker_id),
        }),
    );
    let record = engine.block_record(attacker_id, blocker_id, "player-1");
    engine
        .publish_block_observer_triggers(&record)
        .expect("block observers publish");
    engine
        .publish_block_attachment_triggers(&record)
        .expect("block attachments publish");
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

fn power(engine: &GameEngine, id: &str) -> i32 {
    let (player_index, card_index) = engine.permanent_position(id).expect("permanent");
    engine.current_power(&engine.state.players[player_index].battlefield[card_index])
}

fn flying_rules() -> Vec<Value> {
    parsed_rules("Flyer", "Creature — Bird", "Flying")
}

#[test]
fn attacks_alone_matches_only_a_single_declared_attacker() {
    // Rogue Kavu.
    let rules = parsed_rules(
        "Rogue Kavu",
        "Creature — Kavu",
        "Whenever this creature attacks alone, it gets +2/+0 until end of turn.",
    );
    assert_eq!(rules[0]["event"]["kind"], "eventPattern");
    assert_eq!(rules[0]["event"]["attackingAlone"], true);
    for (attackers, expected_power) in [(vec!["kavu"], 4), (vec!["kavu", "friend"], 2)] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![
            creature("kavu", "Creature — Kavu", "{0}", rules.clone(), "player-0"),
            vanilla("friend", "player-0"),
        ];
        declare_attack(&mut engine, &attackers);
        assert_eq!(engine.state.stack.len(), usize::from(attackers.len() == 1));
        resolve_stack(&mut engine);
        assert_eq!(power(&engine, "kavu"), expected_power, "{attackers:?}");
    }
}

#[test]
fn attacks_and_isnt_blocked_matches_declared_unblocked_attackers_only() {
    // Eternal of Harsh Truths: the defending player is the triggering player.
    let rules = parsed_rules(
        "Eternal of Harsh Truths",
        "Creature — Zombie Cleric",
        "Whenever this creature attacks and isn't blocked, draw a card.",
    );
    assert_eq!(rules[0]["event"]["lemma"], "becomeUnblocked");
    let setup = || {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![creature(
            "eternal",
            "Creature — Zombie Cleric",
            "{0}",
            rules.clone(),
            "player-0",
        )];
        engine.state.players[1].battlefield = vec![vanilla("wall", "player-1")];
        engine
    };
    // Unblocked: one trigger, bound to the attacker and the defending player.
    let mut engine = setup();
    declare_attack(&mut engine, &["eternal"]);
    assert!(engine.state.stack.is_empty());
    engine
        .publish_unblocked_triggers(&BTreeMap::new())
        .expect("unblocked observers publish");
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPlayerId"],
        "player-1"
    );
    let hand = engine.state.players[0].hand.len();
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
    // Blocked: no trigger.
    let mut engine = setup();
    declare_attack(&mut engine, &["eternal"]);
    declare_block(&mut engine, "eternal", "wall");
    engine
        .publish_unblocked_triggers(&BTreeMap::from([(
            "eternal".to_string(),
            vec!["wall".to_string()],
        )]))
        .expect("unblocked observers publish");
    assert!(engine.state.stack.is_empty());
    // Put onto the battlefield attacking: it never attacked (CR 508.4).
    let mut engine = setup();
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: "eternal".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
    engine
        .publish_unblocked_triggers(&BTreeMap::new())
        .expect("unblocked observers publish");
    assert!(engine.state.stack.is_empty());
}

#[test]
fn blocks_a_qualified_creature_reads_the_counterpart_query() {
    // Netcaster Spider.
    let rules = parsed_rules(
        "Netcaster Spider",
        "Creature — Spider",
        "Whenever this creature blocks a creature with flying, this creature gets +2/+0 until end of turn.",
    );
    assert_eq!(rules[0]["event"]["counterpart"]["role"], "attacker");
    for (attacker_rules, expected_power) in [(flying_rules(), 4), (Vec::new(), 2)] {
        let flying = !attacker_rules.is_empty();
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![creature(
            "attacker",
            "Creature — Bird",
            "{0}",
            attacker_rules,
            "player-0",
        )];
        engine.state.players[1].battlefield = vec![creature(
            "spider",
            "Creature — Spider",
            "{0}",
            rules.clone(),
            "player-1",
        )];
        declare_attack(&mut engine, &["attacker"]);
        declare_block(&mut engine, "attacker", "spider");
        assert_eq!(engine.state.stack.len(), usize::from(flying));
        resolve_stack(&mut engine);
        assert_eq!(power(&engine, "spider"), expected_power, "flying {flying}");
    }
}

#[test]
fn becomes_blocked_by_a_counterpart_triggers_once_per_matching_blocker() {
    // Tel-Jilad Wolf (CR 509.3d).
    let rules = parsed_rules(
        "Tel-Jilad Wolf",
        "Creature — Wolf",
        "Whenever this creature becomes blocked by an artifact creature, this creature gets +3/+3 until end of turn.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature(
        "wolf",
        "Creature — Wolf",
        "{0}",
        rules,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        creature(
            "golem-a",
            "Artifact Creature — Golem",
            "{0}",
            Vec::new(),
            "player-1",
        ),
        vanilla("soldier", "player-1"),
        creature(
            "golem-b",
            "Artifact Creature — Golem",
            "{0}",
            Vec::new(),
            "player-1",
        ),
    ];
    declare_attack(&mut engine, &["wolf"]);
    for blocker in ["golem-a", "soldier", "golem-b"] {
        declare_block(&mut engine, "wolf", blocker);
    }
    assert_eq!(engine.state.stack.len(), 2);
    resolve_stack(&mut engine);
    assert_eq!(power(&engine, "wolf"), 8);
}

#[test]
fn one_or_more_counterparts_trigger_once_at_the_first_matching_blocker() {
    // Serra Inquisitors: a white blocker first, then two black ones.
    let rules = parsed_rules(
        "Serra Inquisitors",
        "Creature — Human Cleric",
        "Whenever this creature blocks or becomes blocked by one or more black creatures, this creature gets +2/+0 until end of turn.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature(
        "inquisitors",
        "Creature — Human Cleric",
        "{0}",
        rules,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        creature("white", "Creature — Soldier", "{W}", Vec::new(), "player-1"),
        creature(
            "black-a",
            "Creature — Zombie",
            "{B}",
            Vec::new(),
            "player-1",
        ),
        creature(
            "black-b",
            "Creature — Zombie",
            "{B}",
            Vec::new(),
            "player-1",
        ),
    ];
    declare_attack(&mut engine, &["inquisitors"]);
    declare_block(&mut engine, "inquisitors", "white");
    assert!(engine.state.stack.is_empty());
    declare_block(&mut engine, "inquisitors", "black-a");
    assert_eq!(engine.state.stack.len(), 1);
    declare_block(&mut engine, "inquisitors", "black-b");
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn first_time_each_turn_reads_the_per_turn_attack_ordinal() {
    let rules = parsed_rules(
        "Ordinal Probe",
        "Creature — Samurai",
        "Whenever this creature attacks for the first time each turn, draw a card.",
    );
    assert_eq!(rules[0]["event"]["turnOrdinal"], 1);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature(
        "samurai",
        "Creature — Samurai",
        "{0}",
        rules,
        "player-0",
    )];
    admit(&mut engine);
    declare_attack(&mut engine, &["samurai"]);
    assert_eq!(engine.state.stack.len(), 1);
    resolve_stack(&mut engine);
    // An additional combat phase of the same turn.
    engine.state.combat.attackers.clear();
    declare_attack(&mut engine, &["samurai"]);
    assert!(engine.state.stack.is_empty());
    // The next turn starts a new count.
    engine.state.turn_number += 1;
    engine.state.combat.attackers.clear();
    declare_attack(&mut engine, &["samurai"]);
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn historical_combat_kinds_keep_their_multiplicity_on_the_matcher() {
    // Bushido (`declaredBlocker` or `permanentBecameBlocked`): once each
    // combat even when blocked by two creatures (CR 509.3c).
    let bushido = parsed_rules("Bushido Probe", "Creature — Samurai", "Bushido 1");
    assert_eq!(
        bushido[0]["event"]["events"][1]["kind"],
        "permanentBecameBlocked"
    );
    // A controlled-creature observer (`controlledCreatureBecameBlocked`).
    let observer = parsed_rules(
        "Observer",
        "Enchantment",
        "Whenever a creature you control becomes blocked, draw a card.",
    );
    assert_eq!(
        observer[0]["event"]["kind"],
        "controlledCreatureBecameBlocked"
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature("samurai", "Creature — Samurai", "{0}", bushido, "player-0"),
        test_instance(
            "observer",
            CardDefinition {
                rules: observer,
                ..test_definition("observer", "Enchantment")
            },
            "player-0",
        ),
    ];
    engine.state.players[1].battlefield = vec![
        vanilla("blocker-a", "player-1"),
        vanilla("blocker-b", "player-1"),
    ];
    declare_attack(&mut engine, &["samurai"]);
    declare_block(&mut engine, "samurai", "blocker-a");
    // The bushido trigger then the observer, in battlefield order.
    let sources = engine
        .state
        .stack
        .iter()
        .map(|object| object.card.instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(sources, ["samurai", "observer"]);
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPlayerId"],
        "player-1"
    );
    declare_block(&mut engine, "samurai", "blocker-b");
    assert_eq!(engine.state.stack.len(), 2);
    resolve_stack(&mut engine);
    assert_eq!(power(&engine, "samurai"), 3);
}

#[test]
fn an_unwritten_combination_composes_without_card_code() {
    // Composed IR no card prints: a Wall you control blocks a flyer.
    let flyer_filter = parsed_rules(
        "Netcaster Spider",
        "Creature — Spider",
        "Whenever this creature blocks a creature with flying, this creature gets +2/+0 until end of turn.",
    )[0]["event"]["counterpart"]["where"]
        .clone();
    let rule = json!({
        "kind": "triggeredAbility",
        "source": {"kind": "self"},
        "event": {
            "kind": "eventPattern",
            "lemma": "block",
            "subject": {"role": "blocker", "object": {
                "kind": "controlledPermanent",
                "player": {"kind": "controllerOf", "object": {"kind": "self"}},
                "where": {"kind": "subtypeContains", "value": "Wall"},
            }},
            "counterpart": {"role": "attacker", "where": flyer_filter},
        },
        "effects": [{
            "kind": "drawCards",
            "player": {"kind": "controllerOf", "object": {"kind": "self"}},
            "count": {"kind": "integer", "value": 1},
        }],
    });
    assert!(rule_is_executable(&rule));
    // And a parsed combination of two modifiers.
    let alone_first = parsed_rules(
        "Combination Probe",
        "Creature — Samurai",
        "Whenever this creature attacks alone for the first time each turn, draw a card.",
    );
    assert_eq!(alone_first[0]["event"]["attackingAlone"], true);
    assert_eq!(alone_first[0]["event"]["turnOrdinal"], 1);

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature(
            "flyer",
            "Creature — Bird",
            "{0}",
            flying_rules(),
            "player-0",
        ),
        vanilla("walker", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![
        test_instance(
            "observer",
            CardDefinition {
                rules: vec![rule],
                ..test_definition("observer", "Enchantment")
            },
            "player-1",
        ),
        creature("wall", "Creature — Wall", "{0}", Vec::new(), "player-1"),
        creature("guard", "Creature — Soldier", "{0}", Vec::new(), "player-1"),
    ];
    declare_attack(&mut engine, &["flyer", "walker"]);
    declare_block(&mut engine, "walker", "wall");
    declare_block(&mut engine, "flyer", "guard");
    assert!(engine.state.stack.is_empty(), "near misses do not trigger");
    engine.state.combat.blockers.clear();
    engine.state.turn_number += 1;
    declare_block(&mut engine, "flyer", "wall");
    assert_eq!(engine.state.stack.len(), 1);
    assert_eq!(
        engine.state.stack[0].decisions["blockingPermanentId"],
        "wall"
    );
    assert_eq!(
        engine.state.stack[0].decisions["blockedPermanentId"],
        "flyer"
    );
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

/// The permanent leaves and returns as a new object (CR 400.7).
fn new_incarnation(engine: &mut GameEngine, id: &str) {
    let (player_index, card_index) = engine.permanent_position(id).expect("permanent");
    let mut card = engine.state.players[player_index].battlefield[card_index].clone();
    let before = GameEngine::tracked_zone_identity(&card).map(ToOwned::to_owned);
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("a new incarnation");
    assert_ne!(
        GameEngine::tracked_zone_identity(&card).map(ToOwned::to_owned),
        before
    );
    engine.state.players[player_index].battlefield[card_index] = card;
}

fn on_battlefield(engine: &GameEngine, id: &str) -> bool {
    engine.permanent_position(id).is_some()
}

#[test]
fn that_creature_binds_the_other_combatant_of_the_record() {
    // Ogre Leadfoot: `that creature` is the blocker it becomes blocked by.
    let rules = parsed_rules(
        "Ogre Leadfoot",
        "Creature — Ogre",
        "Whenever this creature becomes blocked by an artifact creature, destroy that creature.",
    );
    assert_eq!(
        rules[0]["effects"][0],
        json!({"kind": "destroyPermanent", "permanent": {"kind": "combatCounterpart"}})
    );
    let setup = || {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![creature(
            "ogre",
            "Creature — Ogre",
            "{0}",
            rules.clone(),
            "player-0",
        )];
        engine.state.players[1].battlefield = vec![creature(
            "golem",
            "Artifact Creature — Golem",
            "{0}",
            Vec::new(),
            "player-1",
        )];
        admit(&mut engine);
        declare_attack(&mut engine, &["ogre"]);
        declare_block(&mut engine, "ogre", "golem");
        assert_eq!(engine.state.stack.len(), 1);
        engine
    };
    let mut engine = setup();
    resolve_stack(&mut engine);
    assert!(!on_battlefield(&engine, "golem"));
    assert!(on_battlefield(&engine, "ogre"));
    // The blocker left and returned before resolution: a new object that
    // the reference no longer denotes (CR 400.7).
    let mut engine = setup();
    new_incarnation(&mut engine, "golem");
    resolve_stack(&mut engine);
    assert!(on_battlefield(&engine, "golem"));
}

#[test]
fn each_branch_of_a_block_union_binds_its_own_counterpart() {
    // Sibling of Cockatrice without its end-of-combat delay: the attacker
    // it blocks, or each blocker it becomes blocked by.
    let rules = parsed_rules(
        "Basilisk Probe",
        "Creature — Basilisk",
        "Whenever this creature blocks or becomes blocked by a creature, destroy that creature. It can't be regenerated.",
    );
    assert_eq!(rules[0]["effects"][0]["cannotRegenerate"], true);
    // As the blocker: the attacker is destroyed.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![vanilla("attacker", "player-0")];
    engine.state.players[1].battlefield = vec![creature(
        "basilisk",
        "Creature — Basilisk",
        "{0}",
        rules.clone(),
        "player-1",
    )];
    admit(&mut engine);
    declare_attack(&mut engine, &["attacker"]);
    declare_block(&mut engine, "attacker", "basilisk");
    resolve_stack(&mut engine);
    assert!(!on_battlefield(&engine, "attacker"));
    assert!(on_battlefield(&engine, "basilisk"));
    // As the attacker: each of its two blockers is destroyed (CR 509.3d).
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature(
        "basilisk",
        "Creature — Basilisk",
        "{0}",
        rules,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        vanilla("blocker-a", "player-1"),
        vanilla("blocker-b", "player-1"),
    ];
    admit(&mut engine);
    declare_attack(&mut engine, &["basilisk"]);
    declare_block(&mut engine, "basilisk", "blocker-a");
    declare_block(&mut engine, "basilisk", "blocker-b");
    assert_eq!(engine.state.stack.len(), 2);
    resolve_stack(&mut engine);
    assert!(!on_battlefield(&engine, "blocker-a"));
    assert!(!on_battlefield(&engine, "blocker-b"));
    assert!(on_battlefield(&engine, "basilisk"));
}

#[test]
fn a_new_object_starts_its_own_per_turn_ordinal() {
    let rules = parsed_rules(
        "Ordinal Probe",
        "Creature — Samurai",
        "Whenever this creature attacks for the first time each turn, draw a card.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature(
        "samurai",
        "Creature — Samurai",
        "{0}",
        rules,
        "player-0",
    )];
    admit(&mut engine);
    declare_attack(&mut engine, &["samurai"]);
    assert_eq!(engine.state.stack.len(), 1);
    resolve_stack(&mut engine);
    engine.state.combat.attackers.clear();
    declare_attack(&mut engine, &["samurai"]);
    assert!(engine.state.stack.is_empty(), "same object, second attack");
    // It left and returned this turn: its first attack as a new object.
    new_incarnation(&mut engine, "samurai");
    engine.state.combat.attackers.clear();
    declare_attack(&mut engine, &["samurai"]);
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn at_end_of_combat_delays_the_action_on_the_bound_combatant() {
    // Cockatrice: the delayed trigger destroys the creature bound when the
    // ability resolved, at the end of combat step (CR 511, 603.7).
    let rules = parsed_rules(
        "Cockatrice",
        "Creature — Cockatrice",
        "Whenever this creature blocks or becomes blocked by a non-Wall creature, destroy that creature at end of combat.",
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "installDelayedStepTrigger");
    assert_eq!(rules[0]["effects"][0]["step"], "endOfCombat");
    assert_eq!(
        rules[0]["effects"][0]["effects"][0]["permanent"],
        json!({"kind": "combatCounterpart"})
    );
    let setup = || {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![vanilla("attacker", "player-0")];
        engine.state.players[1].battlefield = vec![creature(
            "cockatrice",
            "Creature — Cockatrice",
            "{0}",
            rules.clone(),
            "player-1",
        )];
        admit(&mut engine);
        declare_attack(&mut engine, &["attacker"]);
        declare_block(&mut engine, "attacker", "cockatrice");
        resolve_stack(&mut engine);
        // Resolving creates the delayed trigger; nothing is destroyed yet.
        assert!(on_battlefield(&engine, "attacker"));
        engine
    };
    let mut engine = setup();
    engine.fire_delayed_step_triggers("endStep");
    assert!(
        engine.state.stack.is_empty(),
        "another step does not wake it"
    );
    engine.fire_delayed_step_triggers("endOfCombat");
    assert_eq!(engine.state.stack.len(), 1);
    resolve_stack(&mut engine);
    assert!(!on_battlefield(&engine, "attacker"));
    // The creature became a new object before end of combat (CR 400.7).
    let mut engine = setup();
    new_incarnation(&mut engine, "attacker");
    engine.fire_delayed_step_triggers("endOfCombat");
    resolve_stack(&mut engine);
    assert!(on_battlefield(&engine, "attacker"));
}

#[test]
fn one_attacking_creature_is_one_record_for_any_controller_relation() {
    // `Whenever a creature you control attacks`: one trigger per attacker.
    let yours = parsed_rules(
        "Attack Watcher",
        "Enchantment",
        "Whenever a creature you control attacks, you gain 1 life.",
    );
    assert_eq!(yours[0]["event"]["lemma"], "attack");
    assert_eq!(
        yours[0]["event"]["subject"]["object"]["controller"],
        json!({"kind": "controllerOf", "object": {"kind": "self"}})
    );
    // Sibling: the defender observes an opponent's attacker.
    let theirs = parsed_rules(
        "Defense Watcher",
        "Enchantment",
        "Whenever a creature an opponent controls attacks, you gain 1 life.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance(
            "watcher",
            CardDefinition {
                rules: yours,
                ..test_definition("watcher", "Enchantment")
            },
            "player-0",
        ),
        vanilla("a", "player-0"),
        vanilla("b", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![test_instance(
        "defender",
        CardDefinition {
            rules: theirs,
            ..test_definition("defender", "Enchantment")
        },
        "player-1",
    )];
    declare_attack(&mut engine, &["a", "b"]);
    let mut sources = engine
        .state
        .stack
        .iter()
        .map(|object| object.card.instance_id.clone())
        .collect::<Vec<_>>();
    sources.sort();
    assert_eq!(sources, ["defender", "defender", "watcher", "watcher"]);
    for object in &engine.state.stack {
        assert_eq!(object.decisions["triggeringPlayerId"], "player-1");
    }
}

#[test]
fn one_or_more_blocked_creatures_trigger_once_per_block_declaration() {
    let rules = parsed_rules(
        "Blocked Watcher",
        "Enchantment",
        "Whenever one or more creatures you control become blocked, draw a card.",
    );
    assert_eq!(rules[0]["event"]["aggregation"], "oneOrMore");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance(
            "watcher",
            CardDefinition {
                rules,
                ..test_definition("watcher", "Enchantment")
            },
            "player-0",
        ),
        vanilla("a", "player-0"),
        vanilla("b", "player-0"),
    ];
    engine.state.players[1].battlefield =
        vec![vanilla("wall-a", "player-1"), vanilla("wall-b", "player-1")];
    declare_attack(&mut engine, &["a", "b"]);
    // Both blocks belong to one simultaneous declaration (CR 509.1).
    let scope = engine.begin_event_batch();
    declare_block(&mut engine, "a", "wall-a");
    declare_block(&mut engine, "b", "wall-b");
    engine.end_event_batch(scope);
    assert_eq!(engine.state.stack.len(), 1);
}
