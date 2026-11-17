use super::*;

fn graveyard_rule(costs: Value, effects: Value) -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "activationZone": "graveyard",
        "costs": costs,
        "effects": effects,
    })
}

fn engine_with_swamps(source: CardInstance, in_graveyard: bool) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield = vec![
        mana_source("swamp-a", "Basic Land - Swamp", "{B}", None),
        mana_source("swamp-b", "Basic Land - Swamp", "{B}", None),
    ];
    if in_graveyard {
        engine.state.players[0].graveyard.push(source);
    } else {
        engine.state.players[0].battlefield.push(source);
    }
    engine
}

fn activate(engine: &mut GameEngine, source_id: &str) {
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some(source_id)
        })
        .expect("the graveyard activation is offered");
    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(7))
        .expect("activation pays its costs");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the ability resolves");
}

fn source_with(rule: Value) -> CardInstance {
    let mut definition = test_definition("gy-source", "Creature - Construct");
    definition.rules = vec![rule];
    test_instance("gy-source", definition, "player-0")
}

fn offered(engine: &GameEngine) -> bool {
    engine.legal_priority_actions(0).iter().any(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id.as_deref() == Some("gy-source")
    })
}

#[test]
fn exile_self_cost_is_paid_from_the_graveyard_before_the_effect_resolves() {
    let rule = graveyard_rule(
        json!([
            { "kind": "payMana", "manaCost": "{1}{B}" },
            { "kind": "exileSource", "object": { "kind": "self" }, "zone": "graveyard" },
        ]),
        json!([{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 3 },
        }]),
    );
    assert!(activated_rule_supported(&rule));
    let mut engine = engine_with_swamps(source_with(rule.clone()), true);
    let life = engine.state.players[0].life;
    activate(&mut engine, "gy-source");
    assert!(engine.state.players[0].graveyard.is_empty());
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "gy-source")
    );
    assert_eq!(engine.state.players[0].life, life + 3);

    // The activation zone is exclusive: the same rule is not offered from the
    // battlefield, where there is no graveyard card to exile.
    let engine = engine_with_swamps(source_with(rule), false);
    assert!(!offered(&engine));
}

#[test]
fn source_return_from_the_graveyard_moves_the_card_to_its_owners_hand() {
    let rule = graveyard_rule(
        json!([{ "kind": "payMana", "manaCost": "{1}{B}" }]),
        json!([{
            "kind": "moveCards",
            "cards": { "kind": "self" },
            "from": { "kind": "graveyard", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            "to": { "kind": "hand", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
        }]),
    );
    assert!(activated_rule_supported(&rule));
    let mut engine = engine_with_swamps(source_with(rule.clone()), true);
    activate(&mut engine, "gy-source");
    assert!(engine.state.players[0].graveyard.is_empty());
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "gy-source")
    );

    let engine = engine_with_swamps(source_with(rule), false);
    assert!(!offered(&engine));
}
