//! Real Oracle text -> primitive IR -> engine behavior for effects bounded
//! by a watched source state (CR 611.2b): `for as long as this artifact
//! remains tapped`, `for as long as you control this creature`. The
//! duration is the ordinary duration of the power/toughness change or the
//! keyword grant; `engine/condition_durations.rs` ends it.
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

fn permanent(id: &str, type_line: &str, owner: &str, rules: Vec<Value>) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    let mut card = test_instance(id, definition, owner);
    card.summoning_sick = false;
    card
}

fn targeted_stack(card: &CardInstance, rule: &Value, target: &str) -> StackObject {
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let mut targets = BTreeMap::new();
    targets.insert(
        target_id,
        TargetRef::Permanent {
            instance_id: target.to_string(),
        },
    );
    StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: card.controller.clone(),
        card: card.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
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
    .expect("the effect resolves");
}

fn set_tapped(engine: &mut GameEngine, instance_id: &str, tapped: bool) {
    let (player, index) = engine
        .permanent_position(instance_id)
        .expect("the permanent is on the battlefield");
    engine.state.players[player].battlefield[index].tapped = tapped;
}

fn power_toughness(engine: &GameEngine, instance_id: &str) -> (i32, i32) {
    let statistics = GameEngine::effective_power_toughness(&engine.state);
    let entry = statistics
        .get(instance_id)
        .expect("the creature has effective statistics");
    (entry.current_power, entry.current_toughness)
}

fn has_keyword(engine: &GameEngine, instance_id: &str, keyword: &str) -> bool {
    let (player, index) = engine
        .permanent_position(instance_id)
        .expect("the permanent is on the battlefield");
    let permanent = engine.state.players[player].battlefield[index].clone();
    engine.permanent_has_keyword(&permanent, keyword)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_stat_change_lasts_while_its_source_remains_tapped() {
    // Flowstone Armor (CR 611.2b): the change ends when the artifact
    // untaps, and tapping it again does not restore an ended effect.
    let rules = parsed_rules(
        "Flowstone Armor",
        "Artifact",
        "{3}, {T}: Target creature gets +1/-1 for as long as this artifact remains tapped.",
    );
    let activated = rules[0].clone();
    assert_eq!(
        activated["effects"][0]["duration"]["kind"], "whileSourceTapped",
        "{activated:#}"
    );
    let mut engine = test_engine(2);
    let armor = permanent("armor", "Artifact", "player-0", rules);
    engine.state.players[0].battlefield = vec![
        armor.clone(),
        permanent("bear", "Creature - Bear", "player-0", Vec::new()),
    ];
    set_tapped(&mut engine, "armor", true);
    let stack = targeted_stack(&armor, &activated, "bear");
    resolve(&mut engine, &activated, &stack);
    assert_eq!(power_toughness(&engine, "bear"), (3, 1));
    set_tapped(&mut engine, "armor", false);
    assert_eq!(
        power_toughness(&engine, "bear"),
        (2, 2),
        "the change ends as the source untaps"
    );
    engine.refresh_control_effects();
    set_tapped(&mut engine, "armor", true);
    assert_eq!(
        power_toughness(&engine, "bear"),
        (2, 2),
        "an ended effect does not resume"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_watched_effect_never_starts_when_its_state_does_not_hold() {
    // CR 611.2b: the source untapped before resolution, so the change
    // does nothing.
    let rules = parsed_rules(
        "Spirit Shield",
        "Artifact",
        "{2}, {T}: Target creature gets +0/+2 for as long as this artifact remains tapped.",
    );
    let activated = rules[0].clone();
    let mut engine = test_engine(2);
    let shield = permanent("shield", "Artifact", "player-0", rules);
    engine.state.players[0].battlefield = vec![
        shield.clone(),
        permanent("bear", "Creature - Bear", "player-0", Vec::new()),
    ];
    let stack = targeted_stack(&shield, &activated, "bear");
    resolve(&mut engine, &activated, &stack);
    assert_eq!(power_toughness(&engine, "bear"), (2, 2));
    assert!(
        !engine
            .state
            .rule_modifiers
            .iter()
            .any(|modifier| modifier.get("watch").is_some()),
        "no watched modifier was created"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_keyword_grant_lasts_while_its_source_is_controlled() {
    // Unused combination of supported concepts: an activated keyword
    // grant bounded by control of its source (CR 611.2b, 613.1f).
    let rules = parsed_rules(
        "Probe Warden",
        "Creature \u{2014} Human Soldier",
        "{T}: Target creature gains flying for as long as you control this creature.",
    );
    let activated = rules[0].clone();
    let mut engine = test_engine(2);
    let warden = permanent("warden", "Creature - Human Soldier", "player-0", rules);
    engine.state.players[0].battlefield = vec![
        warden.clone(),
        permanent("bear", "Creature - Bear", "player-0", Vec::new()),
    ];
    let stack = targeted_stack(&warden, &activated, "bear");
    resolve(&mut engine, &activated, &stack);
    assert!(has_keyword(&engine, "bear", "flying"));
    let (player, index) = engine.permanent_position("warden").expect("warden");
    engine.state.players[player].battlefield[index].controller = "player-1".to_string();
    assert!(
        !has_keyword(&engine, "bear", "flying"),
        "the grant ends when control of the source changes"
    );
    engine.refresh_control_effects();
    let (player, index) = engine.permanent_position("warden").expect("warden");
    engine.state.players[player].battlefield[index].controller = "player-0".to_string();
    assert!(!has_keyword(&engine, "bear", "flying"));
}
