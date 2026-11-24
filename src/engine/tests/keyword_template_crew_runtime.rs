//! Crew (CR 702.122a) compiled from its rules definition: a total-power tap
//! cost (`src/engine/creature_tap_cost.rs`) then the ordinary become-types
//! effect, through the playable-card adapter that flattens the expansion.
use super::*;

fn compiled(
    name: &str,
    type_line: &str,
    power: &str,
    toughness: &str,
    oracle: &str,
) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace(' ', "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some(power.to_string()),
        toughness: Some(toughness.to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{5}".to_string()),
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

fn creature(id: &str, power: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Test");
    definition.power = Some(power.to_string());
    definition.toughness = Some("2".to_string());
    test_instance(id, definition, "player-0")
}

fn is_tapped(engine: &GameEngine, id: &str) -> bool {
    let (player, index) = engine.permanent_position(id).expect("on the battlefield");
    engine.state.players[player].battlefield[index].tapped
}

fn is_creature(engine: &GameEngine, id: &str) -> bool {
    let (player, index) = engine.permanent_position(id).expect("on the battlefield");
    engine.permanent_is_creature(&engine.state.players[player].battlefield[index])
}

fn crew_actions(engine: &GameEngine, vehicle: &str) -> Vec<LegalAction> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some(vehicle)
        })
        .collect()
}

fn crew_choice(action: &LegalAction) -> Vec<String> {
    action.decisions["activationCostTapPower0"]
        .as_array()
        .expect("the declared crew")
        .iter()
        .map(|id| id.as_str().expect("an instance id").to_string())
        .collect()
}

const DREADNOUGHT: &str = "Crew 6 (Tap any number of creatures you control with total power 6 or more: This Vehicle becomes an artifact creature until end of turn.)";

fn dreadnought_engine(crew: &[(&str, &str)]) -> GameEngine {
    let dreadnought = compiled(
        "Consulate Dreadnought",
        "Artifact \u{2014} Vehicle",
        "7",
        "11",
        DREADNOUGHT,
    );
    let [rule] = dreadnought.rules.as_slice() else {
        panic!("one rule: {:#?}", dreadnought.rules);
    };
    assert_eq!(value_kind(rule), Some("activatedAbility"));
    assert_eq!(rule["keywordProvenance"]["keyword"], "crew");
    assert_eq!(rule["costs"][0]["kind"], "tapPermanentsWithTotalPower");
    assert_eq!(rule["costs"][0]["excludeSource"], true);
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield =
        vec![test_instance("dreadnought", dreadnought, "player-0")];
    engine.state.players[0]
        .battlefield
        .extend(crew.iter().map(|(id, power)| creature(id, power)));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn crew_taps_creatures_with_enough_total_power_and_animates_the_vehicle() {
    // Total power 4 cannot crew 6: the ability is not offered.
    let engine = dreadnought_engine(&[("bear-a", "2"), ("bear-b", "2")]);
    assert!(crew_actions(&engine, "dreadnought").is_empty());

    let mut engine = dreadnought_engine(&[("bear-a", "2"), ("bear-b", "2"), ("ogre", "3")]);
    let actions = crew_actions(&engine, "dreadnought");
    assert!(!actions.is_empty());
    // Every offered crew reaches total power 6, the source is never in it.
    for action in &actions {
        let crew = crew_choice(action);
        assert!(!crew.iter().any(|id| id == "dreadnought"));
        let total: i32 = crew
            .iter()
            .map(|id| match id.as_str() {
                "ogre" => 3,
                _ => 2,
            })
            .sum();
        assert!(total >= 6, "{crew:?}");
    }
    assert!(!is_creature(&engine, "dreadnought"));
    let action = actions[0].clone();
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(7))
        .expect("the crew pays its cost");
    for id in crew_choice(&action) {
        assert!(is_tapped(&engine, &id), "{id} crewed");
    }
    // A summoning-sick creature still crews: the cost is not {T}.
    assert!(!is_tapped(&engine, "dreadnought"));
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the crew ability resolves");
    assert!(is_creature(&engine, "dreadnought"));
    let (player, index) = engine.permanent_position("dreadnought").expect("present");
    assert_eq!(
        engine.current_power(&engine.state.players[player].battlefield[index]),
        7
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn crew_rejects_a_declared_set_below_the_total_power() {
    let mut engine = dreadnought_engine(&[("bear-a", "2"), ("bear-b", "2"), ("ogre", "3")]);
    let mut action = crew_actions(&engine, "dreadnought")
        .into_iter()
        .next()
        .expect("a crew action");
    // Two 2-power creatures total 4 < 6 (CR 118.3: no partial payment).
    action.decisions.insert(
        "activationCostTapPower0".to_string(),
        json!(["bear-a", "bear-b"]),
    );
    assert!(
        engine
            .apply_priority_action(&action, &mut RandomAi::seeded(7))
            .is_err()
    );
    assert!(!is_tapped(&engine, "bear-a"));
    assert!(!is_tapped(&engine, "bear-b"));
    assert!(engine.state.stack.is_empty());
    // The same creature twice does not double its power.
    action.decisions.insert(
        "activationCostTapPower0".to_string(),
        json!(["ogre", "ogre", "bear-a"]),
    );
    assert!(
        engine
            .apply_priority_action(&action, &mut RandomAi::seeded(7))
            .is_err()
    );
    // A tapped creature cannot crew.
    let (player, index) = engine.permanent_position("ogre").expect("present");
    engine.state.players[player].battlefield[index].tapped = true;
    assert!(crew_actions(&engine, "dreadnought").is_empty());
}
