//! One combat clause family compiled from Oracle text for every subject
//! (source, attachment, battlefield collection, temporary effect objects)
//! and enforced at attack and block declaration (CR 508.1c-d, 509.1b-c).
use super::*;

fn parse(name: &str, type_line: &str, oracle: &str) -> crate::oracle::OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("combat-restriction-family-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

/// The executable rules of a printed card; each must pass the engine's own
/// support predicate.
fn compiled(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let parsed = parse(name, type_line, oracle);
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    let rules = parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.clone().expect("canonical ability has a rule"))
        .collect::<Vec<_>>();
    assert!(rules.iter().all(rule_is_executable), "{oracle}: {rules:?}");
    rules
}

fn permanent(id: &str, type_line: &str, owner: &str, rules: Vec<Value>) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    let mut card = test_instance(id, definition, owner);
    card.summoning_sick = false;
    card
}

fn creature(id: &str, owner: &str) -> CardInstance {
    permanent(id, "Creature - Soldier", owner, Vec::new())
}

fn flyer(id: &str, owner: &str) -> CardInstance {
    permanent(
        id,
        "Creature - Bird",
        owner,
        vec![json!({ "kind": "flying" })],
    )
}

fn aura(id: &str, owner: &str, enchanted: &str, rules: Vec<Value>) -> CardInstance {
    let mut aura = permanent(id, "Enchantment - Aura", owner, rules);
    aura.attached_to = Some(enchanted.to_string());
    aura
}

fn can_attack(engine: &GameEngine, attacker: &str) -> bool {
    engine
        .legal_attacker_actions(&BTreeSet::new())
        .iter()
        .any(|action| {
            action.kind == ActionKind::DeclareAttacker
                && action.attacker_id.as_deref() == Some(attacker)
        })
}

fn attack_defenders(engine: &GameEngine, attacker: &str) -> BTreeSet<String> {
    engine
        .legal_attacker_actions(&BTreeSet::new())
        .iter()
        .filter(|action| action.attacker_id.as_deref() == Some(attacker))
        .filter_map(|action| match action.targets.get("defender") {
            Some(TargetRef::Player { player_id }) => Some(player_id.clone()),
            Some(TargetRef::Permanent { instance_id }) => Some(instance_id.clone()),
            _ => None,
        })
        .collect()
}

fn may_finish_attackers(engine: &GameEngine) -> bool {
    engine
        .legal_attacker_actions(&BTreeSet::new())
        .iter()
        .any(|action| action.kind == ActionKind::FinishAttackers)
}

fn attacking(engine: &mut GameEngine, attacker: &str, defending_player: &str) {
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: attacker.to_string(),
        defender: TargetRef::Player {
            player_id: defending_player.to_string(),
        },
        defending_player_id: defending_player.to_string(),
    });
}

fn battlefield_card(engine: &GameEngine, id: &str) -> CardInstance {
    let (player_index, card_index) = engine
        .permanent_position(id)
        .expect("card is on battlefield");
    engine.state.players[player_index].battlefield[card_index].clone()
}

fn resolve_spell(engine: &mut GameEngine, rule: &Value, targets: BTreeMap<String, TargetRef>) {
    let stack_object = StackObject {
        id: "stack:combat-clause".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "combat-clause-spell",
            test_definition("combat-clause-spell", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    };
    let decisions = stack_object.decisions.clone();
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        &stack_object,
        &mut BTreeMap::new(),
        &mut decisions.clone(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the combat clause resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_aura_prohibition_reaches_only_the_enchanted_creature() {
    // Pacifism: the attachment subject shares the source frame's clause.
    let mut engine = test_engine(2);
    let rules = compiled(
        "Pacifism",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature can't attack or block.",
    );
    engine.state.players[0].battlefield = vec![
        creature("pacified", "player-0"),
        creature("free", "player-0"),
        aura("pacifism", "player-1", "pacified", rules),
    ];
    assert!(!can_attack(&engine, "pacified"));
    assert!(can_attack(&engine, "free"));
    let pacified = battlefield_card(&engine, "pacified");
    assert!(engine.permanent_has_keyword(&pacified, "cantBlock"));
    assert!(!engine.permanent_has_keyword(&battlefield_card(&engine, "free"), "cantBlock"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_collection_prohibition_reads_effective_keywords_at_declaration() {
    // Moat: `creatures without flying` is the shared query's keyword
    // complement, observed on granted abilities as well as printed ones.
    let mut engine = test_engine(2);
    let moat = compiled(
        "Moat",
        "Enchantment",
        "Creatures without flying can't attack.",
    );
    let wings = compiled(
        "Wings",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature has flying.",
    );
    engine.state.players[1].battlefield = vec![permanent("moat", "Enchantment", "player-1", moat)];
    engine.state.players[0].battlefield = vec![
        creature("walker", "player-0"),
        flyer("bird", "player-0"),
        creature("winged", "player-0"),
        aura("wings", "player-0", "winged", wings),
    ];
    assert!(!can_attack(&engine, "walker"));
    assert!(can_attack(&engine, "bird"));
    assert!(can_attack(&engine, "winged"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_granted_attack_requirement_withholds_finishing_while_the_attack_is_available() {
    // Angler Turtle: the requirement applies to the opponent's creatures only.
    let mut engine = test_engine(2);
    let turtle = compiled(
        "Angler Turtle",
        "Creature - Turtle",
        "Creatures your opponents control attack each combat if able.",
    );
    engine.state.players[1].battlefield =
        vec![permanent("turtle", "Creature - Turtle", "player-1", turtle)];
    engine.state.players[0].battlefield = vec![creature("compelled", "player-0")];
    assert!(can_attack(&engine, "compelled"));
    assert!(!may_finish_attackers(&engine));

    // A creature that cannot attack is not required to (CR 508.1d).
    engine.state.players[0].battlefield[0].tapped = true;
    assert!(may_finish_attackers(&engine));

    // The turtle's own controller is unaffected.
    engine.state.players[0].battlefield[0].tapped = false;
    engine.state.active_player = 1;
    assert!(may_finish_attackers(&engine));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn defender_restrictions_remove_only_the_protected_defenders() {
    let mut engine = test_engine(3);
    engine.state.active_player = 1;
    // Spectral Grasp protects its controller and that player's planeswalkers.
    let grasp = compiled(
        "Spectral Grasp",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature can't attack you or planeswalkers you control.",
    );
    engine.state.players[0].battlefield = vec![
        aura("grasp", "player-0", "grasped", grasp),
        permanent(
            "walker-0",
            "Legendary Planeswalker - Jace",
            "player-0",
            Vec::new(),
        ),
    ];
    engine.state.players[1].battlefield = vec![
        creature("grasped", "player-1"),
        creature("unbound", "player-1"),
    ];
    let defenders = attack_defenders(&engine, "grasped");
    assert!(!defenders.contains("player-0"));
    assert!(!defenders.contains("walker-0"));
    assert!(defenders.contains("player-2"));
    let unbound = attack_defenders(&engine, "unbound");
    assert!(unbound.contains("player-0") && unbound.contains("walker-0"));

    // Blazing Archon protects only its controller, not its planeswalkers.
    let archon = compiled(
        "Blazing Archon",
        "Creature - Archon",
        "Creatures can't attack you.",
    );
    engine.state.players[0].battlefield = vec![
        permanent("archon", "Creature - Archon", "player-0", archon),
        permanent(
            "walker-0",
            "Legendary Planeswalker - Jace",
            "player-0",
            Vec::new(),
        ),
    ];
    let defenders = attack_defenders(&engine, "unbound");
    assert!(!defenders.contains("player-0"));
    assert!(defenders.contains("walker-0"));
    assert!(defenders.contains("player-2"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn block_only_and_requirement_and_evasion_share_the_attachment_subject() {
    // Air Bladder: `can block only creatures with flying`.
    let mut engine = test_engine(2);
    let bladder = compiled(
        "Air Bladder",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature has flying.\nEnchanted creature can block only creatures with flying.",
    );
    engine.state.players[0].battlefield =
        vec![creature("ground", "player-0"), flyer("bird", "player-0")];
    engine.state.players[1].battlefield = vec![
        creature("bladdered", "player-1"),
        aura("bladder", "player-1", "bladdered", bladder),
    ];
    attacking(&mut engine, "ground", "player-1");
    attacking(&mut engine, "bird", "player-1");
    let blocker = battlefield_card(&engine, "bladdered");
    assert!(!engine.can_block(&blocker, &battlefield_card(&engine, "ground")));
    assert!(engine.can_block(&blocker, &battlefield_card(&engine, "bird")));

    // Fealty to the Realm: a requirement and a defender restriction on the
    // same attachment, a combination no earlier frame accepted.
    let mut engine = test_engine(3);
    engine.state.active_player = 1;
    let fealty = compiled(
        "Fealty to the Realm",
        "Enchantment - Aura",
        "Enchant creature\nEnchanted creature attacks each combat if able and can't attack you.",
    );
    engine.state.players[0].battlefield = vec![aura("fealty", "player-0", "loyal", fealty)];
    engine.state.players[1].battlefield = vec![creature("loyal", "player-1")];
    let defenders = attack_defenders(&engine, "loyal");
    assert_eq!(defenders, BTreeSet::from(["player-2".to_string()]));
    assert!(!may_finish_attackers(&engine));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_block_requirement_withholds_finishing_until_the_creature_blocks() {
    // Watchdog: `This creature blocks each combat if able.`
    let mut engine = test_engine(2);
    let watchdog = compiled(
        "Watchdog",
        "Artifact Creature - Dog",
        "This creature blocks each combat if able.",
    );
    engine.state.players[0].battlefield = vec![creature("raider", "player-0")];
    engine.state.players[1].battlefield = vec![permanent(
        "watchdog",
        "Artifact Creature - Dog",
        "player-1",
        watchdog,
    )];
    attacking(&mut engine, "raider", "player-1");
    let attackers = vec!["raider".to_string()];
    let finish = |actions: &[LegalAction]| {
        actions
            .iter()
            .any(|action| action.kind == ActionKind::FinishBlockers)
    };
    let first = engine.legal_blocker_actions(1, &attackers, &BTreeSet::new(), &BTreeMap::new());
    assert!(
        first
            .iter()
            .any(|action| action.blocker_id.as_deref() == Some("watchdog"))
    );
    assert!(!finish(&first));
    let blocks = BTreeMap::from([("raider".to_string(), vec!["watchdog".to_string()])]);
    let used = BTreeSet::from(["watchdog".to_string()]);
    assert!(finish(
        &engine.legal_blocker_actions(1, &attackers, &used, &blocks)
    ));

    // An unblockable attacker leaves no legal block, so nothing is required.
    let mut engine = test_engine(2);
    let sneak = compiled(
        "Sneak",
        "Creature - Rogue",
        "This creature can't be blocked.",
    );
    let watchdog = compiled(
        "Watchdog",
        "Artifact Creature - Dog",
        "This creature blocks each combat if able.",
    );
    engine.state.players[0].battlefield =
        vec![permanent("sneak", "Creature - Rogue", "player-0", sneak)];
    engine.state.players[1].battlefield = vec![permanent(
        "watchdog",
        "Artifact Creature - Dog",
        "player-1",
        watchdog,
    )];
    attacking(&mut engine, "sneak", "player-1");
    let actions = engine.legal_blocker_actions(
        1,
        &["sneak".to_string()],
        &BTreeSet::new(),
        &BTreeMap::new(),
    );
    assert!(finish(&actions));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_temporary_collection_prohibition_marks_the_objects_for_the_turn() {
    // Seismic Stomp: the same clause as a resolved effect on a collection.
    let mut engine = test_engine(2);
    let rule = compiled(
        "Seismic Stomp",
        "Sorcery",
        "Creatures without flying can't block this turn.",
    )
    .remove(0);
    engine.state.players[1].battlefield =
        vec![creature("ground", "player-1"), flyer("bird", "player-1")];
    resolve_spell(&mut engine, &rule, BTreeMap::new());
    assert!(engine.permanent_has_keyword(&battlefield_card(&engine, "ground"), "cantBlock"));
    assert!(!engine.permanent_has_keyword(&battlefield_card(&engine, "bird"), "cantBlock"));
    engine.state.players[0].battlefield = vec![creature("raider", "player-0")];
    attacking(&mut engine, "raider", "player-1");
    let actions = engine.legal_blocker_actions(
        1,
        &["raider".to_string()],
        &BTreeSet::new(),
        &BTreeMap::new(),
    );
    assert!(
        !actions
            .iter()
            .any(|action| action.blocker_id.as_deref() == Some("ground"))
    );
    assert!(
        actions
            .iter()
            .any(|action| action.blocker_id.as_deref() == Some("bird"))
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_temporary_requirement_with_an_inner_duration_marks_its_target() {
    // `Target creature blocks this turn if able.` (Shessra's requirement).
    let mut engine = test_engine(2);
    let rule = compiled(
        "Provocation",
        "Sorcery",
        "Target creature blocks this turn if able.",
    )
    .remove(0);
    engine.state.players[1].battlefield = vec![creature("provoked", "player-1")];
    let target_id = rule["declaration"]["decisions"]
        .as_array()
        .and_then(|decisions| decisions.first())
        .and_then(|decision| decision["id"].as_str())
        .expect("the requirement targets one creature")
        .to_string();
    resolve_spell(
        &mut engine,
        &rule,
        BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "provoked".to_string(),
            },
        )]),
    );
    assert!(engine.permanent_has_keyword(
        &battlefield_card(&engine, "provoked"),
        "blockEachCombatIfAble"
    ));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn unrepresentable_neighbours_of_the_family_stay_explicitly_unsupported() {
    for (type_line, oracle) in [
        // A fixed block capacity is now a `blockCapacity` modifier; one
        // scaled by a count has no counted modifier.
        (
            "Enchantment - Aura",
            "Enchant creature\nEnchanted creature can block an additional creature for each Forest you control.",
        ),
        // A defender restriction has no per-object temporary marker.
        (
            "Sorcery",
            "Until your next turn, creatures can't attack you.",
        ),
        // A blocker-attacker pair restriction is static only.
        (
            "Sorcery",
            "Target creature can't be blocked by Walls this turn.",
        ),
        // A standing requirement needs `each combat`.
        ("Enchantment", "Creatures you control attack if able."),
    ] {
        assert_ne!(
            parse("Neighbour", type_line, oracle).status,
            "canonical",
            "{oracle}"
        );
    }
}
