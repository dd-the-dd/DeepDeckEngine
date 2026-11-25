//! Real Oracle text -> `aftermath` keyword IR -> engine behavior (CR
//! 702.127a): the half with aftermath is cast only from its owner's
//! graveyard and is exiled as it leaves the stack; the other half is cast
//! only from the hand (absent another route).
use super::*;

const AFTERMATH_REMINDER: &str = "(Cast this spell only from your graveyard. Then exile it.)";

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

fn split_face(id: &str, name: &str, mana_cost: &str, oracle: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "typeLine": "Sorcery",
        "manaCost": mana_cost,
        "oracleText": oracle,
        "power": null,
        "toughness": null,
        "rules": parsed_rules(name, "Sorcery", mana_cost, oracle),
    })
}

/// Spring // Mind, each half compiled from its real Oracle text. (Dusk //
/// Dawn's return of creature cards by power is not executable: a card-zone
/// query reads no power, see `plural_effect_subject_runtime`.)
fn spring_mind() -> CardInstance {
    let mut definition = test_definition("spring-mind", "Sorcery // Sorcery");
    definition.name = "Spring // Mind".to_string();
    definition.mana_cost = "{2}{G} // {4}{U}{U}".to_string();
    definition.rules = vec![json!({
        "kind": "rulesMarker",
        "text": "Split card faces.",
        "splitFaces": [
            split_face(
                "spring",
                "Spring",
                "{2}{G}",
                "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.",
            ),
            split_face(
                "mind",
                "Mind",
                "{4}{U}{U}",
                &format!("Aftermath {AFTERMATH_REMINDER}\nDraw two cards."),
            ),
        ],
    })];
    test_instance("spring-mind", definition, "player-0")
}

/// Enough mana for either half.
fn mana(engine: &mut GameEngine) {
    engine.state.players[0].mana_pool = ["G", "U", "U", "C", "C", "C", "C"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect();
}

fn half_casts(engine: &GameEngine) -> Vec<u64> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("spring-mind")
        })
        .filter_map(|action| {
            action
                .decisions
                .get("splitFaceIndex")
                .and_then(Value::as_u64)
        })
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_aftermath_half_is_cast_from_the_graveyard_and_then_exiled() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    mana(&mut engine);
    engine.state.players[0].graveyard = vec![spring_mind()];
    engine.state.players[0].library = (0..3)
        .map(|index| {
            let id = format!("library-{index}");
            test_instance(&id, test_definition(&id, "Instant"), "player-0")
        })
        .collect();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let hand_before = engine.state.players[0].hand.len();

    let halves = half_casts(&engine);
    assert!(halves.contains(&1), "Mind is cast from the graveyard");
    assert!(
        !halves.contains(&0),
        "Spring has no route from the graveyard: {halves:?}"
    );
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("spring-mind")
                && action.decisions.get("splitFaceIndex") == Some(&json!(1))
        })
        .expect("the aftermath half can be cast");
    assert_eq!(
        cast.decisions.get("exileOnLeaveStack"),
        Some(&Value::Bool(true))
    );
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("cast Mind");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("resolve Mind");
    assert_eq!(
        engine.state.players[0].hand.len(),
        hand_before + 2,
        "Mind drew two cards"
    );
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "spring-mind"),
        "exiled as it left the stack"
    );
    assert!(
        !engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "spring-mind")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn only_the_first_half_is_cast_from_the_hand() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    mana(&mut engine);
    engine.state.players[0].hand = vec![spring_mind()];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    let halves = half_casts(&engine);
    assert!(halves.contains(&0), "Spring is cast from the hand");
    assert!(
        !halves.contains(&1),
        "Mind can't be cast from any zone other than a graveyard"
    );
}
