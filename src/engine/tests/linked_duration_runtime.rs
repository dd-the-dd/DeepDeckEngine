//! Real Oracle text -> primitive IR -> engine behavior for one-shot `until`
//! zone changes (CR 610.3) and delayed step triggers created by an effect
//! program (CR 603.7).
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
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

fn parsed_permanent(instance_id: &str, name: &str, type_line: &str, oracle: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.rules = parsed_rules(name, type_line, oracle);
    test_instance(instance_id, definition, "player-0")
}

fn creature(instance_id: &str, owner: &str) -> CardInstance {
    test_instance(
        instance_id,
        test_definition(instance_id, "Creature — Bear"),
        owner,
    )
}

const SILKWRAP: &str = "When this enchantment enters, exile target creature with mana value 3 or less an opponent controls until this enchantment leaves the battlefield. (That creature returns under its owner's control.)";
const FLICKERWISP: &str = "Flying\nWhen this creature enters, exile another target permanent. Return that card to the battlefield under its owner's control at the beginning of the next end step.";

/// Puts `source` onto the battlefield with `others` and publishes its real
/// entry trigger.
fn enter_with_trigger(
    engine: &mut GameEngine,
    source: CardInstance,
    others: Vec<(usize, CardInstance)>,
) {
    engine.state.players[0].battlefield.push(source.clone());
    for (player, card) in others {
        engine.state.players[player].battlefield.push(card);
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let (player, index) = engine
        .permanent_position(&source.instance_id)
        .expect("source entered");
    let live = engine.state.players[player].battlefield[index].clone();
    engine.enqueue_enter_battlefield_triggers(&live);
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the entry trigger is on the stack"
    );
}

fn in_exile(engine: &GameEngine, instance_id: &str) -> bool {
    engine.state.players.iter().any(|player| {
        player
            .exile
            .iter()
            .any(|card| card.instance_id == instance_id)
    })
}

fn has_delayed_trigger(engine: &GameEngine) -> bool {
    engine
        .state
        .rule_modifiers
        .iter()
        .any(|modifier| value_kind(modifier) == Some("delayedStepTrigger"))
}

#[test]
fn parsed_until_source_leaves_returns_the_exiled_creature_under_its_owner() {
    let mut engine = test_engine(2);
    let source = parsed_permanent("silkwrap", "Silkwrap", "Enchantment", SILKWRAP);
    enter_with_trigger(
        &mut engine,
        source,
        vec![(1, creature("captive", "player-1"))],
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
    assert!(engine.permanent_position("captive").is_none());
    assert!(in_exile(&engine, "captive"));

    engine
        .move_permanent_to_graveyard("silkwrap", "destroyed")
        .expect("the source leaves the battlefield");
    engine
        .check_state_based_actions_with_provider(Some(&mut EmeritusDecisionProvider))
        .expect("the linked return completes");
    let (player, index) = engine
        .permanent_position("captive")
        .expect("the exiled creature returned");
    assert_eq!(player, 1);
    assert_eq!(
        engine.state.players[1].battlefield[index].controller,
        "player-1"
    );
    assert!(!in_exile(&engine, "captive"));
}

/// CR 610.3b: the source left after the ability triggered, so the object
/// doesn't move at all (no exile followed by an immediate return).
#[test]
fn parsed_until_source_leaves_does_nothing_when_the_source_already_left() {
    let mut engine = test_engine(2);
    let source = parsed_permanent("silkwrap", "Silkwrap", "Enchantment", SILKWRAP);
    enter_with_trigger(
        &mut engine,
        source,
        vec![(1, creature("captive", "player-1"))],
    );
    engine
        .move_permanent_to_graveyard("silkwrap", "destroyed")
        .expect("the source leaves before resolution");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
    assert!(engine.permanent_position("captive").is_some());
    assert!(!in_exile(&engine, "captive"));
    assert!(!engine.state.events.iter().any(|event| {
        event.kind == "cardExiled" && event.card_instance_id.as_deref() == Some("captive")
    }));
}

#[test]
fn parsed_flicker_returns_the_exiled_card_at_the_next_end_step() {
    let mut engine = test_engine(2);
    let source = parsed_permanent(
        "flickerwisp",
        "Flickerwisp",
        "Creature — Elemental",
        FLICKERWISP,
    );
    enter_with_trigger(
        &mut engine,
        source,
        vec![(1, creature("captive", "player-1"))],
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
    assert!(in_exile(&engine, "captive"));
    assert!(has_delayed_trigger(&engine));

    engine.enqueue_step_triggers("upkeep");
    assert!(engine.state.stack.is_empty(), "only the end step wakes it");
    engine.enqueue_step_triggers("endStep");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the delayed return resolves");
    let (player, _) = engine
        .permanent_position("captive")
        .expect("the exiled card returned");
    assert_eq!(player, 1);
    assert!(
        !has_delayed_trigger(&engine),
        "a delayed trigger without duration triggers only once (CR 603.7b)"
    );
}

/// Unused combination: preposed timing, the controller's next upkeep, and a
/// spell's own exile result.
#[test]
fn parsed_controller_upkeep_delay_waits_for_its_controllers_turn() {
    let rules = parsed_rules(
        "Test Slow Return",
        "Sorcery",
        "Exile target creature. At the beginning of your next upkeep, return that card to the battlefield under its owner's control.",
    );
    let rule = &rules[0];
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a target id")
        .to_string();
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![creature("captive", "player-1")];
    let stack = StackObject {
        id: "stack:slow-return".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "slow-return",
            test_definition("slow-return", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "captive".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    assert!(in_exile(&engine, "captive"));

    engine.state.active_player = 1;
    engine.enqueue_step_triggers("upkeep");
    assert!(
        engine.state.stack.is_empty(),
        "an opponent's upkeep is not the controller's upkeep"
    );
    engine.state.active_player = 0;
    engine.enqueue_step_triggers("upkeep");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the delayed return resolves");
    assert!(engine.permanent_position("captive").is_some());
}

const SILUMGAR: &str = "Flying, deathtouch\nWhen Dragonlord Silumgar enters, gain control of target creature or planeswalker for as long as you control Dragonlord Silumgar.";

fn controller_of(engine: &GameEngine, instance_id: &str) -> String {
    let (player, index) = engine
        .permanent_position(instance_id)
        .expect("permanent on the battlefield");
    engine.state.players[player].battlefield[index]
        .controller
        .clone()
}

/// CR 611.2b: control lasts while the controller controls that source.
#[test]
fn parsed_control_for_as_long_as_you_control_the_source_ends_when_it_leaves() {
    let mut engine = test_engine(2);
    let source = parsed_permanent(
        "silumgar",
        "Dragonlord Silumgar",
        "Legendary Creature — Elder Dragon",
        SILUMGAR,
    );
    enter_with_trigger(
        &mut engine,
        source,
        vec![(1, creature("stolen", "player-1"))],
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
    assert_eq!(controller_of(&engine, "stolen"), "player-0");

    engine
        .move_permanent_to_graveyard("silumgar", "destroyed")
        .expect("the source leaves the battlefield");
    engine
        .check_state_based_actions_with_provider(Some(&mut EmeritusDecisionProvider))
        .expect("state-based checks complete");
    assert_eq!(controller_of(&engine, "stolen"), "player-1");
}

/// CR 611.2b: a duration that never starts does nothing.
#[test]
fn parsed_control_duration_that_never_starts_changes_nothing() {
    let mut engine = test_engine(2);
    let source = parsed_permanent(
        "silumgar",
        "Dragonlord Silumgar",
        "Legendary Creature — Elder Dragon",
        SILUMGAR,
    );
    enter_with_trigger(
        &mut engine,
        source,
        vec![(1, creature("stolen", "player-1"))],
    );
    engine
        .move_permanent_to_graveyard("silumgar", "destroyed")
        .expect("the source leaves before resolution");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
    assert_eq!(controller_of(&engine, "stolen"), "player-1");
}

#[test]
fn parsed_act_of_treason_takes_untaps_and_hastes_until_end_of_turn() {
    let rules = parsed_rules(
        "Act of Treason",
        "Sorcery",
        "Gain control of target creature until end of turn. Untap that creature. It gains haste until end of turn. (It can attack and {T} this turn.)",
    );
    let rule = &rules[0];
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a target id")
        .to_string();
    let mut engine = test_engine(2);
    let mut stolen = creature("stolen", "player-1");
    stolen.tapped = true;
    engine.state.players[1].battlefield = vec![stolen];
    let stack = StackObject {
        id: "stack:treason".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("treason", test_definition("treason", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "stolen".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    assert_eq!(controller_of(&engine, "stolen"), "player-0");
    let (player, index) = engine.permanent_position("stolen").expect("on battlefield");
    let stolen = engine.state.players[player].battlefield[index].clone();
    assert!(!stolen.tapped);
    assert!(engine.permanent_has_keyword(&stolen, "haste"));
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("controlUntilEndOfTurn")
            && modifier["originalController"] == "player-1"
    }));
}
