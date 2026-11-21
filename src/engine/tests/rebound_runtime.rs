//! Real Oracle text -> keyword IR -> engine behavior for Rebound
//! (CR 702.88a): the shared delayed step trigger of the controller's upkeep.
use super::*;

const REMINDER: &str = "(If you cast this spell from your hand, exile it as it resolves. At the beginning of your next upkeep, you may cast this card from exile without paying its mana cost.)";

fn rebound_spell(instance_id: &str, body: &str) -> CardInstance {
    let oracle = format!("{body}\nRebound {REMINDER}");
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: instance_id.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(oracle.clone()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{oracle}");
    let mut definition = test_definition(instance_id, "Sorcery");
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect();
    assert!(
        definition
            .rules
            .iter()
            .any(|rule| rule["ability"]["kind"] == "rebound")
    );
    test_instance(instance_id, definition, "player-0")
}

fn cast_from_hand(engine: &mut GameEngine, instance_id: &str) {
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some(instance_id)
        })
        .expect("the spell can be cast from hand");
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("the spell is cast");
}

fn zone_has(cards: &[CardInstance], instance_id: &str) -> bool {
    cards.iter().any(|card| card.instance_id == instance_id)
}

#[test]
fn parsed_rebound_exiles_on_resolution_and_recasts_on_the_controllers_next_upkeep() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![rebound_spell("gift", "You gain 2 life.")];
    cast_from_hand(&mut engine, "gift");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the spell resolves");
    assert_eq!(engine.state.players[0].life, 22);
    assert!(zone_has(&engine.state.players[0].exile, "gift"));
    assert!(!zone_has(&engine.state.players[0].graveyard, "gift"));

    engine.state.active_player = 1;
    engine.enqueue_step_triggers("upkeep");
    assert!(
        engine.state.stack.is_empty(),
        "only the controller's upkeep"
    );
    engine.state.active_player = 0;
    engine.enqueue_step_triggers("upkeep");
    assert_eq!(engine.state.stack.len(), 1, "the rebound trigger");
    engine
        .resolve_top_stack(&mut CastSpellDecisionProvider)
        .expect("the rebound trigger resolves and its controller casts the card");
    if !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the rebound copy-free cast resolves");
    }
    assert_eq!(engine.state.players[0].life, 24, "cast again for free");
    assert!(
        zone_has(&engine.state.players[0].graveyard, "gift"),
        "a spell cast from exile does not rebound again"
    );
}

/// CR 608.2b: a spell whose only target became illegal doesn't resolve, so
/// its rebound replacement never applies.
#[test]
fn parsed_rebound_spell_that_does_not_resolve_goes_to_the_graveyard() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![rebound_spell(
        "bounce",
        "Return target creature to its owner's hand.",
    )];
    engine.state.players[1].battlefield = vec![test_instance(
        "victim",
        test_definition("victim", "Creature — Bear"),
        "player-1",
    )];
    cast_from_hand(&mut engine, "bounce");
    engine
        .move_permanent_to_graveyard("victim", "destroyed")
        .expect("the target leaves");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the spell is removed without resolving");
    assert!(zone_has(&engine.state.players[0].graveyard, "bounce"));
    assert!(
        !engine
            .state
            .rule_modifiers
            .iter()
            .any(|modifier| { value_kind(modifier) == Some("delayedStepTrigger") })
    );
}
