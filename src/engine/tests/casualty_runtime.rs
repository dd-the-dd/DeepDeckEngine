//! Real Oracle text -> `casualty` keyword IR -> engine behavior (CR
//! 702.153a-b): an optional sacrifice of a creature with enough power as the
//! spell is cast, and one copy of the spell when that cost was paid.
use super::*;

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
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
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn join_the_maestros() -> CardInstance {
    let mut definition = test_definition("maestros", "Sorcery");
    definition.name = "Join the Maestros".to_string();
    definition.mana_cost = "{4}{B}".to_string();
    definition.rules = parsed_rules(
        "Join the Maestros",
        "Sorcery",
        "{4}{B}",
        "Casualty 2 (As you cast this spell, you may sacrifice a creature with power 2 or greater. When you do, copy this spell.)\nCreate a 4/3 black Ogre Warrior creature token.",
    );
    test_instance("maestros", definition, "player-0")
}

fn creature(instance_id: &str, power: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, "Creature — Citizen");
    definition.power = Some(power.to_string());
    test_instance(instance_id, definition, "player-0")
}

fn engine_with_maestros() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = ["B", "C", "C", "C", "C"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect();
    engine.state.players[0].hand = vec![join_the_maestros()];
    engine.state.players[0].battlefield = vec![creature("citizen", "2"), creature("small", "1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn cast_with(engine: &GameEngine, sacrifice: Value) -> Option<LegalAction> {
    engine.legal_priority_actions(0).into_iter().find(|action| {
        action.kind == ActionKind::CastSpell
            && action.card_instance_id.as_deref() == Some("maestros")
            && action.decisions.get("casualtySacrifice:0") == Some(&sacrifice)
    })
}

fn ogres(engine: &GameEngine) -> usize {
    engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.type_line.contains("Ogre"))
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_paid_casualty_sacrifices_the_creature_and_copies_the_spell() {
    let mut engine = engine_with_maestros();
    assert!(
        cast_with(&engine, json!(["small"])).is_none(),
        "a creature with power 1 can't pay casualty 2"
    );
    let cast = cast_with(&engine, json!(["citizen"])).expect("casualty can be paid");
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("cast with casualty");
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "citizen"),
        "the creature was sacrificed as a cost"
    );
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "casualtyTriggered")
    );
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the copy trigger, the copy and the spell resolve");
    }
    assert_eq!(ogres(&engine), 2, "the spell and its copy");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_unpaid_casualty_casts_the_spell_alone() {
    let mut engine = engine_with_maestros();
    let cast = cast_with(&engine, json!([])).expect("casualty may be declined");
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("cast without casualty");
    assert_eq!(engine.state.stack.len(), 1, "no copy trigger");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the spell resolves");
    assert_eq!(ogres(&engine), 1);
    assert_eq!(engine.state.players[0].battlefield.len(), 3);
}
