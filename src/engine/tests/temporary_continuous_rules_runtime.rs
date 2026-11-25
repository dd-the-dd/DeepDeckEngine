//! Real Oracle text -> primitive IR -> engine behavior for the rules a
//! resolving effect creates (CR 611.2a), the global rule statics of player
//! and permanent populations, `for as long as` source-state windows
//! (CR 611.2b) and the bestow attachment subject (CR 702.103b).
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
        .flat_map(|rule| crate::oracle::keyword_expansion_rules(&rule))
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

fn stack_object(card: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: card.controller.clone(),
        card: card.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
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

fn tapped(engine: &GameEngine, instance_id: &str) -> bool {
    let (player, index) = engine
        .permanent_position(instance_id)
        .expect("the permanent is on the battlefield");
    engine.state.players[player].battlefield[index].tapped
}

fn power_toughness(engine: &GameEngine, instance_id: &str) -> (i32, i32) {
    let statistics = GameEngine::effective_power_toughness(&engine.state);
    let entry = statistics
        .get(instance_id)
        .expect("the creature has effective statistics");
    (entry.current_power, entry.current_toughness)
}

const NIMBUS_NAIAD: &str = "Bestow {4}{U} (If you cast this card for its bestow cost, it's an Aura spell with enchant creature. It becomes a creature again if it's not attached.)\nFlying\nEnchanted creature gets +2/+2 and has flying.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_bestow_static_applies_only_while_attached_as_an_aura() {
    // CR 702.103b: `enchanted creature` names the creature the bestowed
    // Aura enchants; an unattached bestow creature affects nothing.
    let rules = parsed_rules(
        "Nimbus Naiad",
        "Enchantment Creature \u{2014} Nymph",
        NIMBUS_NAIAD,
    );
    let mut engine = test_engine(2);
    let naiad = permanent("naiad", "Enchantment Creature - Nymph", "player-0", rules);
    let bear = permanent("bear", "Creature - Bear", "player-0", Vec::new());
    engine.state.players[0].battlefield = vec![naiad, bear];
    let unattached = power_toughness(&engine, "bear");
    assert_eq!(unattached, (2, 2), "an unattached bestow creature is inert");
    let (player, index) = engine.permanent_position("naiad").expect("naiad");
    engine.state.players[player].battlefield[index].attached_to = Some("bear".to_string());
    assert_eq!(power_toughness(&engine, "bear"), (4, 4));
    let bear = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "bear")
        .cloned()
        .expect("bear");
    assert!(engine.permanent_has_keyword(&bear, "flying"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_named_players_next_untap_step_is_the_only_one_it_restricts() {
    // Kefnet's Last Word's second sentence: `your next untap step` names
    // the controller; another player's untap step neither applies nor
    // consumes the restriction (CR 502.3).
    let rule = parsed_rules(
        "Untap Window Probe",
        "Sorcery",
        "Lands you control don't untap during your next untap step.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let mut forest = permanent("forest", "Basic Land - Forest", "player-0", Vec::new());
    forest.tapped = true;
    let mut swamp = permanent("swamp", "Basic Land - Swamp", "player-1", Vec::new());
    swamp.tapped = true;
    engine.state.players[0].battlefield = vec![forest];
    engine.state.players[1].battlefield = vec![swamp];
    let source = test_instance("probe", test_definition("probe", "Sorcery"), "player-0");
    resolve(&mut engine, &rule, &stack_object(&source, &rule));
    engine.state.active_player = 1;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the opponent's untap step");
    assert!(
        !tapped(&engine, "swamp"),
        "the opponent's land is not restricted"
    );
    assert!(tapped(&engine, "forest"));
    engine.state.active_player = 0;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the controller's untap step");
    assert!(tapped(&engine, "forest"), "the restricted untap step");
    engine.state.active_player = 0;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the controller's following untap step");
    assert!(!tapped(&engine, "forest"), "the restriction was consumed");
}

const RUST_TICK: &str = "You may choose not to untap this creature during your untap step.\n{1}, {T}: Tap target artifact. It doesn't untap during its controller's untap step for as long as this creature remains tapped.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_remains_tapped_window_ends_for_good_when_its_source_untaps() {
    // CR 611.2b: the restriction lasts while the same source stays tapped;
    // once the source untaps it ends and does not resume when retapped.
    let rules = parsed_rules("Rust Tick", "Artifact Creature \u{2014} Insect", RUST_TICK);
    let activated = rules
        .iter()
        .find(|rule| rule["kind"] == "activatedAbility")
        .cloned()
        .expect("the activated ability");
    let mut engine = test_engine(2);
    let mut tick = permanent("tick", "Artifact Creature - Insect", "player-0", rules);
    tick.tapped = true;
    let mut relic = permanent("relic", "Artifact", "player-1", Vec::new());
    relic.tapped = true;
    engine.state.players[0].battlefield = vec![tick.clone()];
    engine.state.players[1].battlefield = vec![relic];
    let target_id = activated["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let mut stack = stack_object(&tick, &activated);
    stack.targets.insert(
        target_id,
        TargetRef::Permanent {
            instance_id: "relic".to_string(),
        },
    );
    resolve(&mut engine, &activated, &stack);
    engine.state.active_player = 1;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the artifact controller's untap step");
    assert!(tapped(&engine, "relic"), "held while the tick stays tapped");
    engine.attempt_untap_permanent("tick");
    assert!(!tapped(&engine, "tick"), "the source untapped");
    let (player, index) = engine.permanent_position("tick").expect("tick");
    engine.state.players[player].battlefield[index].tapped = true;
    engine.state.active_player = 1;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the next untap step");
    assert!(
        !tapped(&engine, "relic"),
        "the ended effect does not resume"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_created_entry_rule_taps_entering_permanents_until_the_turn_ends() {
    // Due Respect (CR 611.2a, 614.1c): the rule applies to permanents that
    // enter after it was created, and ends with the turn.
    let rule = parsed_rules(
        "Due Respect",
        "Instant",
        "Permanents enter tapped this turn.\nDraw a card.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let source = test_instance("respect", test_definition("respect", "Instant"), "player-0");
    resolve(&mut engine, &rule, &stack_object(&source, &rule));
    let mut entering = test_instance(
        "entering",
        test_definition("entering", "Creature - Bear"),
        "player-1",
    );
    engine
        .apply_enter_replacements(&mut entering, &mut EmeritusDecisionProvider)
        .expect("the entry replacements apply");
    assert!(entering.tapped, "a permanent entering this turn is tapped");
    engine.state.turn_number += 1;
    let mut later = test_instance(
        "later",
        test_definition("later", "Creature - Bear"),
        "player-1",
    );
    engine
        .apply_enter_replacements(&mut later, &mut EmeritusDecisionProvider)
        .expect("the entry replacements apply");
    assert!(!later.tapped, "the created rule ended with its turn");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_created_graveyard_allowance_expires_with_the_turn() {
    // Yawgmoth's Will's allowance (CR 611.2a, 601.3): the controller may
    // cast spells from their graveyard until the turn ends.
    let rule = parsed_rules(
        "Graveyard Allowance Probe",
        "Sorcery",
        "Until end of turn, you may play lands and cast spells from your graveyard.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let source = test_instance("will", test_definition("will", "Sorcery"), "player-0");
    resolve(&mut engine, &rule, &stack_object(&source, &rule));
    let bolt = test_instance("bolt", test_definition("bolt", "Instant"), "player-0");
    assert!(
        engine
            .battlefield_casting_permission("player-0", &bolt, "graveyard")
            .is_some(),
        "the controller casts from the graveyard this turn"
    );
    assert!(
        engine
            .battlefield_casting_permission("player-1", &bolt, "graveyard")
            .is_none(),
        "the opponent holds no allowance"
    );
    engine.state.turn_number += 1;
    assert!(
        engine
            .battlefield_casting_permission("player-0", &bolt, "graveyard")
            .is_none(),
        "the allowance expired with its turn"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_legend_rule_waiver_keeps_both_legendary_twins() {
    // Mirror Gallery (CR 704.5j): the legend rule ignores the waived
    // permanents; without the waiver one twin goes to the graveyard.
    let gallery = parsed_rules(
        "Mirror Gallery",
        "Artifact",
        "The \"legend rule\" doesn't apply.",
    );
    let twins = |engine: &mut GameEngine| {
        for id in ["hero-a", "hero-b"] {
            let mut hero = permanent(id, "Legendary Creature - Human", "player-0", Vec::new());
            hero.definition.name = "Twin Hero".to_string();
            engine.state.players[0].battlefield.push(hero);
        }
    };
    let mut waived = test_engine(2);
    waived.state.players[0].battlefield =
        vec![permanent("gallery", "Artifact", "player-0", gallery)];
    twins(&mut waived);
    waived.check_state_based_actions();
    assert_eq!(waived.state.players[0].battlefield.len(), 3);
    let mut ruled = test_engine(2);
    twins(&mut ruled);
    ruled.check_state_based_actions();
    assert_eq!(ruled.state.players[0].battlefield.len(), 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn no_more_than_one_creature_attacks_each_combat() {
    // Silent Arbiter (CR 508.1c): once one attacker is selected, no other
    // creature may join the declaration.
    let arbiter = parsed_rules(
        "Silent Arbiter",
        "Artifact Creature \u{2014} Construct",
        "No more than one creature can attack each combat.\nNo more than one creature can block each combat.",
    );
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.players[1].battlefield = vec![permanent(
        "arbiter",
        "Artifact Creature - Construct",
        "player-1",
        arbiter,
    )];
    engine.state.players[0].battlefield = vec![
        permanent("first", "Creature - Soldier", "player-0", Vec::new()),
        permanent("second", "Creature - Soldier", "player-0", Vec::new()),
    ];
    let offered = |selected: &BTreeSet<String>| {
        engine
            .legal_attacker_actions(selected)
            .iter()
            .filter(|action| action.kind == ActionKind::DeclareAttacker)
            .filter_map(|action| action.attacker_id.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(offered(&BTreeSet::new()).len(), 2);
    assert!(offered(&BTreeSet::from(["first".to_string()])).is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn players_skip_their_untap_steps_and_cant_draw_or_win() {
    // Stasis, Omen Machine and Platinum Angel: player rules over a
    // population (CR 500.11, 121.1, 104.2a).
    let mut rules = parsed_rules(
        "Stasis",
        "Enchantment",
        "Players skip their untap steps.\nAt the beginning of your upkeep, sacrifice this enchantment unless you pay {U}.",
    );
    rules.extend(parsed_rules(
        "Omen Lock Probe",
        "Artifact",
        "Players can't draw cards.",
    ));
    rules.extend(parsed_rules(
        "Platinum Angel",
        "Artifact Creature \u{2014} Angel",
        "Flying\nYou can't lose the game and your opponents can't win the game.",
    ));
    let mut engine = test_engine(2);
    let mut land = permanent("land", "Basic Land - Island", "player-1", Vec::new());
    land.tapped = true;
    engine.state.players[0].battlefield = vec![permanent("lock", "Enchantment", "player-0", rules)];
    engine.state.players[1].battlefield = vec![land];
    engine.state.active_player = 1;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the skipped untap step");
    assert!(tapped(&engine, "land"), "the untap step was skipped");
    let library = engine.state.players[1].library.len();
    engine
        .draw_cards("player-1", 1)
        .expect("the draw is prohibited");
    assert_eq!(engine.state.players[1].library.len(), library);
    assert!(!engine.player_can_win("player-1"));
    assert!(engine.player_can_win("player-0"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_haste_permission_lets_summoning_sick_creatures_pay_tap_costs() {
    // Thousand-Year Elixir (CR 302.6): only creatures its controller
    // controls ignore summoning sickness for tap-symbol costs.
    let elixir = parsed_rules(
        "Thousand-Year Elixir",
        "Artifact",
        "You may activate abilities of creatures you control as though those creatures had haste.\n{1}, {T}: Untap target creature.",
    );
    let mut engine = test_engine(2);
    let mut own = permanent("own", "Creature - Elf", "player-0", Vec::new());
    own.summoning_sick = true;
    let mut theirs = permanent("theirs", "Creature - Elf", "player-1", Vec::new());
    theirs.summoning_sick = true;
    engine.state.players[0].battlefield = vec![
        permanent("elixir", "Artifact", "player-0", elixir),
        own.clone(),
    ];
    engine.state.players[1].battlefield = vec![theirs.clone()];
    assert!(!engine.permanent_cannot_pay_tap_symbol(&own));
    assert!(engine.permanent_cannot_pay_tap_symbol(&theirs));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_preposed_lifetime_scopes_an_entry_rule_and_a_draw_prohibition() {
    // An unused combination of the frame's rules under one preposed
    // lifetime (CR 611.2a, 611.2b): both rules last until the controller's
    // next turn.
    let rule = parsed_rules(
        "Lockdown Probe",
        "Sorcery",
        "Until your next turn, artifacts your opponents control enter tapped and players can't draw cards.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let source = test_instance(
        "lockdown",
        test_definition("lockdown", "Sorcery"),
        "player-0",
    );
    resolve(&mut engine, &rule, &stack_object(&source, &rule));
    let mut theirs = test_instance(
        "their-relic",
        test_definition("their-relic", "Artifact"),
        "player-1",
    );
    engine
        .apply_enter_replacements(&mut theirs, &mut EmeritusDecisionProvider)
        .expect("the entry replacements apply");
    assert!(theirs.tapped, "an opponent's artifact enters tapped");
    let mut own = test_instance(
        "own-relic",
        test_definition("own-relic", "Artifact"),
        "player-0",
    );
    engine
        .apply_enter_replacements(&mut own, &mut EmeritusDecisionProvider)
        .expect("the entry replacements apply");
    assert!(!own.tapped, "the controller's artifact is not affected");
    let library = engine.state.players[0].library.len();
    engine
        .draw_cards("player-0", 1)
        .expect("the draw is prohibited");
    assert_eq!(engine.state.players[0].library.len(), library);
}
