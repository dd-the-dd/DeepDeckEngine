use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}
fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1893,
    )
    .unwrap()
}
fn add(
    engine: &mut GameEngine,
    owner: usize,
    controller: usize,
    id: &str,
    type_line: &str,
) -> CardInstance {
    let mut card = card_instance(
        &format!("player-{owner}"),
        &definition(id, type_line),
        engine.state.players[controller].battlefield.len() + 100,
    );
    card.controller = format!("player-{controller}");
    engine.state.players[controller]
        .battlefield
        .push(card.clone());
    card
}
fn counter(engine: &GameEngine, event: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|e| e.kind == event)
        .count()
}

#[test]
fn installation_separates_all_physical_moves_from_callbacks_and_keeps_real_owners() {
    let mut e = engine();
    let a = add(&mut e, 0, 1, "foreign-a", "Creature");
    let b = add(&mut e, 1, 0, "foreign-b", "Creature");
    e.admit_explicit_test_fixture_objects().unwrap();
    let first = e
        .install_permanent_graveyard_move(&a.instance_id, FrozenGraveyardDestination::Graveyard)
        .unwrap()
        .unwrap();
    let second = e
        .install_permanent_graveyard_move(&b.instance_id, FrozenGraveyardDestination::Graveyard)
        .unwrap()
        .unwrap();
    assert!(e.state.players.iter().all(|p| p.battlefield.is_empty()));
    assert_eq!(e.state.players[0].graveyard[0].instance_id, a.instance_id);
    assert_eq!(e.state.players[1].graveyard[0].instance_id, b.instance_id);
    assert_eq!(counter(&e, "permanentLeftBattlefield"), 0);
    assert_eq!(counter(&e, "cardsEnteredGraveyard"), 0);
    assert!(e.state.stack.is_empty());
    assert_eq!(first.permanent.controller, "player-1");
    assert_eq!(second.permanent.controller, "player-0");
    e.publish_installed_battlefield_departure(&first, "destroyed")
        .unwrap();
    e.publish_installed_battlefield_departure(&second, "destroyed")
        .unwrap();
    assert_eq!(counter(&e, "permanentLeftBattlefield"), 2);
    assert_eq!(counter(&e, "permanentDied"), 2);
}

#[test]
fn destination_plan_is_frozen_before_its_replacement_source_leaves() {
    let mut e = engine();
    let mut source = add(&mut e, 0, 0, "destination-source", "Enchantment");
    let replacement = json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"replaceGraveyardDestination","graveyardOwners":{"kind":"eachPlayer"},
            "where":{"kind":"cardTypeContains","value":"Creature"},"from":{"kind":"anyZone"},"to":{"kind":"exile"}}]});
    assert!(rule_is_executable(&replacement));
    source.definition.rules.push(replacement);
    e.state.players[0].battlefield[0] = source.clone();
    let target = add(&mut e, 1, 1, "recipient", "Creature");
    e.admit_explicit_test_fixture_objects().unwrap();
    let modifiers = e.battlefield_static_modifiers();
    let source_plan = e
        .plan_graveyard_departure(&source, &modifiers, GraveyardMoveCause::Destruction)
        .unwrap();
    let target_plan = e
        .plan_graveyard_departure(&target, &modifiers, GraveyardMoveCause::Destruction)
        .unwrap();
    assert!(matches!(
        source_plan.outcome,
        PlannedGraveyardOutcome::MoveTo(FrozenGraveyardDestination::Graveyard)
    ));
    assert!(matches!(
        target_plan.outcome,
        PlannedGraveyardOutcome::MoveTo(FrozenGraveyardDestination::Exile { .. })
    ));
    let first = e
        .install_planned_graveyard_departure(&source, &source_plan, "destroyed")
        .unwrap();
    let second = e
        .install_planned_graveyard_departure(&target, &target_plan, "destroyed")
        .unwrap();
    assert_eq!(e.state.players[1].exile[0].instance_id, target.instance_id);
    assert!(second.died().is_none());
    assert_eq!(counter(&e, "permanentLeftBattlefield"), 0);
    e.publish_installed_graveyard_departure(&first, "destroyed")
        .unwrap();
    e.publish_installed_graveyard_departure(&second, "destroyed")
        .unwrap();
    assert_eq!(counter(&e, "permanentDied"), 1);
}

#[test]
fn earthbend_return_is_a_publication_continuation_not_an_installation_callback() {
    let mut e = engine();
    let mut card = add(&mut e, 0, 1, "earthbent", "Land Creature");
    card.flags.insert("earthbent".to_string(), true);
    e.state.players[1].battlefield[0] = card.clone();
    e.admit_explicit_test_fixture_objects().unwrap();
    let movement = e
        .install_permanent_graveyard_move(&card.instance_id, FrozenGraveyardDestination::Graveyard)
        .unwrap()
        .unwrap();
    assert!(e.permanent_position(&card.instance_id).is_none());
    assert_eq!(
        e.state.players[0].graveyard[0].instance_id,
        card.instance_id
    );
    assert_eq!(counter(&e, "earthbentLandReturned"), 0);
    e.publish_installed_battlefield_departure(&movement, "destroyed")
        .unwrap();
    let (owner, index) = e.permanent_position(&card.instance_id).unwrap();
    assert_eq!(owner, 0);
    assert!(e.state.players[owner].battlefield[index].tapped);
    assert_eq!(
        e.state.players[owner].battlefield[index].controller,
        "player-0"
    );
    assert_eq!(counter(&e, "earthbentLandReturned"), 1);
}

#[test]
fn sacrifice_destination_planning_has_no_destruction_prevention() {
    let mut e = engine();
    let mut card = add(&mut e, 0, 0, "protected", "Creature");
    card.definition
        .rules
        .push(json!({"kind":"keywordAbility","ability":{"kind":"indestructible"}}));
    card.counters.insert("shield".to_string(), 1);
    e.state.players[0].battlefield[0] = card.clone();
    e.state
        .rule_modifiers
        .push(json!({"kind":"regenerationShield","cardInstanceId":card.instance_id}));
    let mut aura = add(&mut e, 0, 0, "umbra", "Enchantment - Aura");
    aura.attached_to = Some(card.instance_id.clone());
    aura.definition
        .rules
        .push(json!({"kind":"keywordAbility","ability":{"kind":"umbraArmor"}}));
    e.state.players[0].battlefield[1] = aura;
    assert!(e.permanent_has_keyword(&card, "indestructible"));
    let before = serde_json::to_value(&e.state).unwrap();
    let plan = e
        .plan_graveyard_departure(
            &card,
            &e.battlefield_static_modifiers(),
            GraveyardMoveCause::Sacrifice,
        )
        .unwrap();
    assert!(matches!(
        plan.outcome,
        PlannedGraveyardOutcome::MoveTo(FrozenGraveyardDestination::Graveyard)
    ));
    assert_eq!(
        serde_json::to_value(&e.state).unwrap(),
        before,
        "planning does not consume shields or auras"
    );
}

#[test]
fn ambiguous_destination_is_rejected_before_any_mutation() {
    let mut e = engine();
    let card = add(&mut e, 0, 0, "recipient", "Creature");
    for source in ["first", "second"] {
        e.state
            .rule_modifiers
            .push(json!({"kind":"exileSpecificCardInsteadOfGraveyard",
            "cardInstanceId":card.instance_id,"sourceCardInstanceId":source}));
    }
    let before = serde_json::to_value(&e.state).unwrap();
    let result = e.plan_graveyard_departure(
        &card,
        &e.battlefield_static_modifiers(),
        GraveyardMoveCause::Sacrifice,
    );
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("sacrifice batch has multiple graveyard destination replacements")
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
