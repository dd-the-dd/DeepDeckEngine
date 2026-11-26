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
    // Fixture permanents get their tracked zone identities, as a real entry
    // gives them, so a legend-rule move is not refused for a missing one.
    let check = |engine: &mut GameEngine| {
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        engine
            .check_state_based_actions_with_provider(None)
            .expect("state-based actions complete");
    };
    check(&mut waived);
    assert_eq!(waived.state.players[0].battlefield.len(), 3);
    let mut ruled = test_engine(2);
    twins(&mut ruled);
    check(&mut ruled);
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

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn control_lasts_while_the_source_remains_on_the_battlefield() {
    // Sower of Temptation's window (CR 611.2b, 613.1b): control returns to
    // the original controller once the source leaves the battlefield.
    let rules = parsed_rules(
        "Control Window Probe",
        "Creature \u{2014} Faerie Wizard",
        "{T}: Gain control of target creature for as long as this creature remains on the battlefield.",
    );
    let activated = rules[0].clone();
    let mut engine = test_engine(2);
    let sower = permanent("sower", "Creature - Faerie Wizard", "player-0", rules);
    engine.state.players[0].battlefield = vec![sower.clone()];
    engine.state.players[1].battlefield =
        vec![permanent("bear", "Creature - Bear", "player-1", Vec::new())];
    let target_id = activated["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let mut stack = stack_object(&sower, &activated);
    stack.targets.insert(
        target_id,
        TargetRef::Permanent {
            instance_id: "bear".to_string(),
        },
    );
    resolve(&mut engine, &activated, &stack);
    let controller = |engine: &GameEngine| {
        let (player, index) = engine.permanent_position("bear").expect("bear");
        engine.state.players[player].battlefield[index]
            .controller
            .clone()
    };
    engine.refresh_control_effects();
    assert_eq!(controller(&engine), "player-0");
    engine.state.players[0]
        .battlefield
        .retain(|card| card.instance_id != "sower");
    engine.refresh_control_effects();
    assert_eq!(
        controller(&engine),
        "player-1",
        "control ended with the source"
    );
}

// ---------------------------------------------------------------------------
// Spell and ability rules: temporary static bodies (CR 611.2a, 611.2c),
// activated-ability and targeting populations of the cost-adjustment frame
// (CR 601.2c, 601.2f, 602.2b) and spell-history ordinals (CR 601.2i).
// ---------------------------------------------------------------------------

/// The total mana value of cost components (each non-generic symbol is 1).
fn total_mana(costs: &[String]) -> i32 {
    costs
        .iter()
        .flat_map(|cost| super::super::mana::parse_mana_symbols(cost))
        .map(|symbol| symbol.parse::<i32>().unwrap_or(1))
        .sum()
}

fn spell_definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition
}

fn spell_ability(rules: &[Value]) -> Value {
    rules
        .iter()
        .find(|rule| rule["kind"] == "spellAbility")
        .cloned()
        .expect("the instant has a spell ability")
}

/// A spell on the stack, controlled by `controller`.
fn stack_spell(id: &str, definition: CardDefinition, controller: &str) -> StackObject {
    let card = test_instance(id, definition, controller);
    StackObject {
        id: format!("stack:{id}"),
        controller: controller.to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_temporary_spell_cost_reduction_expires_with_the_turn() {
    // Armor Wars II's body: `Artifact spells you cast this turn cost {1}
    // less to cast.` The lifetime is the subject's cast period; the body is
    // the static reduction of the controller's artifact spells.
    let rules = parsed_rules(
        "Test Armory",
        "Instant",
        "Artifact spells you cast this turn cost {1} less to cast.",
    );
    let rule = spell_ability(&rules);
    assert_eq!(
        rule["effects"][0]["kind"],
        "installTemporaryStaticModifiers"
    );
    assert_eq!(
        rule["effects"][0]["modifiers"][0]["kind"],
        "reduceCastingCost"
    );
    let mut engine = test_engine(2);
    let source = permanent("armory", "Instant", "player-0", rules.clone());
    let stack = stack_object(&source, &rule);
    let artifact = spell_definition("relic", "Artifact", "{3}");
    let creature = spell_definition("bear", "Creature - Bear", "{3}");
    let costs = |engine: &GameEngine, card: &CardDefinition, player: &str| {
        total_mana(&engine.casting_mana_costs(card, player, &BTreeMap::new()))
    };
    assert_eq!(costs(&engine, &artifact, "player-0"), 3);
    resolve(&mut engine, &rule, &stack);
    assert_eq!(costs(&engine, &artifact, "player-0"), 2);
    assert_eq!(
        costs(&engine, &creature, "player-0"),
        3,
        "only artifact spells"
    );
    assert_eq!(
        costs(&engine, &artifact, "player-1"),
        3,
        "only its controller's"
    );
    engine.state.turn_number += 1;
    assert_eq!(
        costs(&engine, &artifact, "player-0"),
        3,
        "the effect ends with the turn (CR 514.2)"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_temporary_uncounterable_marker_protects_only_its_spells() {
    // Domri, Anarch of Bolas's +1 body: `Creature spells you cast this turn
    // can't be countered.` A counterspell checks the prohibition as it
    // would counter (CR 701.6).
    let rules = parsed_rules(
        "Test Shield",
        "Instant",
        "Creature spells you cast this turn can't be countered.",
    );
    let rule = spell_ability(&rules);
    let mut engine = test_engine(2);
    let source = permanent("shield", "Instant", "player-0", rules.clone());
    let stack = stack_object(&source, &rule);
    let creature = stack_spell(
        "bear-spell",
        spell_definition("bear", "Creature - Bear", "{2}"),
        "player-0",
    );
    let instant = stack_spell(
        "bolt-spell",
        spell_definition("bolt", "Instant", "{R}"),
        "player-0",
    );
    let opposing = stack_spell(
        "wolf-spell",
        spell_definition("wolf", "Creature - Wolf", "{2}"),
        "player-1",
    );
    assert!(!engine.stack_object_cant_be_countered(&creature));
    resolve(&mut engine, &rule, &stack);
    assert!(engine.stack_object_cant_be_countered(&creature));
    assert!(!engine.stack_object_cant_be_countered(&instant));
    assert!(!engine.stack_object_cant_be_countered(&opposing));
    engine.state.turn_number += 1;
    assert!(!engine.stack_object_cant_be_countered(&creature));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_static_counter_prohibition_reads_the_cast_relation() {
    // Unused combination: an instant spell query with the `you cast`
    // relation under the static counter prohibition.
    let rules = parsed_rules(
        "Test Warden",
        "Enchantment",
        "Instant spells you cast can't be countered.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![permanent("warden", "Enchantment", "player-0", rules)];
    let instant = stack_spell(
        "bolt-spell",
        spell_definition("bolt", "Instant", "{R}"),
        "player-0",
    );
    let opposing = stack_spell(
        "shock-spell",
        spell_definition("shock", "Instant", "{R}"),
        "player-1",
    );
    assert!(engine.stack_object_cant_be_countered(&instant));
    assert!(!engine.stack_object_cant_be_countered(&opposing));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn an_untimed_next_spell_reduction_waits_for_its_spell() {
    // Xho Cai's body: `the next noncreature spell you cast costs {1} less
    // to cast.` Without `this turn` it lasts until that spell is cast; the
    // spell that created it was cast before the rule existed.
    let rules = parsed_rules(
        "Test Focus",
        "Instant",
        "The next noncreature spell you cast costs {1} less to cast.",
    );
    let rule = spell_ability(&rules);
    assert_eq!(rule["effects"][0]["expires"]["kind"], "whenConsumed");
    let mut engine = test_engine(2);
    let source = permanent("focus", "Instant", "player-0", rules.clone());
    let stack = stack_object(&source, &rule);
    let artifact = spell_definition("relic", "Artifact", "{3}");
    let creature = spell_definition("bear", "Creature - Bear", "{3}");
    assert_eq!(
        engine.one_shot_casting_cost_reduction("player-0", &artifact),
        0
    );
    resolve(&mut engine, &rule, &stack);
    assert_eq!(
        engine.one_shot_casting_cost_reduction("player-0", &creature),
        0
    );
    engine.state.turn_number += 2;
    assert_eq!(
        engine.one_shot_casting_cost_reduction("player-0", &artifact),
        1
    );
    engine.consume_one_shot_spell_cast_modifier("player-0", &artifact);
    assert_eq!(
        engine.one_shot_casting_cost_reduction("player-0", &artifact),
        0
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn an_ordinal_spell_reduction_counts_the_casters_earlier_spells() {
    // Raging Battle Mouse: `The second spell you cast each turn costs {1}
    // less to cast.` The cost is determined before the spell becomes cast
    // (CR 601.2f, 601.2i), so one earlier cast makes it the second.
    let rules = parsed_rules(
        "Test Mouse",
        "Creature \u{2014} Mouse",
        "The second spell you cast each turn costs {1} less to cast.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![permanent("mouse", "Creature - Mouse", "player-0", rules)];
    let spell = spell_definition("relic", "Artifact", "{3}");
    let cost = |engine: &GameEngine| {
        total_mana(&engine.casting_mana_costs(&spell, "player-0", &BTreeMap::new()))
    };
    assert_eq!(cost(&engine), 3, "the first spell");
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some("first".to_string()),
        json!({}),
    );
    assert_eq!(cost(&engine), 2, "the second spell");
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some("second".to_string()),
        json!({}),
    );
    assert_eq!(cost(&engine), 3, "the third spell");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn an_equip_reduction_applies_only_when_targeting_its_source() {
    // Fervent Champion: `Equip abilities you activate that target this
    // creature cost {3} less to activate.` The target is chosen before the
    // total cost (CR 601.2c via 602.2b).
    let rules = parsed_rules(
        "Test Champion",
        "Creature \u{2014} Human Knight",
        "Equip abilities you activate that target this creature cost {3} less to activate.",
    );
    let mut engine = test_engine(2);
    let champion = permanent("champion", "Creature - Human Knight", "player-0", rules);
    let bear = permanent("bear", "Creature - Bear", "player-0", Vec::new());
    let sword = permanent("sword", "Artifact - Equipment", "player-0", Vec::new());
    engine.state.players[0].battlefield = vec![champion, bear, sword.clone()];
    let equip_cost = |engine: &GameEngine, target: &str, activator: &str| {
        let targets = BTreeMap::from([(
            "equipTarget".to_string(),
            TargetRef::Permanent {
                instance_id: target.to_string(),
            },
        )]);
        let mut costs = vec!["{4}".to_string()];
        engine.adjust_activation_mana_costs(
            &mut costs,
            &super::super::casting_cost::ActivationCostSubject {
                activator,
                source: &sword,
                on_battlefield: true,
                class: Some("equip"),
                targets: &targets,
            },
        );
        total_mana(&costs)
    };
    assert_eq!(equip_cost(&engine, "champion", "player-0"), 1);
    assert_eq!(equip_cost(&engine, "bear", "player-0"), 4, "another target");
    assert_eq!(
        equip_cost(&engine, "champion", "player-1"),
        4,
        "another activator"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_loyalty_tax_and_a_floored_cycling_reduction_keep_their_populations() {
    // Eidolon of Obstruction: `Loyalty abilities of planeswalkers your
    // opponents control cost {1} more to activate.` Unused combination: a
    // cycling population with the one-mana floor.
    let eidolon = parsed_rules(
        "Test Eidolon",
        "Enchantment Creature \u{2014} Nymph",
        "Loyalty abilities of planeswalkers your opponents control cost {1} more to activate.",
    );
    let fluctuator = parsed_rules(
        "Test Fluctuator",
        "Artifact",
        "Cycling abilities you activate cost {2} less to activate. This effect can't reduce the mana in that cost to less than one mana.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent(
            "eidolon",
            "Enchantment Creature - Nymph",
            "player-0",
            eidolon,
        ),
        permanent("fluctuator", "Artifact", "player-0", fluctuator),
        permanent("own-walker", "Planeswalker - Test", "player-0", Vec::new()),
    ];
    engine.state.players[1].battlefield = vec![permanent(
        "their-walker",
        "Planeswalker - Test",
        "player-1",
        Vec::new(),
    )];
    let loyalty_cost = |engine: &GameEngine, walker: &str, activator: &str| {
        let (player, index) = engine.permanent_position(walker).expect("walker");
        let source = engine.state.players[player].battlefield[index].clone();
        let mut costs = Vec::new();
        engine.adjust_activation_mana_costs(
            &mut costs,
            &super::super::casting_cost::ActivationCostSubject {
                activator,
                source: &source,
                on_battlefield: true,
                class: Some("loyalty"),
                targets: &BTreeMap::new(),
            },
        );
        total_mana(&costs)
    };
    assert_eq!(loyalty_cost(&engine, "their-walker", "player-1"), 1);
    assert_eq!(loyalty_cost(&engine, "own-walker", "player-0"), 0);
    let cycler = test_instance(
        "cycler",
        spell_definition("cycler", "Creature - Beast", "{4}"),
        "player-0",
    );
    let generic = engine.cycling_activation_mana_costs("player-0", &cycler, "{2}");
    assert_eq!(total_mana(&generic), 1, "never below one mana");
    let colored = engine.cycling_activation_mana_costs("player-0", &cycler, "{2}{U}");
    assert_eq!(total_mana(&colored), 1, "the colored symbol keeps one mana");
    let opposing = engine.cycling_activation_mana_costs("player-1", &cycler, "{2}");
    assert_eq!(
        total_mana(&opposing),
        2,
        "only its controller's activations"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_targeting_spell_tax_reads_the_chosen_targets() {
    // Syr Elenora's sibling: `Spells your opponents cast that target this
    // creature cost {2} more to cast.` (CR 601.2c, 601.2f).
    let rules = parsed_rules(
        "Test Guardian",
        "Creature \u{2014} Elf Knight",
        "Spells your opponents cast that target this creature cost {2} more to cast.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("guardian", "Creature - Elf Knight", "player-0", rules),
        permanent("bear", "Creature - Bear", "player-0", Vec::new()),
    ];
    let spell = spell_definition("bolt", "Instant", "{R}");
    let targeting = |target: &str| {
        BTreeMap::from([(
            "target".to_string(),
            TargetRef::Permanent {
                instance_id: target.to_string(),
            },
        )])
    };
    assert_eq!(
        engine.spell_targeting_cost_adjustment("player-1", &spell, &targeting("guardian")),
        (0, vec!["{2}".to_string()])
    );
    assert_eq!(
        engine.spell_targeting_cost_adjustment("player-1", &spell, &targeting("bear")),
        (0, Vec::new())
    );
    assert_eq!(
        engine.spell_targeting_cost_adjustment("player-0", &spell, &targeting("guardian")),
        (0, Vec::new()),
        "its controller's spells are not taxed"
    );
    // Without targets the global increase never applies the tax.
    assert_eq!(
        total_mana(&engine.casting_mana_costs(&spell, "player-1", &BTreeMap::new())),
        1
    );
}
