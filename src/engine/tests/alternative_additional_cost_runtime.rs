use super::*;

fn alternative_cost_spell_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Removal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(
            "As an additional cost to cast this spell, discard a card or pay 3 life.\nDestroy target creature or planeswalker."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    let mut definition = test_definition("test-alternative-removal", "Instant");
    definition.mana_cost = "{1}{B}".to_string();
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

fn engine_with_spell(life: i32, include_discard_card: bool) -> GameEngine {
    let definition = alternative_cost_spell_definition();
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].life = life;
    engine.state.players[0].mana_pool = floating_mana(&["C", "B"]);
    engine.state.players[0].hand = vec![test_instance(
        "test-alternative-removal",
        definition,
        "player-0",
    )];
    if include_discard_card {
        engine.state.players[0].hand.push(test_instance(
            "discard-payment",
            test_definition("discard-payment", "Sorcery"),
            "player-0",
        ));
    }
    engine.state.players[1].battlefield = vec![test_instance(
        "opposing-creature",
        test_definition("opposing-creature", "Creature - Test"),
        "player-1",
    )];
    engine
}

fn casting_actions(engine: &GameEngine) -> Vec<LegalAction> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("test-alternative-removal")
        })
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn casting_can_pay_exactly_the_discard_branch() {
    let mut engine = engine_with_spell(20, true);
    let action = casting_actions(&engine)
        .into_iter()
        .find(|action| {
            action.decisions["additionalCostMode"] == json!(["discard"])
                && action.targets.get("discardCost1")
                    == Some(&TargetRef::Card {
                        instance_id: "discard-payment".to_string(),
                    })
        })
        .expect("the discard branch should be payable");

    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("the discard branch should cast the spell");
    assert_eq!(engine.state.players[0].life, 20);
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "discard-payment")
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "cardDiscarded"
            && event.card_instance_id.as_deref() == Some("discard-payment")
            && event.detail["reason"] == "cost"
    }));

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the primitive destroy effect should resolve");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "opposing-creature")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn casting_can_pay_exactly_the_life_branch() {
    let mut engine = engine_with_spell(20, true);
    let action = casting_actions(&engine)
        .into_iter()
        .find(|action| {
            action.decisions["additionalCostMode"] == json!(["payLife"])
                && !action.targets.contains_key("discardCost1")
        })
        .expect("the life branch should be payable");

    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("the life branch should cast the spell");
    assert_eq!(engine.state.players[0].life, 17);
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "discard-payment")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn casting_is_unavailable_when_neither_branch_is_payable() {
    let engine = engine_with_spell(2, false);

    assert!(casting_actions(&engine).is_empty());
}
