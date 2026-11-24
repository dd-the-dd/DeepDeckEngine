//! Runtime proofs of the clause-layer object-subject cells and action
//! adjuncts: the Prepared designation, the defender exception, the
//! regeneration prohibition, a source returned with entry counters (and the
//! finality counter it carries), and a repeated keyword action. Each case
//! starts from Oracle text, so the parser-to-runtime path is the one proved.
use super::*;

fn parse(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("clause-subject-cells-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
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

/// The resolving object of an ability whose source is `source`.
fn ability_of(source: CardInstance) -> StackObject {
    StackObject {
        id: format!("stack:{}", source.instance_id),
        controller: source.controller.clone(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// Resolves through the shared resolution-start fixture path: the ability's
/// source copy adopts its admitted incarnation and declared targets are
/// checked for legality, as a real resolution does (CR 608.2b).
fn resolve(engine: &mut GameEngine, effects: &Value, stack_object: &StackObject) {
    let mut decisions = stack_object.decisions.clone();
    execute_effects_at_resolution_start(
        engine,
        effects,
        stack_object,
        &mut BTreeMap::new(),
        &mut decisions,
        &mut RandomAi::seeded(5),
    )
    .expect("the effects resolve");
}

fn battlefield_ids(engine: &GameEngine, player: usize) -> Vec<String> {
    engine.state.players[player]
        .battlefield
        .iter()
        .map(|card| card.instance_id.clone())
        .collect()
}

/// `This creature can attack this turn as though it didn't have defender.`
/// (Wall of One Thousand Cuts): the marker lifts the defender restriction
/// for the turn; without it the same creature cannot attack.
#[test]
fn defender_exception_marker_lets_the_wall_attack_this_turn() {
    let rules = parse(
        "Wall of One Thousand Cuts",
        "Creature - Wall",
        "{3}{W}{W}",
        "Defender, flying\n{W}: This creature can attack this turn as though it didn't have defender.",
    );
    let mut engine = test_engine(2);
    engine.state.step = GameStep::DeclareAttackers;
    let mut definition = test_definition("wall", "Creature - Wall");
    definition.rules = rules.clone();
    let mut wall = test_instance("wall", definition, "player-0");
    wall.summoning_sick = false;
    engine.state.players[0].battlefield.push(wall.clone());
    let can_attack = |engine: &GameEngine| {
        engine
            .legal_attacker_actions(&BTreeSet::new())
            .iter()
            .any(|action| {
                action.kind == ActionKind::DeclareAttacker
                    && action.attacker_id.as_deref() == Some("wall")
            })
    };
    assert!(!can_attack(&engine), "defender forbids the attack");
    let activated = rules
        .iter()
        .find(|rule| rule["kind"] == "activatedAbility")
        .expect("the activated defender exception");
    resolve(&mut engine, &activated["effects"], &ability_of(wall));
    assert!(
        can_attack(&engine),
        "the exception lifts defender this turn"
    );
}

/// `Target creature can't be regenerated this turn.` (Furnace Brood): the
/// shield stays unused and the creature is destroyed; without the marker
/// the same shield regenerates it.
#[test]
fn regeneration_prohibition_leaves_the_shield_unused() {
    let rules = parse(
        "Furnace Brood",
        "Creature - Elemental",
        "{3}{R}",
        "{R}: Target creature can't be regenerated this turn.",
    );
    let grant = rules[0]["effects"][0].clone();
    assert_eq!(grant["keyword"], "cantBeRegenerated");
    let destroy = json!([{
        "kind": "destroyPermanent",
        "permanent": { "kind": "eachPermanent", "where": { "kind": "cardTypeContains", "value": "Creature" } },
    }]);
    for prohibited in [false, true] {
        let mut engine = test_engine(2);
        let victim = test_instance(
            "victim",
            test_definition("victim", "Creature - Troll"),
            "player-1",
        );
        engine.state.players[1].battlefield.push(victim.clone());
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        engine.install_regeneration_shield("victim");
        if prohibited {
            // The lowered grant on the declared target, resolved with that
            // target chosen.
            let mut stack_object = ability_of(test_instance(
                "brood",
                test_definition("brood", "Creature - Elemental"),
                "player-0",
            ));
            let target_id = grant["object"]["id"]
                .as_str()
                .expect("a chosen target")
                .to_string();
            stack_object.targets.insert(
                target_id,
                TargetRef::Permanent {
                    instance_id: "victim".to_string(),
                },
            );
            resolve(&mut engine, &json!([grant.clone()]), &stack_object);
        }
        let sorcery = ability_of(test_instance(
            "sorcery",
            test_definition("sorcery", "Sorcery"),
            "player-0",
        ));
        resolve(&mut engine, &destroy, &sorcery);
        assert_eq!(
            battlefield_ids(&engine, 1).contains(&"victim".to_string()),
            !prohibited,
            "prohibited: {prohibited}"
        );
    }
}

/// `Return this card from your graveyard to the battlefield with a finality
/// counter on it.` (Gastal Thrillroller): the card enters with the counter;
/// destroyed later, it is exiled instead of being put into the graveyard.
#[test]
fn a_returned_source_enters_with_finality_and_is_exiled_instead_of_dying() {
    let rules = parse(
        "Finality Witness",
        "Creature - Zombie",
        "{B}",
        "{2}{B}: Return this card from your graveyard to the battlefield with a finality counter on it.",
    );
    let mut engine = test_engine(2);
    let mut definition = test_definition("witness", "Creature - Zombie");
    definition.rules = rules.clone();
    let card = test_instance("witness", definition, "player-0");
    engine.state.players[0].graveyard.push(card.clone());
    resolve(&mut engine, &rules[0]["effects"], &ability_of(card));
    let returned = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "witness")
        .expect("the source returned to the battlefield");
    assert_eq!(returned.counters.get("finality"), Some(&1));

    let sorcery = ability_of(test_instance(
        "sorcery",
        test_definition("sorcery", "Sorcery"),
        "player-1",
    ));
    resolve(
        &mut engine,
        &json!([{
            "kind": "destroyPermanent",
            "permanent": { "kind": "eachPermanent", "where": { "kind": "cardTypeContains", "value": "Creature" } },
        }]),
        &sorcery,
    );
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "witness"),
        "the finality counter exiles it"
    );
    assert!(
        !engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "witness"),
        "it never reaches the graveyard"
    );
}

/// `Investigate twice.` (Wavesifter's trigger): two separate investigations,
/// two Clues; the count is the repetition, not a token quantity.
#[test]
fn investigate_twice_performs_two_investigations() {
    let rules = parse(
        "Repetition Witness",
        "Sorcery",
        "{1}{U}",
        "Investigate twice.",
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "repeatEffects");
    assert_eq!(rules[0]["effects"][0]["effects"][0]["quantity"]["value"], 1);
    let mut engine = test_engine(2);
    let spell = ability_of(test_instance(
        "spell",
        test_definition("spell", "Sorcery"),
        "player-0",
    ));
    resolve(&mut engine, &rules[0]["effects"], &spell);
    let clues = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|card| card.definition.name == "Clue")
        .count();
    assert_eq!(clues, 2);
}

/// `This creature becomes prepared.` (Kirol, Tam): the lowered designation
/// is the engine's `setPrepared`, which sets the prepared flag.
#[test]
fn a_source_becomes_prepared_through_set_prepared() {
    let rules = parse(
        "Prepared Witness",
        "Creature - Wizard",
        "{1}{U}",
        "{1}: This creature becomes prepared.",
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "setPrepared");
    let mut engine = test_engine(2);
    let mut definition = test_definition("prepared-witness", "Creature - Wizard");
    definition.rules = rules.clone();
    let witness = test_instance("prepared-witness", definition, "player-0");
    engine.state.players[0].battlefield.push(witness.clone());
    resolve(&mut engine, &rules[0]["effects"], &ability_of(witness));
    assert_eq!(
        engine.state.players[0].battlefield[0].flags.get("prepared"),
        Some(&true)
    );
}
