use super::*;

fn hand_activation_rule() -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "activationZone": "hand",
        "costs": [{
            "kind": "discardCard",
            "card": { "kind": "self" },
        }],
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 1 },
        }],
    })
}

fn hand_source(instance_id: &str) -> CardInstance {
    let mut definition = test_definition("hand-activation-source", "Land");
    definition.rules = vec![hand_activation_rule()];
    test_instance(instance_id, definition, "player-0")
}

fn activation() -> CanonicalActivatedAbilityActivation {
    CanonicalActivatedAbilityActivation {
        controller_id: "player-0".to_string(),
        source_definition_id: "hand-activation-source".to_string(),
        ability_rule_index: Some(0),
        targets: BTreeMap::new(),
    }
}

#[test]
fn canonical_activated_ability_executes_a_legal_source_from_hand() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![hand_source("hand-source")];

    assert_eq!(
        engine.canonical_activated_ability_available(&activation()),
        Ok(true)
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .activate_canonical_ability(&activation(), &mut EmeritusDecisionProvider)
        .expect("the deterministic boundary reuses the legal hand activation");

    assert!(engine.state.players[0].hand.is_empty());
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    assert_eq!(
        engine.state.players[0].graveyard[0].definition.id,
        "hand-activation-source"
    );
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn canonical_activated_ability_rejects_a_source_in_the_wrong_zone() {
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard = vec![hand_source("graveyard-source")];

    assert_eq!(
        engine.canonical_activated_ability_available(&activation()),
        Ok(false)
    );
}

#[test]
fn canonical_activated_ability_rejects_an_ambiguous_definition_across_zones() {
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![hand_source("hand-source")];
    engine.state.players[0].graveyard = vec![hand_source("graveyard-source")];

    let error = engine
        .canonical_activated_ability_available(&activation())
        .expect_err("definition-based deterministic selection remains exact");

    assert!(error.message.contains("ambiguous across"));
}
