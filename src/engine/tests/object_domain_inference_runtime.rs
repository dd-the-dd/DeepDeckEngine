//! Roadmap #1 (CR 109.2a, 205.3d): subtype heads compiled from real Oracle
//! text select exactly the permanents with that subtype, the `other` field
//! of the collection frame excludes only the source, and a subtype target
//! accepts only permanents carrying it.
use super::*;

fn compiled(name: &str, type_line: &str, oracle: &str) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace(' ', "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{1}{G}".to_string()),
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

fn creature(id: &str, type_line: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
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

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn bare_subtype_collections_select_controlled_permanents_with_that_subtype() {
    let lord = compiled(
        "Elf Lord Probe",
        "Creature \u{2014} Elf Warrior",
        "Other Elves you control get +1/+1.",
    );
    let warchief = compiled(
        "Goblin Warchief Probe",
        "Creature \u{2014} Goblin Warrior",
        "Goblins you control have haste.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("lord", lord, "player-0"),
        test_instance("warchief", warchief, "player-0"),
        creature("own-elf", "Creature \u{2014} Elf Druid", "player-0"),
        creature("own-bear", "Creature \u{2014} Bear", "player-0"),
        creature("own-goblin", "Creature \u{2014} Goblin", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![
        creature("their-elf", "Creature \u{2014} Elf", "player-1"),
        creature("their-goblin", "Creature \u{2014} Goblin", "player-1"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    // `other`: the lord itself is excluded; only Elves its controller
    // controls get the bonus.
    assert_eq!(power_toughness(&engine, "lord"), (2, 2));
    assert_eq!(power_toughness(&engine, "own-elf"), (3, 3));
    assert_eq!(power_toughness(&engine, "own-bear"), (2, 2));
    assert_eq!(power_toughness(&engine, "their-elf"), (2, 2));

    let haste = |id: &str| {
        let (player, index) = engine.permanent_position(id).expect("on the battlefield");
        engine.permanent_has_keyword(&engine.state.players[player].battlefield[index], "haste")
    };
    // No `other`: the Warchief is one of the Goblins it names.
    assert!(haste("warchief"));
    assert!(haste("own-goblin"));
    assert!(!haste("own-bear"));
    assert!(!haste("their-goblin"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_subtype_target_accepts_only_permanents_with_that_subtype() {
    let spell = compiled("Wall Breaker Probe", "Instant", "Destroy target Wall.");
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = vec![
        FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        },
        FloatingMana {
            symbol: "G".to_string(),
            spend_restriction: None,
        },
    ];
    engine.state.players[0].hand = vec![test_instance("wall-breaker", spell, "player-0")];
    engine.state.players[1].battlefield = vec![
        creature("wall", "Creature \u{2014} Wall", "player-1"),
        creature("bear", "Creature \u{2014} Bear", "player-1"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let casts = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("wall-breaker")
        })
        .collect::<Vec<_>>();
    assert_eq!(casts.len(), 1, "only the Wall is a legal target");
    engine
        .apply_priority_action(&casts[0], &mut RandomAi::seeded(7))
        .expect("cast the destruction spell");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(8))
        .expect("resolve destroyPermanent");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "wall")
    );
    assert!(engine.permanent_position("bear").is_some());
}

/// Casts a target-free spell from player 0's hand and resolves it.
fn cast_and_resolve(engine: &mut GameEngine, id: &str, spell: CardDefinition) {
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = ["C", "G"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect();
    engine.state.players[0].hand = vec![test_instance(id, spell, "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell && action.card_instance_id.as_deref() == Some(id)
        })
        .expect("the spell can be cast");
    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(11))
        .expect("cast the spell");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(12))
        .expect("resolve the spell");
}

fn equipment(id: &str, controller: &str) -> CardInstance {
    test_instance(
        id,
        test_definition(id, "Artifact \u{2014} Equipment"),
        controller,
    )
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn source_counts_exclude_only_the_source_and_scale_damage() {
    // `for each other Elf you control`: the source is the modified object.
    let lord = compiled(
        "Elf Counter Probe",
        "Creature \u{2014} Elf",
        "This creature gets +1/+1 for each other Elf you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("lord", lord, "player-0"),
        creature("elf-a", "Creature \u{2014} Elf", "player-0"),
        creature("elf-b", "Creature \u{2014} Elf", "player-0"),
    ];
    engine.state.players[1].battlefield =
        vec![creature("their-elf", "Creature \u{2014} Elf", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert_eq!(power_toughness(&engine, "lord"), (4, 4));

    // `for each Equipment you control` as a damage amount.
    let spell = compiled(
        "Probe",
        "Sorcery",
        "Probe deals 1 damage to each opponent for each Equipment you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        equipment("sword", "player-0"),
        equipment("boots", "player-0"),
        creature("bear", "Creature \u{2014} Bear", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![equipment("their-sword", "player-1")];
    let opponent = engine.state.players[1].life;
    cast_and_resolve(&mut engine, "probe", spell);
    assert_eq!(engine.state.players[1].life, opponent - 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn bare_subtype_sweeps_reach_exactly_the_permanents_with_that_subtype() {
    let goblins = || {
        vec![
            creature("own-goblin", "Creature \u{2014} Goblin", "player-0"),
            creature("own-bear", "Creature \u{2014} Bear", "player-0"),
        ]
    };
    let their_goblin = || creature("their-goblin", "Creature \u{2014} Goblin", "player-1");

    // `Destroy all Goblins.`
    let spell = compiled("Goblin Sweep Probe", "Sorcery", "Destroy all Goblins.");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = goblins();
    engine.state.players[1].battlefield = vec![their_goblin()];
    cast_and_resolve(&mut engine, "sweep", spell);
    assert!(engine.permanent_position("own-goblin").is_none());
    assert!(engine.permanent_position("their-goblin").is_none());
    assert!(engine.permanent_position("own-bear").is_some());

    // `Put a +1/+1 counter on each Vampire you control.`
    let spell = compiled(
        "Vampire Counter Probe",
        "Sorcery",
        "Put a +1/+1 counter on each Vampire you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature("own-vampire", "Creature \u{2014} Vampire", "player-0"),
        creature("own-bear", "Creature \u{2014} Bear", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![creature(
        "their-vampire",
        "Creature \u{2014} Vampire",
        "player-1",
    )];
    cast_and_resolve(&mut engine, "counters", spell);
    assert_eq!(power_toughness(&engine, "own-vampire"), (3, 3));
    assert_eq!(power_toughness(&engine, "own-bear"), (2, 2));
    assert_eq!(power_toughness(&engine, "their-vampire"), (2, 2));
}
