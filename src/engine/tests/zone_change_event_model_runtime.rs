//! Zone-change family of the generic event model
//! (docs/oracle-event-model.md): one record per move with the object's
//! last-known information, one matcher, look-back departures (CR 603.10a).
use super::*;

fn parsed_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {:?}", parsed.abilities);
    parsed
        .abilities
        .iter()
        .map(|ability| {
            let rule = ability.rule.clone().expect("canonical rule");
            assert!(rule_is_executable(&rule), "{text}: {rule}");
            rule
        })
        .collect()
}

fn object(id: &str, type_line: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    test_instance(id, definition, owner)
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn stack_sources(engine: &GameEngine) -> Vec<String> {
    engine
        .state
        .stack
        .iter()
        .map(|object| object.card.instance_id.clone())
        .collect()
}

#[test]
fn a_death_is_observed_with_the_last_known_controller_and_power() {
    // A creature owned by player 1 and controlled by player 0 dies while
    // pumped: observers read its last controller and power (CR 603.10a).
    let observer_rules = parsed_rules(
        "Death Watcher",
        "Enchantment",
        "Whenever a creature you control dies, draw a card.",
    );
    assert_eq!(observer_rules[0]["event"]["kind"], "permanentDied");
    let mut engine = test_engine(2);
    let mut stolen = object("stolen", "Creature — Bear", Vec::new(), "player-1");
    stolen.controller = "player-0".to_string();
    stolen.power_modifier = 3;
    engine.state.players[0].battlefield = vec![
        stolen,
        object(
            "watcher-0",
            "Enchantment",
            observer_rules.clone(),
            "player-0",
        ),
    ];
    engine.state.players[1].battlefield = vec![object(
        "watcher-1",
        "Enchantment",
        observer_rules,
        "player-1",
    )];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("stolen", "destroyed")
        .expect("the creature dies");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "stolen")
    );
    // Only the last controller's observer triggers, with the last power.
    assert_eq!(stack_sources(&engine), ["watcher-0"]);
    let decisions = &engine.state.stack[0].decisions;
    assert_eq!(decisions["triggeringPermanentController"], "player-0");
    assert_eq!(decisions["triggeringPermanentOwner"], "player-1");
    assert_eq!(decisions["triggeringPermanentPower"], 5);
}

#[test]
fn a_departure_of_any_controllers_permanent_reads_the_move_pattern() {
    // Dingus Egg's event with a plain body.
    let rules = parsed_rules(
        "Dingus Probe",
        "Artifact",
        "Whenever a land is put into a graveyard from the battlefield, you gain 2 life.",
    );
    assert_eq!(
        rules[0]["event"]["subject"]["object"]["kind"], "permanents",
        "{}",
        rules[0]["event"]
    );
    assert_eq!(rules[0]["event"]["from"], "battlefield");
    assert_eq!(rules[0]["event"]["to"], "graveyard");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![object("egg", "Artifact", rules, "player-0")];
    engine.state.players[1].battlefield = vec![
        object("land", "Land — Forest", Vec::new(), "player-1"),
        object("exiled-land", "Land — Forest", Vec::new(), "player-1"),
        object("bear", "Creature — Bear", Vec::new(), "player-1"),
    ];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("bear", "destroyed")
        .expect("a creature dies");
    engine
        .exile_permanent("exiled-land")
        .expect("a land is exiled");
    assert!(engine.state.stack.is_empty(), "near misses do not trigger");
    engine
        .move_permanent_to_graveyard("land", "destroyed")
        .expect("an opponent's land is destroyed");
    assert_eq!(stack_sources(&engine), ["egg"]);
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPermanentController"],
        "player-1"
    );
}

#[test]
fn a_cards_own_from_anywhere_ability_triggers_as_it_arrives() {
    // The Eldrazi titans' event, with a plain body.
    let rules = parsed_rules(
        "Titan Probe",
        "Creature — Eldrazi",
        "When this creature is put into a graveyard from anywhere, you gain 3 life.",
    );
    assert_eq!(
        rules[0]["event"],
        json!({"kind": "eventPattern", "lemma": "move",
            "subject": {"object": {"kind": "self"}}, "to": "graveyard"})
    );
    // From the library.
    let mut engine = test_engine(2);
    engine.state.players[0].library.push(object(
        "titan",
        "Creature — Eldrazi",
        rules.clone(),
        "player-0",
    ));
    admit(&mut engine);
    engine.mill_cards("player-0", 1, None).expect("mill");
    assert_eq!(stack_sources(&engine), ["titan"]);
    // From the battlefield.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![object("titan", "Creature — Eldrazi", rules, "player-0")];
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("titan", "destroyed")
        .expect("the titan dies");
    assert_eq!(stack_sources(&engine), ["titan"]);
    // Another card's arrival is not its event.
    let mut engine = test_engine(2);
    engine.state.players[0].library.push(object(
        "other",
        "Creature — Bear",
        Vec::new(),
        "player-0",
    ));
    admit(&mut engine);
    engine.mill_cards("player-0", 1, None).expect("mill");
    assert!(engine.state.stack.is_empty());
}

#[test]
fn historical_departure_kinds_keep_their_observers() {
    // `permanentLeftBattlefield` (self) and `controlledPermanentLeftBattlefield`.
    let own = parsed_rules(
        "Leaver",
        "Creature — Spirit",
        "When this creature leaves the battlefield, draw a card.",
    );
    assert_eq!(own[0]["event"]["kind"], "permanentLeftBattlefield");
    let watcher = parsed_rules(
        "Departure Watcher",
        "Enchantment",
        "Whenever a creature you control leaves the battlefield, draw a card.",
    );
    assert_eq!(
        watcher[0]["event"]["kind"],
        "controlledPermanentLeftBattlefield"
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        object("leaver", "Creature — Spirit", own, "player-0"),
        object("watcher", "Enchantment", watcher, "player-0"),
    ];
    admit(&mut engine);
    engine.exile_permanent("leaver").expect("exiled");
    let mut sources = stack_sources(&engine);
    sources.sort();
    assert_eq!(sources, ["leaver", "watcher"]);
}
