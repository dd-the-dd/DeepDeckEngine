//! CR 119.4: a player can pay life only if their life total is greater than
//! or equal to the payment, so paying exactly one's whole life total is legal.

use super::*;

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_alternative_life_cost_equal_to_the_life_total_is_payable() {
    // The primitive alternative cost `pay 4 life` (CR 118.9); the payment
    // legality under test is the runtime's, independent of any card grammar.
    let mut definition = test_definition("life-cost-probe", "Sorcery");
    definition.mana_cost = "{3}{B}".to_string();
    definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "alternativeCost",
            "costs": [{
                "kind": "payLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 4 },
            }],
        },
    })];
    assert!(definition.rules.iter().any(|rule| {
        value_kind(rule) == Some("keywordAbility") && alternative_cost_supported(&rule["ability"])
    }));
    definition.rules.push(json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 1 },
        }],
    }));

    for (life, payable) in [(4, true), (3, false)] {
        let mut engine = test_engine(2);
        engine.state.step = GameStep::PrecombatMain;
        engine.state.active_player = 0;
        engine.state.priority_player = Some(0);
        engine.state.players[0].life = life;
        engine.state.players[0].hand = vec![test_instance(
            "life-cost-probe",
            definition.clone(),
            "player-0",
        )];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let casts = engine
            .legal_priority_actions(0)
            .into_iter()
            .filter(|action| {
                action.kind == ActionKind::CastSpell
                    && action.decisions.get("useAlternativeCost") == Some(&Value::Bool(true))
            })
            .collect::<Vec<_>>();
        assert_eq!(casts.len(), usize::from(payable), "life {life}: {casts:#?}");
        if payable {
            engine
                .apply_priority_action(&casts[0], &mut RandomAi::seeded(7))
                .expect("paying the whole life total is legal");
            assert_eq!(engine.state.players[0].life, 0);
        }
    }
}
