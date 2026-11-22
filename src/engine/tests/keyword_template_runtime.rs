//! Template keywords (CR 702) compiled from real Oracle keyword lines run on
//! the rules of their instantiated definition, through the playable-card
//! adapter that flattens the keyword expansion.
use super::*;

/// The executable definition of a real card, compiled from its Oracle text.
fn compiled(
    name: &str,
    type_line: &str,
    power_toughness: (&str, &str),
    oracle: &str,
) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace(' ', "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some(power_toughness.0.to_string()),
        toughness: Some(power_toughness.1.to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{1}{W}".to_string()),
            oracle_text: Some(oracle.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the card compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{name}: {:#?}",
        compilation.parser_result.diagnostics
    );
    compilation.card
}

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Test");
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    test_instance(id, definition, controller)
}

fn power_toughness(engine: &GameEngine, id: &str) -> (i32, i32) {
    let (player, index) = engine.permanent_position(id).expect("on the battlefield");
    let permanent = &engine.state.players[player].battlefield[index];
    (
        engine.current_power(permanent),
        engine.current_toughness(permanent),
    )
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn accorder_paladin_battle_cry_pumps_each_other_attacker_once() {
    let paladin = compiled(
        "Accorder Paladin",
        "Creature \u{2014} Human Knight",
        ("3", "1"),
        "Battle cry (Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn.)",
    );
    let [rule] = paladin.rules.as_slice() else {
        panic!("one rule: {:#?}", paladin.rules);
    };
    assert_eq!(value_kind(rule), Some("triggeredAbility"));
    assert_eq!(rule["keywordProvenance"]["keyword"], "battle-cry");

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("accorder-paladin", paladin, "player-0"),
        creature("attacker", "player-0"),
        creature("home", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![creature("opposing-attacker", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for id in ["accorder-paladin", "attacker"] {
        let (player, index) = engine.permanent_position(id).expect("on the battlefield");
        engine.state.players[player].battlefield[index]
            .flags
            .insert("attacking".to_string(), true);
    }
    engine.enqueue_declared_attacker_triggers(&[
        "accorder-paladin".to_string(),
        "attacker".to_string(),
    ]);
    assert_eq!(engine.state.stack.len(), 1, "one battle cry trigger");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("battle cry resolves");

    // CR 702.91a: each OTHER attacking creature gets +1/+0.
    assert_eq!(power_toughness(&engine, "attacker"), (3, 2));
    assert_eq!(power_toughness(&engine, "accorder-paladin"), (3, 1));
    assert_eq!(power_toughness(&engine, "home"), (2, 2));
    assert_eq!(power_toughness(&engine, "opposing-attacker"), (2, 2));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn migrated_afterlife_creates_its_spirits_when_its_creature_is_destroyed() {
    // CR 702.135a: the definition's zone-change wording still triggers on a
    // creature's death and creates N flying Spirits for its controller.
    let seraph = compiled(
        "Seraph of the Scales",
        "Creature \u{2014} Angel",
        ("4", "3"),
        "Afterlife 2 (When this creature dies, create two 1/1 white and black Spirit creature tokens with flying.)",
    );
    assert_eq!(seraph.rules[0]["keywordProvenance"]["keyword"], "afterlife");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![test_instance("seraph-of-the-scales", seraph, "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let killer = StackObject {
        id: "stack:killer".to_string(),
        controller: "player-1".to_string(),
        card: creature("killer", "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .destroy_permanent_from_stack("seraph-of-the-scales", &killer)
        .expect("the destruction resolves");
    assert!(engine.permanent_position("seraph-of-the-scales").is_none());
    assert_eq!(engine.state.stack.len(), 1, "one afterlife trigger");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("afterlife resolves");
    let spirits = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| {
            permanent.definition.is_token && permanent.definition.type_line.contains("Spirit")
        })
        .collect::<Vec<_>>();
    assert_eq!(spirits.len(), 2);
    assert!(
        spirits
            .iter()
            .all(|spirit| engine.permanent_has_keyword(spirit, "flying"))
    );
}

fn counters(engine: &GameEngine, id: &str) -> i32 {
    let (player, index) = engine.permanent_position(id).expect("on the battlefield");
    engine.state.players[player].battlefield[index]
        .counters
        .get("+1/+1")
        .copied()
        .unwrap_or(0)
}

fn mana_engine(lands: &[(&str, &str)]) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = lands
        .iter()
        .map(|(id, symbol)| mana_source(id, "Basic Land - Forest", symbol, None))
        .collect();
    engine
}

fn activate(engine: &mut GameEngine, source_id: &str) -> bool {
    let Some(action) = engine.legal_priority_actions(0).into_iter().find(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id.as_deref() == Some(source_id)
    }) else {
        return false;
    };
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(7))
        .expect("the activation pays its costs");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the ability resolves");
    true
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn outlast_and_reinforce_activate_from_their_definitions_zones() {
    // Outlast {1}{G} (CR 702.107a): tap and pay on the battlefield.
    let bond_kin = compiled(
        "Ainok Bond-Kin",
        "Creature \u{2014} Dog Soldier",
        ("2", "1"),
        "Outlast {1}{G} ({1}{G}, {T}: Put a +1/+1 counter on this creature. Outlast only as a sorcery.)",
    );
    let mut engine = mana_engine(&[("forest-a", "{G}"), ("forest-b", "{G}")]);
    engine.state.players[0]
        .battlefield
        .push(test_instance("ainok-bond-kin", bond_kin, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(activate(&mut engine, "ainok-bond-kin"));
    assert_eq!(counters(&engine, "ainok-bond-kin"), 1);
    // {T} is part of the cost: the tapped creature cannot outlast again.
    assert!(!activate(&mut engine, "ainok-bond-kin"));

    // Reinforce 2—{1}{G} (CR 702.77a): discard the card from hand.
    let triad = compiled(
        "Hunting Triad",
        "Tribal Sorcery \u{2014} Elf",
        ("0", "0"),
        "Reinforce 2\u{2014}{1}{G} ({1}{G}, Discard this card: Put two +1/+1 counters on target creature.)",
    );
    let mut engine = mana_engine(&[("forest-a", "{G}"), ("forest-b", "{G}")]);
    engine.state.players[0]
        .battlefield
        .push(creature("bear", "player-0"));
    engine.state.players[0].hand = vec![test_instance("hunting-triad", triad.clone(), "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(activate(&mut engine, "hunting-triad"));
    assert_eq!(counters(&engine, "bear"), 2);
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "hunting-triad")
    );
    // From the battlefield the hand-only ability is not offered.
    let mut engine = mana_engine(&[("forest-a", "{G}"), ("forest-b", "{G}")]);
    engine.state.players[0]
        .battlefield
        .push(test_instance("hunting-triad", triad, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(!activate(&mut engine, "hunting-triad"));
}

/// Puts `card` onto player 0's battlefield and resolves its entry trigger.
fn enter_and_resolve(engine: &mut GameEngine, card: CardInstance) {
    engine.state.players[0].battlefield.push(card.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let (player, index) = engine
        .permanent_position(&card.instance_id)
        .expect("entered");
    let live = engine.state.players[player].battlefield[index].clone();
    engine.enqueue_enter_battlefield_triggers(&live);
    assert_eq!(engine.state.stack.len(), 1, "one entry trigger");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn entry_keywords_create_and_attach_or_grow_from_their_definitions() {
    // For Mirrodin! (CR 702.163a): a 2/2 red Rebel equipped by the source.
    let bardiche = compiled(
        "Mirran Bardiche",
        "Artifact \u{2014} Equipment",
        ("0", "0"),
        "For Mirrodin! (When this Equipment enters, create a 2/2 red Rebel creature token, then attach this to it.)",
    );
    let mut engine = test_engine(2);
    enter_and_resolve(
        &mut engine,
        test_instance("mirran-bardiche", bardiche, "player-0"),
    );
    let rebel = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.definition.is_token)
        .expect("a Rebel token")
        .clone();
    assert!(rebel.definition.type_line.contains("Rebel"));
    let (player, index) = engine
        .permanent_position("mirran-bardiche")
        .expect("the Equipment stays");
    assert_eq!(
        engine.state.players[player].battlefield[index]
            .attached_to
            .as_deref(),
        Some(rebel.instance_id.as_str())
    );

    // Fabricate 2 (CR 702.123a), choosing the counters.
    let artisan = compiled(
        "Glint-Sleeve Artisan",
        "Creature \u{2014} Dwarf Artificer",
        ("2", "2"),
        "Fabricate 2 (When this creature enters, put two +1/+1 counters on it or create two 1/1 colorless Servo artifact creature tokens.)",
    );
    let mut engine = test_engine(2);
    enter_and_resolve(
        &mut engine,
        test_instance("glint-sleeve-artisan", artisan, "player-0"),
    );
    let tokens = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.is_token)
        .count();
    let placed = counters(&engine, "glint-sleeve-artisan");
    // Exactly one branch of the choice happens (CR 702.123a "If you don't").
    assert!(
        (placed, tokens) == (2, 0) || (placed, tokens) == (0, 2),
        "counters {placed}, Servos {tokens}"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn modular_moves_the_last_known_counters_of_its_dead_source() {
    // CR 702.43a and 603.10a: the death trigger counts the counters the
    // creature had when it left the battlefield, a +1/+1 counter added after
    // entering included.
    let hoarder = compiled(
        "Arcbound Hoarder",
        "Artifact Creature \u{2014} Construct",
        ("0", "0"),
        "Modular 2 (This creature enters with two +1/+1 counters on it. When it dies, you may put its +1/+1 counters on target artifact creature.)",
    );
    assert_eq!(hoarder.rules.len(), 2);
    let mut engine = test_engine(2);
    let mut entering = test_instance("arcbound-hoarder", hoarder, "player-0");
    engine
        .apply_enter_replacements(&mut entering, &mut EmeritusDecisionProvider)
        .expect("the modular counters are placed as it enters");
    assert_eq!(entering.counters.get("+1/+1"), Some(&2));
    entering.counters.insert("+1/+1".to_string(), 3);
    let mut receiver_definition =
        test_definition("receiver", "Artifact Creature \u{2014} Construct");
    receiver_definition.power = Some("1".to_string());
    receiver_definition.toughness = Some("1".to_string());
    engine.state.players[0].battlefield = vec![
        entering,
        test_instance("receiver", receiver_definition, "player-0"),
        creature("plain-creature", "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let killer = StackObject {
        id: "stack:killer".to_string(),
        controller: "player-1".to_string(),
        card: creature("killer", "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .destroy_permanent_from_stack("arcbound-hoarder", &killer)
        .expect("the destruction resolves");
    assert_eq!(engine.state.stack.len(), 1, "one modular trigger");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("modular resolves with the source's last-known counters");
    assert_eq!(counters(&engine, "receiver"), 3);
    assert_eq!(counters(&engine, "plain-creature"), 0);
}

/// Accepts or declines every optional choice; every other choice is the first.
struct Optional(bool);

impl DecisionProvider for Optional {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(match request.kind {
            DecisionKind::ResolutionChoice if self.0 => request.options.len().saturating_sub(1),
            _ => 0,
        })
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn exploit_publishes_its_keyword_action_only_when_a_creature_is_sacrificed() {
    // An unused combination: exploit with a life-gain exploit trigger.
    let oracle = "Exploit (When this creature enters, you may sacrifice a creature.)\nWhen this creature exploits a creature, you gain 3 life.";
    for (accept, exploited) in [(true, true), (false, false)] {
        let scavenger = compiled(
            "Exploit Probe",
            "Creature \u{2014} Zombie",
            ("2", "2"),
            oracle,
        );
        let mut engine = test_engine(2);
        engine.state.players[0]
            .battlefield
            .push(creature("fodder", "player-0"));
        let card = test_instance("exploit-probe", scavenger, "player-0");
        engine.state.players[0].battlefield.push(card.clone());
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let (player, index) = engine.permanent_position("exploit-probe").expect("entered");
        let live = engine.state.players[player].battlefield[index].clone();
        engine.enqueue_enter_battlefield_triggers(&live);
        assert_eq!(engine.state.stack.len(), 1, "the exploit trigger");
        let life = engine.state.players[0].life;
        engine
            .resolve_top_stack(&mut Optional(accept))
            .expect("exploit resolves");
        // CR 702.110b: the keyword action triggers "exploits a creature".
        assert_eq!(
            engine.state.stack.len(),
            usize::from(exploited),
            "accept {accept}"
        );
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut Optional(accept))
                .expect("the exploit trigger resolves");
        }
        assert_eq!(
            engine.state.players[0].life,
            life + if exploited { 3 } else { 0 },
            "accept {accept}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn bloodthirst_counters_depend_on_damage_dealt_to_an_opponent_this_turn() {
    // CR 702.54a: checked as the creature enters; damage to a permanent or to
    // its own controller does not count.
    let minotaurs = compiled(
        "Gorehorn Minotaurs",
        "Creature \u{2014} Minotaur Warrior",
        ("3", "3"),
        "Bloodthirst 2 (If an opponent was dealt damage this turn, this creature enters with two +1/+1 counters on it.)",
    );
    for (recipient, expected) in [
        (None, None),
        (Some(("player-0", false)), None),
        (Some(("player-1", true)), None),
        (Some(("player-1", false)), Some(&2)),
    ] {
        let mut engine = test_engine(2);
        if let Some((player, to_permanent)) = recipient {
            engine.record_event(
                "damageDealt",
                (!to_permanent).then(|| player.to_string()),
                to_permanent.then(|| "some-permanent".to_string()),
                json!({ "amount": 2, "combat": true }),
            );
        }
        let mut entering = test_instance("gorehorn-minotaurs", minotaurs.clone(), "player-0");
        engine
            .apply_enter_replacements(&mut entering, &mut EmeritusDecisionProvider)
            .expect("the creature enters");
        assert_eq!(entering.counters.get("+1/+1"), expected, "{recipient:?}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn counter_possession_gates_triggers_when_put_on_the_stack_and_after_death() {
    // CR 603.4: an intervening `if` over the source is checked as the trigger
    // would be put on the stack (it used to be read as false there).
    let watcher = compiled(
        "Counter Watcher",
        "Creature \u{2014} Plant",
        ("1", "1"),
        "Whenever another creature enters, if this creature has two or more +1/+1 counters on it, you gain 1 life.",
    );
    for (counters, triggers) in [(2, 1), (1, 0)] {
        let mut engine = test_engine(2);
        let mut source = test_instance("counter-watcher", watcher.clone(), "player-0");
        source.counters.insert("+1/+1".to_string(), counters);
        engine.state.players[0].battlefield = vec![source];
        let newcomer = creature("newcomer", "player-0");
        engine.state.players[0].battlefield.push(newcomer);
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let (player, index) = engine.permanent_position("newcomer").expect("entered");
        let live = engine.state.players[player].battlefield[index].clone();
        engine.enqueue_enter_battlefield_triggers(&live);
        assert_eq!(engine.state.stack.len(), triggers, "{counters} counters");
    }

    // CR 603.10a: a dies trigger reads the counters its source had.
    let mourner = compiled(
        "Counter Mourner",
        "Creature \u{2014} Spirit",
        ("2", "2"),
        "When this creature dies, if it had no -1/-1 counters on it, you gain 2 life.",
    );
    for (counters, gained) in [(0, 2), (1, 0)] {
        let mut engine = test_engine(2);
        let mut source = test_instance("counter-mourner", mourner.clone(), "player-0");
        if counters > 0 {
            source.counters.insert("-1/-1".to_string(), counters);
        }
        engine.state.players[0].battlefield = vec![source];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let life = engine.state.players[0].life;
        let killer = StackObject {
            id: "stack:killer".to_string(),
            controller: "player-1".to_string(),
            card: creature("killer", "player-1"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("spellAbility".to_string()),
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        };
        engine
            .destroy_permanent_from_stack("counter-mourner", &killer)
            .expect("the destruction resolves");
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut EmeritusDecisionProvider)
                .expect("the dies trigger resolves");
        }
        assert_eq!(engine.state.players[0].life, life + gained, "{counters}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn vanishing_removes_a_time_counter_each_upkeep_and_sacrifices_at_the_last() {
    // CR 702.63a: enters with N time counters; each upkeep removes one; the
    // removal of the last one triggers the sacrifice.
    let shadow = compiled(
        "Vanishing Probe",
        "Creature \u{2014} Shade",
        ("3", "3"),
        "Vanishing 2 (This creature enters with two time counters on it. At the beginning of your upkeep, remove a time counter from it. When the last is removed, sacrifice it.)",
    );
    assert_eq!(shadow.rules.len(), 3);
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    let mut entering = test_instance("vanishing-probe", shadow, "player-0");
    engine
        .apply_enter_replacements(&mut entering, &mut EmeritusDecisionProvider)
        .expect("it enters with its time counters");
    assert_eq!(entering.counters.get("time"), Some(&2));
    engine.state.players[0].battlefield = vec![entering];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let upkeep = |engine: &mut GameEngine| {
        engine.enqueue_step_triggers("upkeep");
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut EmeritusDecisionProvider)
                .expect("the upkeep triggers resolve");
        }
    };
    upkeep(&mut engine);
    let (player, index) = engine
        .permanent_position("vanishing-probe")
        .expect("still on the battlefield after the first upkeep");
    assert_eq!(
        engine.state.players[player].battlefield[index]
            .counters
            .get("time"),
        Some(&1)
    );
    upkeep(&mut engine);
    assert!(
        engine.permanent_position("vanishing-probe").is_none(),
        "the last time counter's removal sacrificed it"
    );
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "vanishing-probe")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn evolve_checks_the_entering_creature_against_its_source_when_it_triggers() {
    // CR 702.100a/603.4: the intervening `if` compares the entering
    // creature's power and toughness with this creature's, when the trigger
    // would be put on the stack.
    let raptor = compiled(
        "Cloudfin Raptor",
        "Creature \u{2014} Bird Mutant",
        ("0", "1"),
        "Flying\nEvolve (Whenever a creature you control enters, if that creature has greater power or toughness than this creature, put a +1/+1 counter on this creature.)",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![test_instance("cloudfin-raptor", raptor, "player-0")];
    let sized = |id: &str, power: &str, toughness: &str| {
        let mut definition = test_definition(id, "Creature \u{2014} Test");
        definition.power = Some(power.to_string());
        definition.toughness = Some(toughness.to_string());
        test_instance(id, definition, "player-0")
    };
    let enter = |engine: &mut GameEngine, card: CardInstance| -> usize {
        engine.state.players[0].battlefield.push(card.clone());
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let (player, index) = engine
            .permanent_position(&card.instance_id)
            .expect("entered");
        let live = engine.state.players[player].battlefield[index].clone();
        engine.enqueue_enter_battlefield_triggers(&live);
        let published = engine.state.stack.len();
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut EmeritusDecisionProvider)
                .expect("the evolve trigger resolves");
        }
        published
    };
    // 0/1 vs 1/1: greater power.
    assert_eq!(enter(&mut engine, sized("squire", "1", "1")), 1);
    assert_eq!(power_toughness(&engine, "cloudfin-raptor"), (1, 2));
    // 1/2 vs 1/2: neither is greater, so nothing is put on the stack.
    assert_eq!(enter(&mut engine, sized("twin", "1", "2")), 0);
    assert_eq!(power_toughness(&engine, "cloudfin-raptor"), (1, 2));
    // 0/3 vs 1/2: greater toughness alone is enough.
    assert_eq!(enter(&mut engine, sized("wall", "0", "3")), 1);
    assert_eq!(power_toughness(&engine, "cloudfin-raptor"), (2, 3));
    // An opponent's creature is not "a creature you control".
    engine.state.players[1]
        .battlefield
        .push(creature("giant", "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let (player, index) = engine.permanent_position("giant").expect("entered");
    let live = engine.state.players[player].battlefield[index].clone();
    engine.enqueue_enter_battlefield_triggers(&live);
    assert!(engine.state.stack.is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn mentor_targets_only_attackers_with_less_power_than_its_source() {
    // CR 702.134a: the target filter compares each candidate's power with
    // the source's power, read when the target is chosen.
    let mentor = compiled(
        "Goblin Banneret",
        "Creature \u{2014} Goblin Soldier",
        ("1", "1"),
        "Mentor (Whenever this creature attacks, put a +1/+1 counter on target attacking creature with lesser power.)",
    );
    let mut engine = test_engine(2);
    let sized = |id: &str, power: &str| {
        let mut definition = test_definition(id, "Creature \u{2014} Test");
        definition.power = Some(power.to_string());
        definition.toughness = Some("4".to_string());
        test_instance(id, definition, "player-0")
    };
    let mut source = test_instance("goblin-banneret", mentor, "player-0");
    source.counters.insert("+1/+1".to_string(), 2);
    engine.state.players[0].battlefield = vec![
        source,
        sized("weaker", "2"),
        sized("equal", "3"),
        sized("idle", "0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for id in ["goblin-banneret", "weaker", "equal"] {
        let (player, index) = engine.permanent_position(id).expect("on the battlefield");
        engine.state.players[player].battlefield[index]
            .flags
            .insert("attacking".to_string(), true);
    }
    engine.enqueue_declared_attacker_triggers(&[
        "goblin-banneret".to_string(),
        "weaker".to_string(),
        "equal".to_string(),
    ]);
    assert_eq!(engine.state.stack.len(), 1, "one mentor trigger");
    engine
        .commit_pending_triggered_targets(&mut EmeritusDecisionProvider)
        .expect("the target is chosen");
    // The 3-power source: only the attacking 2-power creature is legal.
    assert_eq!(
        engine.state.stack[0].targets.values().collect::<Vec<_>>(),
        vec![&TargetRef::Permanent {
            instance_id: "weaker".to_string()
        }]
    );
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("mentor resolves");
    assert_eq!(counters(&engine, "weaker"), 1);
    assert_eq!(counters(&engine, "equal"), 0);

    // Outside a target domain nothing binds the source's value: a static
    // filter that reads it stays non-executable instead of comparing to 0.
    let compilation = compile_playable_card(PlayableCardInput {
        id: "near-miss".to_string(),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        oracle: OracleCardParseRequest {
            card_name: "Near Miss".to_string(),
            type_line: "Creature \u{2014} Test".to_string(),
            mana_cost: Some("{1}{W}".to_string()),
            oracle_text: Some(
                "Creatures with power less than this creature's power can't block.".to_string(),
            ),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the card compiles");
    assert_ne!(compilation.engine_status, "executable");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn death_trigger_guards_read_the_dead_creature_through_the_trigger_frame() {
    // CR 603.4/603.10a: `it` / `its` in the intervening `if` is the event's
    // creature (no new code: the guard reads the C2 trigger frame), seen as
    // it last existed on the battlefield.
    let blowfly = compiled(
        "Blowfly Infestation",
        "Enchantment",
        ("0", "0"),
        "Whenever a creature dies, if it had a -1/-1 counter on it, put a -1/-1 counter on target creature.",
    );
    let massacre = compiled(
        "Massacre Girl, Known Killer",
        "Legendary Creature \u{2014} Human Assassin",
        ("4", "4"),
        "Whenever a creature an opponent controls dies, if its toughness was less than 1, draw a card.",
    );
    let killer = StackObject {
        id: "stack:killer".to_string(),
        controller: "player-1".to_string(),
        card: creature("killer", "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let sized = |id: &str, toughness: &str| {
        let mut definition = test_definition(id, "Creature \u{2014} Test");
        definition.power = Some("1".to_string());
        definition.toughness = Some(toughness.to_string());
        test_instance(id, definition, "player-1")
    };
    let published_on_death = |source: CardInstance, victim: CardInstance| {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![source, creature("bystander", "player-0")];
        engine.state.players[1].battlefield = vec![victim.clone()];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        engine
            .destroy_permanent_from_stack(&victim.instance_id, &killer)
            .expect("the destruction resolves");
        engine.state.stack.len()
    };
    let mut marked = sized("marked", "3");
    marked.counters.insert("-1/-1".to_string(), 1);
    let blowfly_source = || test_instance("blowfly-infestation", blowfly.clone(), "player-0");
    assert_eq!(published_on_death(blowfly_source(), marked), 1);
    assert_eq!(published_on_death(blowfly_source(), sized("plain", "3")), 0);

    let massacre_source = || test_instance("massacre-girl", massacre.clone(), "player-0");
    assert_eq!(
        published_on_death(massacre_source(), sized("withered", "0")),
        1
    );
    assert_eq!(
        published_on_death(massacre_source(), sized("sturdy", "2")),
        0
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn designations_are_given_once_and_read_by_conditions_selectors_and_last_known_deaths() {
    // Renown 1 (CR 702.112a-b): the trigger's `if it isn't renowned` and
    // `it becomes renowned` read and give the registry's designation.
    let renowned = compiled(
        "Topan Freeblade",
        "Creature \u{2014} Human Soldier",
        ("2", "2"),
        "Vigilance\nRenown 1 (When this creature deals combat damage to a player, if it isn't renowned, put a +1/+1 counter on it and it becomes renowned.)",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![test_instance("topan-freeblade", renowned, "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for _ in 0..2 {
        engine.enqueue_combat_damage_to_player_triggers("topan-freeblade", "player-1", 2);
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut EmeritusDecisionProvider)
                .expect("renown resolves");
        }
    }
    // Only the first hit grows it: the second trigger is not put on the
    // stack because it is already renowned (CR 603.4).
    assert_eq!(counters(&engine, "topan-freeblade"), 1);
    let (player, index) = engine.permanent_position("topan-freeblade").expect("alive");
    assert_eq!(
        engine.state.players[player].battlefield[index]
            .flags
            .get("renowned"),
        Some(&true)
    );

    // Selector (sibling, no new code): only the renowned creature is a
    // legal target; a static condition reads the source's designation.
    let slayer = compiled(
        "Renown Slayer",
        "Sorcery",
        ("0", "0"),
        "Destroy target renowned creature.",
    );
    let candidates = &slayer.rules[0]["declaration"]["decisions"][0]["candidates"];
    engine.state.players[1]
        .battlefield
        .push(creature("plain", "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert_eq!(
        engine.target_candidates("player-1", candidates),
        Some(vec![TargetRef::Permanent {
            instance_id: "topan-freeblade".to_string()
        }])
    );

    // A death trigger reads the designation the creature had when it died.
    let mourner = compiled(
        "Mourner",
        "Creature \u{2014} Elf",
        ("1", "1"),
        "Whenever another creature you control dies, if it was renowned, draw a card.",
    );
    engine.state.players[0]
        .battlefield
        .push(test_instance("mourner", mourner, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let killer = StackObject {
        id: "stack:killer".to_string(),
        controller: "player-1".to_string(),
        card: creature("killer", "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .destroy_permanent_from_stack("topan-freeblade", &killer)
        .expect("the destruction resolves");
    assert_eq!(engine.state.stack.len(), 1, "the renowned creature died");
}

/// Answers an optional entry replacement (`you may have ...`).
struct EntryChoice(bool);

impl DecisionProvider for EntryChoice {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let wanted = format!(":{}", self.0);
        Ok(request
            .options
            .iter()
            .position(|option| option.id.ends_with(&wanted))
            .unwrap_or(0))
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn unleash_lets_its_controller_choose_the_counter_that_stops_it_blocking() {
    // CR 702.98a: an optional entry replacement and a static ability whose
    // condition is the source's +1/+1 counter possession.
    let chainwalker = compiled(
        "Gore-House Chainwalker",
        "Creature \u{2014} Human Warrior",
        ("2", "1"),
        "Unleash (You may have this creature enter with a +1/+1 counter on it. It can't block as long as it has a +1/+1 counter on it.)",
    );
    for unleashed in [true, false] {
        let mut engine = test_engine(2);
        let mut entering = test_instance("gore-house-chainwalker", chainwalker.clone(), "player-0");
        engine
            .apply_enter_replacements(&mut entering, &mut EntryChoice(unleashed))
            .expect("the creature enters");
        assert_eq!(
            entering.counters.get("+1/+1").copied().unwrap_or(0),
            i32::from(unleashed)
        );
        engine.state.players[0].battlefield.push(entering);
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let (player, index) = engine
            .permanent_position("gore-house-chainwalker")
            .expect("entered");
        let permanent = engine.state.players[player].battlefield[index].clone();
        assert_eq!(
            engine.permanent_has_keyword(&permanent, "cantBlock"),
            unleashed,
            "unleashed: {unleashed}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn source_counter_possession_gates_statics_and_activations_without_new_code() {
    // The static condition `this PERMANENT has N or more KIND counters on it`
    // (Unleash's condition) composes with a continuous effect over other
    // creatures and with an activation restriction.
    let ascension = compiled(
        "Beastmaster Ascension",
        "Enchantment",
        ("0", "0"),
        "Whenever a creature you control attacks, you may put a quest counter on this enchantment.\nAs long as this enchantment has seven or more quest counters on it, creatures you control get +5/+5.",
    );
    for (quest, expected) in [(6, (2, 2)), (7, (7, 7))] {
        let mut engine = test_engine(2);
        let mut source = test_instance("beastmaster-ascension", ascension.clone(), "player-0");
        source.counters.insert("quest".to_string(), quest);
        engine.state.players[0].battlefield = vec![source, creature("bear", "player-0")];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        assert_eq!(power_toughness(&engine, "bear"), expected, "{quest} quest");
    }

    let tablet = compiled(
        "Tablet of Compleation",
        "Artifact",
        ("0", "0"),
        "{1}, {T}: Draw a card. Activate only if this artifact has five or more oil counters on it.",
    );
    for (oil, allowed) in [(4, false), (5, true)] {
        let mut engine = mana_engine(&[("forest-a", "{G}")]);
        let mut source = test_instance("tablet-of-compleation", tablet.clone(), "player-0");
        source.counters.insert("oil".to_string(), oil);
        engine.state.players[0].battlefield.push(source);
        engine.state.players[0].library = vec![creature("drawn", "player-0")];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        assert_eq!(
            activate(&mut engine, "tablet-of-compleation"),
            allowed,
            "{oil} oil"
        );
    }
}
