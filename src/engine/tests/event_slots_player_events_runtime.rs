//! Real Oracle text -> player event slot IR -> engine behavior: a discard
//! batch of one instruction (CR 603.2c), the causing spell or ability of a
//! self discard (CR 701.9), a player as the recipient of damage (CR 120.3),
//! the source object of an activated ability (CR 602.2) and a player
//! attacking a written recipient (CR 508.1).
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
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn parsed_card(
    instance_id: &str,
    owner: &str,
    name: &str,
    type_line: &str,
    oracle: &str,
) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    if type_line.contains("Creature") {
        definition.power = Some("1".to_string());
        definition.toughness = Some("1".to_string());
    }
    definition.rules = parsed_rules(name, type_line, oracle);
    test_instance(instance_id, definition, owner)
}

fn plain_card(instance_id: &str, owner: &str, type_line: &str) -> CardInstance {
    test_instance(instance_id, test_definition(instance_id, type_line), owner)
}

fn triggers_from(engine: &GameEngine, instance_id: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| {
            event.kind == "triggeredAbilityPutOnStack"
                && event.card_instance_id.as_deref() == Some(instance_id)
        })
        .count()
}

fn resolve_stack(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the stack resolves");
    }
}

/// A resolving spell of `controller` whose effects are `oracle`.
fn resolve_spell_effects(engine: &mut GameEngine, controller: &str, oracle: &str) {
    let spell = parsed_rules("Effect Source", "Sorcery", oracle)
        .into_iter()
        .next()
        .expect("the spell compiles");
    let source = StackObject {
        id: format!("stack:{controller}:effect"),
        controller: controller.to_string(),
        card: plain_card(&format!("{controller}-effect"), controller, "Sorcery"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        engine,
        &spell["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell's effects resolve");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn one_discard_instruction_is_one_batch_with_its_card_count() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        // Marauding Mako's batch observer.
        parsed_card(
            "mako",
            "player-0",
            "Marauding Mako",
            "Creature — Shark Pirate",
            "Whenever you discard one or more cards, put that many +1/+1 counters on this creature.",
        ),
        // The per-card observer of the same discards.
        parsed_card(
            "per-card",
            "player-0",
            "Per Card Probe",
            "Enchantment",
            "Whenever you discard a card, you gain 1 life.",
        ),
        // A batch filter no discarded card satisfies.
        parsed_card(
            "land-batch",
            "player-0",
            "Land Batch Probe",
            "Enchantment",
            "Whenever you discard one or more land cards, you gain 5 life.",
        ),
    ];
    engine.state.players[0].hand = vec![
        plain_card("bolt", "player-0", "Instant"),
        plain_card("bear", "player-0", "Creature — Bear"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    resolve_spell_effects(&mut engine, "player-0", "Discard two cards.");
    assert_eq!(triggers_from(&engine, "per-card"), 2, "one per card");
    assert_eq!(triggers_from(&engine, "mako"), 1, "one per discard batch");
    assert_eq!(triggers_from(&engine, "land-batch"), 0, "no land discarded");
    resolve_stack(&mut engine);
    let mako = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "mako")
        .expect("Mako stays");
    assert_eq!(mako.counters.get("+1/+1"), Some(&2), "that many: two cards");
    assert_eq!(engine.state.players[0].life, 22);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_cost_discard_is_its_own_discard_batch() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![parsed_card(
        "batch",
        "player-0",
        "Batch Probe",
        "Enchantment",
        "Whenever you discard one or more creature cards, you gain 1 life.",
    )];
    engine.state.players[0].hand = vec![
        plain_card("bear", "player-0", "Creature — Bear"),
        plain_card("elk", "player-0", "Creature — Elk"),
    ];
    engine.state.players[1].hand = vec![plain_card("their-bear", "player-1", "Creature — Bear")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .discard_card_as_cost("player-0", "bear")
        .expect("the first cost discard");
    engine
        .discard_card_as_cost("player-0", "elk")
        .expect("the second cost discard");
    engine
        .discard_card_as_cost("player-1", "their-bear")
        .expect("the opponent's cost discard");
    assert_eq!(
        triggers_from(&engine, "batch"),
        2,
        "each cost is one discard action; the opponent's is not observed"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_caused_self_discard_triggers_only_for_an_opponents_spell() {
    const PURGE: &str = "Psychic Purge deals 1 damage to any target.\nWhen a spell or ability an opponent controls causes you to discard this card, that player loses 5 life.";
    for (discarding_spell_controller, effect, triggers) in [
        ("player-1", "Each opponent discards a card.", 1),
        ("player-0", "Discard a card.", 0),
    ] {
        let mut engine = test_engine(2);
        engine.state.players[0].hand = vec![parsed_card(
            "purge",
            "player-0",
            "Psychic Purge",
            "Sorcery",
            PURGE,
        )];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        resolve_spell_effects(&mut engine, discarding_spell_controller, effect);
        assert!(
            engine.state.players[0]
                .graveyard
                .iter()
                .any(|card| card.instance_id == "purge"),
            "{effect}: Psychic Purge was discarded"
        );
        assert_eq!(triggers_from(&engine, "purge"), triggers, "{effect}");
        resolve_stack(&mut engine);
        assert_eq!(
            engine.state.players[1].life,
            20 - 5 * triggers as i32,
            "{effect}: that player is the opponent who controlled the cause"
        );
        assert_eq!(engine.state.players[0].life, 20);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_player_dealt_damage_is_observed_through_the_written_relation() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        parsed_card(
            "droplet",
            "player-0",
            "Sun Droplet",
            "Artifact",
            "Whenever you're dealt damage, put that many charge counters on this artifact.",
        ),
        // Unused combination: the damage as subject, combat damage only.
        parsed_card(
            "combat-ward",
            "player-0",
            "Combat Ward",
            "Enchantment",
            "Whenever combat damage is dealt to you, you gain 3 life.",
        ),
        plain_card("source", "player-0", "Creature — Bear"),
    ];
    engine.state.players[1].battlefield =
        vec![plain_card("their-source", "player-1", "Creature — Bear")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for (target, combat) in [("player-1", false), ("player-0", false)] {
        engine
            .deal_damage_amount_with_kind_and_origin(
                TargetRef::Player {
                    player_id: target.to_string(),
                },
                2,
                "their-source",
                combat,
                QuantityOrigin::RuleAction,
                None,
            )
            .expect("damage is dealt");
    }
    assert_eq!(
        triggers_from(&engine, "droplet"),
        1,
        "only damage dealt to you"
    );
    assert_eq!(triggers_from(&engine, "combat-ward"), 0, "noncombat damage");
    resolve_stack(&mut engine);
    let droplet = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "droplet")
        .expect("the artifact stays");
    assert_eq!(droplet.counters.get("charge"), Some(&2), "that many");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_activation_is_observed_by_its_source_object_and_binds_the_activating_player() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![parsed_card(
        "shaman",
        "player-0",
        "Immolation Shaman",
        "Creature — Lizard Shaman",
        "Whenever an opponent activates an ability of an artifact, creature, or land that isn't a mana ability, this creature deals 1 damage to that player.",
    )];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for (stack_id, controller, source_type, triggers) in [
        ("stack:their-artifact", "player-1", "Artifact", 1),
        ("stack:their-enchantment", "player-1", "Enchantment", 1),
        ("stack:own-artifact", "player-0", "Artifact", 1),
    ] {
        engine.state.stack.push(StackObject {
            id: stack_id.to_string(),
            controller: controller.to_string(),
            card: plain_card(&format!("{stack_id}-source"), controller, source_type),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("activatedAbility".to_string()),
            ability_rule: Some(json!({"kind": "activatedAbility", "costs": []})),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        });
        engine.enqueue_nonmana_ability_activated_triggers(controller, stack_id);
        assert_eq!(
            triggers_from(&engine, "shaman"),
            triggers,
            "{stack_id}: only an opponent's artifact, creature or land ability"
        );
    }
    let trigger = engine
        .state
        .stack
        .iter()
        .find(|object| object.card.instance_id == "shaman")
        .expect("the trigger waits on the stack");
    assert_eq!(
        trigger.decisions.get("triggeringPlayerId"),
        Some(&json!("player-1")),
        "that player is the activating player"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_opponent_attacking_you_is_one_event_of_that_declaration() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        parsed_card(
            "ward",
            "player-0",
            "Attack Ward",
            "Enchantment",
            "Whenever an opponent attacks you, you gain 1 life.",
        ),
        plain_card("own-attacker", "player-0", "Creature — Bear"),
    ];
    engine.state.players[1].battlefield = vec![
        plain_card("their-a", "player-1", "Creature — Bear"),
        plain_card("their-b", "player-1", "Creature — Bear"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let declare = |engine: &mut GameEngine, attackers: &[&str], defender: &str| {
        engine.state.combat.attackers = attackers
            .iter()
            .map(|attacker| DeclaredAttacker {
                attacker_id: attacker.to_string(),
                defender: TargetRef::Player {
                    player_id: defender.to_string(),
                },
                defending_player_id: defender.to_string(),
            })
            .collect();
        let ids = attackers
            .iter()
            .map(|attacker| attacker.to_string())
            .collect::<Vec<_>>();
        engine.publish_attackers_attacked_player(&ids);
    };
    declare(&mut engine, &["own-attacker"], "player-1");
    assert_eq!(triggers_from(&engine, "ward"), 0, "you attacked");
    declare(&mut engine, &["their-a", "their-b"], "player-0");
    assert_eq!(triggers_from(&engine, "ward"), 1, "one declaration");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_spell_you_control_countering_a_spell_is_observed_by_its_controller() {
    let counter_rule = parsed_rules("Test Counterspell", "Instant", "Counter target spell.")
        .into_iter()
        .next()
        .expect("the counterspell compiles");
    for (countering_player, triggers) in [("player-0", 1), ("player-1", 0)] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![parsed_card(
            "mentor",
            "player-0",
            "Lullmage Mentor",
            "Creature — Merfolk Wizard",
            "Whenever a spell or ability you control counters a spell, you gain 2 life.",
        )];
        let target_owner = if countering_player == "player-0" {
            "player-1"
        } else {
            "player-0"
        };
        engine.state.stack.push(StackObject {
            id: "stack:target".to_string(),
            controller: target_owner.to_string(),
            card: plain_card("target-spell", target_owner, "Instant"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: None,
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        });
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let counter = StackObject {
            id: "stack:counter".to_string(),
            controller: countering_player.to_string(),
            card: plain_card("counter", countering_player, "Instant"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("spellAbility".to_string()),
            ability_rule: Some(counter_rule.clone()),
            decisions: BTreeMap::new(),
            // `Counter target spell.` declares its stack target `targetSpell`.
            targets: BTreeMap::from([(
                "targetSpell".to_string(),
                TargetRef::StackObject {
                    stack_id: "stack:target".to_string(),
                },
            )]),
        };
        engine
            .execute_effects(
                &counter_rule["effects"],
                &counter,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut EmeritusDecisionProvider,
            )
            .expect("the counterspell resolves");
        assert!(
            engine
                .state
                .events
                .iter()
                .any(|event| event.kind == "spellCountered"),
            "{countering_player}: the spell was countered"
        );
        assert_eq!(
            triggers_from(&engine, "mentor"),
            triggers,
            "{countering_player}: only your spell or ability"
        );
    }
}
