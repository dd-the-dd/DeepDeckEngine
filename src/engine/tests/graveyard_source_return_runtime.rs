//! `Return this card from your graveyard to the battlefield [tapped]`
//! (CR 110.2, 110.5) compiled from real Oracle text: an ability activated
//! from the graveyard moves its own card onto the battlefield.
use super::*;

fn compiled(name: &str, type_line: &str, oracle: &str) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace(' ', "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("1".to_string()),
        toughness: Some("1".to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{1}{B}".to_string()),
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
fn a_graveyard_activation_returns_its_own_card_to_the_battlefield_tapped() {
    let skeleton = compiled(
        "Reassembling Skeleton",
        "Creature \u{2014} Skeleton Warrior",
        "{1}{B}: Return this card from your graveyard to the battlefield tapped.",
    );
    let [rule] = skeleton.rules.as_slice() else {
        panic!("one rule: {:#?}", skeleton.rules);
    };
    assert_eq!(rule["activationZone"], "graveyard");
    assert_eq!(rule["effects"][0]["to"]["kind"], "battlefield");
    assert_eq!(rule["effects"][0]["to"]["tapped"], true);

    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        mana_source("swamp-a", "Basic Land - Swamp", "{B}", None),
        mana_source("swamp-b", "Basic Land - Swamp", "{B}", None),
    ];
    engine.state.players[0].graveyard =
        vec![test_instance("skeleton", skeleton.clone(), "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(activate(&mut engine, "skeleton"));
    assert!(engine.state.players[0].graveyard.is_empty());
    let returned = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.name == "Reassembling Skeleton")
        .expect("the skeleton is on the battlefield");
    assert!(returned.tapped, "it returns tapped");

    // Near miss: on the battlefield, the graveyard ability is not offered.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        mana_source("swamp-a", "Basic Land - Swamp", "{B}", None),
        mana_source("swamp-b", "Basic Land - Swamp", "{B}", None),
        test_instance("skeleton", skeleton, "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(!activate(&mut engine, "skeleton"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn an_untapped_graveyard_return_enters_untapped() {
    // Sibling without `tapped`, behind an exile-other-cards cost.
    let despoiler = compiled(
        "Despoiler of Souls",
        "Creature \u{2014} Horror",
        "{B}{B}, Exile two other creature cards from your graveyard: Return this card from your graveyard to the battlefield.",
    );
    assert_eq!(despoiler.rules[0]["effects"][0]["to"]["tapped"], false);
    let fodder = |id: &str| {
        let mut definition = test_definition(id, "Creature \u{2014} Test");
        definition.power = Some("1".to_string());
        definition.toughness = Some("1".to_string());
        test_instance(id, definition, "player-0")
    };
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        mana_source("swamp-a", "Basic Land - Swamp", "{B}", None),
        mana_source("swamp-b", "Basic Land - Swamp", "{B}", None),
    ];
    engine.state.players[0].graveyard = vec![
        test_instance("despoiler", despoiler, "player-0"),
        fodder("fodder-a"),
        fodder("fodder-b"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(activate(&mut engine, "despoiler"));
    let returned = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.definition.name == "Despoiler of Souls")
        .expect("the despoiler is on the battlefield");
    assert!(!returned.tapped);
    assert!(engine.state.players[0].graveyard.is_empty());
}
