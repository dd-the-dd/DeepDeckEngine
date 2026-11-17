use super::*;

fn spell_cast_event(scope: &str) -> Value {
    match scope {
        "opponent" => json!({
            "kind": "spellCast",
            "opponentOfSourceController": true,
            "where": Value::Null,
        }),
        "any" => json!({
            "kind": "spellCast",
            "anyPlayer": true,
            "where": Value::Null,
        }),
        "controller" => json!({
            "kind": "spellCast",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "where": Value::Null,
        }),
        unsupported => panic!("unsupported test scope {unsupported}"),
    }
}

fn stack_depth_after_cast(scope: &str, caster: usize) -> usize {
    let mut observer_definition = test_definition("spell-cast-observer", "Enchantment");
    observer_definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": spell_cast_event(scope),
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": { "kind": "integer", "value": 1 },
        }],
    })];

    let mut spell_definition = test_definition("scope-test-spell", "Sorcery");
    spell_definition.mana_cost = "{0}".to_string();
    spell_definition.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": { "kind": "integer", "value": 1 },
        }],
    })];

    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = caster;
    engine.state.priority_player = Some(caster);
    engine.state.players[0].battlefield = vec![test_instance(
        "spell-cast-observer-instance",
        observer_definition,
        "player-0",
    )];
    engine.state.players[caster].hand = vec![test_instance(
        "scope-test-spell-instance",
        spell_definition,
        &format!("player-{caster}"),
    )];

    let action = engine
        .legal_priority_actions(caster)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("scope-test-spell-instance")
        })
        .expect("the zero-cost test sorcery is castable by the active player");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(96))
        .expect("the test spell is cast through the normal priority path");
    engine.state.stack.len()
}

#[test]
fn spell_cast_player_scopes_distinguish_opponent_controller_and_any_player() {
    assert_eq!(stack_depth_after_cast("opponent", 1), 2);
    assert_eq!(stack_depth_after_cast("opponent", 0), 1);

    assert_eq!(stack_depth_after_cast("controller", 0), 2);
    assert_eq!(stack_depth_after_cast("controller", 1), 1);

    assert_eq!(stack_depth_after_cast("any", 0), 2);
    assert_eq!(stack_depth_after_cast("any", 1), 2);
}
