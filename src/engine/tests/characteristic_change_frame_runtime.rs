//! Oracle text -> canonical IR -> runtime for the characteristic-change
//! frame reached from new positions: the imperative `switch` (layer 7d, CR
//! 613.4d), the base power alone (layer 7b, CR 613.4b), a copy's stated name
//! (CR 707.9b) and the static copula complement (CR 205.1b, 613.1).
use super::*;

fn parsed_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    let rules = parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.clone().expect("canonical rule"))
        .collect::<Vec<_>>();
    for rule in &rules {
        assert!(rule_is_executable(rule), "{text}: {rule:#}");
    }
    rules
}

fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    parsed_rules(name, type_line, text)
        .pop()
        .expect("one canonical rule")
}

fn place(engine: &mut GameEngine, id: &str, type_line: &str, stats: Option<(i32, i32)>) {
    let mut definition = test_definition(id, type_line);
    if let Some((power, toughness)) = stats {
        definition.power = Some(power.to_string());
        definition.toughness = Some(toughness.to_string());
    }
    engine.state.players[0]
        .battlefield
        .push(test_instance(id, definition, "player-0"));
}

fn live(engine: &GameEngine, id: &str) -> CardInstance {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|permanent| permanent.instance_id == id)
        .cloned()
        .expect("permanent on the battlefield")
}

fn stats(engine: &GameEngine, id: &str) -> (i32, i32) {
    let permanent = live(engine, id);
    (
        engine.current_power(&permanent),
        engine.current_toughness(&permanent),
    )
}

/// Resolves `rule` with `source` as its source; every listed target id of
/// the rule's effects names `target`.
fn resolve(engine: &mut GameEngine, rule: &Value, source: &str, target_ids: &[(&str, &str)]) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let source = live(engine, source);
    let stack = StackObject {
        id: "stack:characteristic-change".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(rule["kind"].as_str().unwrap_or("spellAbility").to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: target_ids
            .iter()
            .map(|(id, target)| {
                (
                    id.to_string(),
                    TargetRef::Permanent {
                        instance_id: target.to_string(),
                    },
                )
            })
            .collect(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("characteristic change resolves");
}

/// Real card (Strange Inversion): the switch applies in layer 7d, after a
/// layer-7c +2/+0, whatever their order, and ends with the turn.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn switch_applies_after_every_other_power_toughness_effect_until_cleanup() {
    let rule = parsed_rule(
        "Strange Inversion",
        "Instant - Arcane",
        "Switch target creature's power and toughness until end of turn.",
    );
    assert_eq!(rule["effects"][0]["kind"], "switchPowerToughness");
    let target_id = rule["effects"][0]["object"]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let mut engine = test_engine(2);
    place(&mut engine, "inversion", "Instant - Arcane", None);
    place(&mut engine, "bear", "Creature - Bear", Some((1, 4)));
    engine.state.players[0].battlefield[1].power_modifier = 2;
    assert_eq!(stats(&engine, "bear"), (3, 4));

    resolve(&mut engine, &rule, "inversion", &[(&target_id, "bear")]);
    assert_eq!(stats(&engine, "bear"), (4, 3), "layer 7d after the +2/+0");

    engine.state.turn_number += 1;
    assert_eq!(
        stats(&engine, "bear"),
        (3, 4),
        "the switch ended with the turn"
    );
}

/// Real card (Windreaver): the ability's own creature; two switches restore
/// the original order (CR 613.4d applies each switch in turn).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn two_switches_of_the_source_cancel() {
    let rule = parsed_rule(
        "Windreaver",
        "Creature - Elemental",
        "{U}: Switch this creature's power and toughness until end of turn.",
    );
    assert_eq!(rule["effects"][0]["object"]["kind"], "self");
    let mut engine = test_engine(2);
    place(
        &mut engine,
        "windreaver",
        "Creature - Elemental",
        Some((1, 3)),
    );
    resolve(&mut engine, &rule, "windreaver", &[]);
    assert_eq!(stats(&engine, "windreaver"), (3, 1));
    resolve(&mut engine, &rule, "windreaver", &[]);
    assert_eq!(stats(&engine, "windreaver"), (1, 3));
}

/// Real card (Singing Tree): the base power alone is set in layer 7b; the
/// toughness keeps its value and counters still apply on top (CR 613.4c).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn base_power_alone_keeps_the_toughness() {
    let rule = parsed_rule(
        "Singing Tree",
        "Creature - Plant",
        "{T}: Target attacking creature has base power 0 until end of turn.",
    );
    assert_eq!(rule["effects"][0]["kind"], "setPower");
    let target_id = rule["effects"][0]["object"]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let mut engine = test_engine(2);
    place(&mut engine, "tree", "Creature - Plant", Some((0, 2)));
    place(&mut engine, "ogre", "Creature - Ogre", Some((4, 3)));
    resolve(&mut engine, &rule, "tree", &[(&target_id, "ogre")]);
    assert_eq!(stats(&engine, "ogre"), (0, 3));
}

/// Real card (Impossible Man): the copy keeps the stated name (CR 707.9b).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn copy_exception_states_the_copy_name() {
    let rule = parsed_rule(
        "Impossible Man",
        "Legendary Creature - Human Hero",
        "{2}{U}: Impossible Man becomes a copy of another target permanent until end of turn, except his name is Impossible Man.",
    );
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "becomeCopyOfPermanent");
    assert_eq!(
        effect["except"],
        json!([{ "kind": "setName", "name": "Impossible Man" }])
    );
    let copied_id = effect["copy"]["id"]
        .as_str()
        .expect("a declared copied target")
        .to_string();
    let mut engine = test_engine(2);
    place(
        &mut engine,
        "impossible",
        "Legendary Creature - Human Hero",
        Some((2, 2)),
    );
    place(&mut engine, "giant", "Creature - Giant", Some((5, 5)));
    resolve(&mut engine, &rule, "impossible", &[(&copied_id, "giant")]);
    let copy = live(&engine, "impossible");
    assert_eq!(copy.definition.name, "Impossible Man");
    assert_eq!(stats(&engine, "impossible"), (5, 5));
}

/// Real card (Living Lands): `1/1 creatures that are still lands` adds the
/// creature type and sets the base power and toughness of each Forest.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn static_copula_animates_lands_that_are_still_lands() {
    let rule = parsed_rule(
        "Living Lands",
        "Enchantment",
        "All Forests are 1/1 creatures that are still lands.",
    );
    let mut engine = test_engine(2);
    let mut enchantment = test_definition("living-lands", "Enchantment");
    enchantment.rules = vec![rule];
    engine.state.players[0].battlefield.push(test_instance(
        "living-lands",
        enchantment,
        "player-0",
    ));
    place(&mut engine, "forest", "Basic Land - Forest", None);
    place(&mut engine, "plains", "Basic Land - Plains", None);
    let forest = live(&engine, "forest");
    assert!(engine.permanent_has_card_type(&forest, "Creature"));
    assert!(engine.permanent_has_card_type(&forest, "Land"));
    assert_eq!(stats(&engine, "forest"), (1, 1));
    let plains = live(&engine, "plains");
    assert!(!engine.permanent_has_card_type(&plains, "Creature"));
}
