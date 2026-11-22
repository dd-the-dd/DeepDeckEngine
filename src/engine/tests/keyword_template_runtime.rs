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
