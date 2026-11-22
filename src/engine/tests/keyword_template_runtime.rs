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
