//! Object qualifiers (C4) compiled from Oracle text run on the engine's
//! status, relation and history atoms.
use super::*;

/// The executable definition of a card compiled from its Oracle text.
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

fn attachment(id: &str, type_line: &str, controller: &str, attached_to: &str) -> CardInstance {
    let mut card = test_instance(id, test_definition(id, type_line), controller);
    card.attached_to = Some(attached_to.to_string());
    card
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
fn status_registry_atoms_read_kicked_equipped_and_enchanted_permanents() {
    // `if it was kicked` (CR 702.33d): the permanent keeps its spell's status.
    let mystic = compiled(
        "Kicked Probe",
        "Creature \u{2014} Human",
        ("1", "1"),
        "When this creature enters, if it was kicked, draw a card.",
    );
    for kicked in [true, false] {
        let mut engine = test_engine(2);
        let mut card = test_instance("kicked-probe", mystic.clone(), "player-0");
        if kicked {
            card.flags.insert("kicked".to_string(), true);
        }
        engine.state.players[0].battlefield = vec![card];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let (player, index) = engine.permanent_position("kicked-probe").expect("entered");
        let live = engine.state.players[player].battlefield[index].clone();
        engine.enqueue_enter_battlefield_triggers(&live);
        assert_eq!(
            engine.state.stack.len(),
            usize::from(kicked),
            "kicked {kicked}"
        );
    }

    // `as long as this creature is enchanted or equipped` (CR 301.5, 303.4).
    let knight = compiled(
        "Attachment Probe",
        "Creature \u{2014} Human Knight",
        ("2", "2"),
        "As long as this creature is enchanted or equipped, this creature gets +1/+1.",
    );
    for attached in [
        None,
        Some("Artifact \u{2014} Equipment"),
        Some("Enchantment \u{2014} Aura"),
    ] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield =
            vec![test_instance("knight", knight.clone(), "player-0")];
        if let Some(type_line) = attached {
            engine.state.players[0]
                .battlefield
                .push(attachment("gear", type_line, "player-0", "knight"));
        }
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let expected = if attached.is_some() { (3, 3) } else { (2, 2) };
        assert_eq!(power_toughness(&engine, "knight"), expected, "{attached:?}");
    }

    // `target enchanted permanent`: only the enchanted one is a candidate.
    let bounce = compiled(
        "Bounce Probe",
        "Instant",
        ("0", "0"),
        "Return target enchanted permanent to its owner's hand.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        creature("bare", "player-1"),
        creature("charmed", "player-1"),
        attachment("aura", "Enchantment \u{2014} Aura", "player-1", "charmed"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let candidates = &bounce.rules[0]["declaration"]["decisions"][0]["candidates"];
    assert_eq!(
        engine.target_candidates("player-0", candidates),
        Some(vec![TargetRef::Permanent {
            instance_id: "charmed".to_string()
        }])
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn relative_history_clauses_read_this_turns_events_for_the_same_object() {
    let wrath = compiled(
        "Wrath Probe",
        "Sorcery",
        ("0", "0"),
        "Destroy all creatures that attacked this turn.",
    );
    let gain = compiled(
        "Count Probe",
        "Sorcery",
        ("0", "0"),
        "You gain 1 life for each creature that attacked this turn.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature("attacker", "player-0"),
        creature("idle", "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let identity = engine.object_identity_value("attacker");
    engine.record_event(
        "attackerDeclared",
        Some("player-0".to_string()),
        Some("attacker".to_string()),
        json!({ "objectIdentity": identity }),
    );
    let run = |engine: &mut GameEngine, card: &CardDefinition| {
        let rule = card.rules[0].clone();
        let spell = StackObject {
            id: "stack:probe".to_string(),
            controller: "player-0".to_string(),
            card: test_instance("probe", card.clone(), "player-0"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("spellAbility".to_string()),
            ability_rule: Some(rule.clone()),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        };
        engine
            .execute_effects(
                &rule["effects"],
                &spell,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut EmeritusDecisionProvider,
            )
            .expect("the spell resolves");
    };
    let life = engine.state.players[0].life;
    run(&mut engine, &gain);
    assert_eq!(
        engine.state.players[0].life,
        life + 1,
        "one creature attacked"
    );
    run(&mut engine, &wrath);
    assert!(engine.permanent_position("attacker").is_none());
    assert!(engine.permanent_position("idle").is_some());

    // `that was dealt damage this turn` in a target domain.
    let fatal = compiled(
        "Fatal Probe",
        "Instant",
        ("0", "0"),
        "Destroy target creature that was dealt damage this turn.",
    );
    engine.record_event(
        "damageDealt",
        None,
        None,
        json!({ "targetPermanentId": "idle", "amount": 2 }),
    );
    let candidates = &fatal.rules[0]["declaration"]["decisions"][0]["candidates"];
    assert_eq!(
        engine.target_candidates("player-1", candidates),
        Some(vec![TargetRef::Permanent {
            instance_id: "idle".to_string()
        }])
    );
}
