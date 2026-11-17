//! CR 702.5a / 303.4a / 704.5m: the `enchant` keyword restricts what an Aura
//! spell may target on resolution and what the Aura may stay attached to.
use super::*;

/// Fixture objects are admitted with an incarnation, as a zone writer would.
fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn enchant_rule(restriction: Value) -> Value {
    let mut ability = json!({ "kind": "enchant" });
    for (key, value) in restriction.as_object().expect("restriction object") {
        ability[key] = value.clone();
    }
    json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": ability })
}

fn creature_restriction() -> Value {
    json!({ "where": { "kind": "cardTypeContains", "value": "Creature" } })
}

fn aura_card(restriction: Value, attached_to: Option<&str>) -> CardInstance {
    let mut definition = test_definition("aura", "Enchantment - Aura");
    definition.rules.push(enchant_rule(restriction));
    let mut aura = test_instance("aura", definition, "player-0");
    aura.attached_to = attached_to.map(ToOwned::to_owned);
    aura
}

fn on_battlefield_after_sba(aura: CardInstance, host: Option<(&str, &str, &str)>) -> bool {
    let mut engine = test_engine(2);
    let aura = admitted(&mut engine, aura);
    engine.state.players[0].battlefield.push(aura);
    if let Some((id, type_line, owner)) = host {
        let index = if owner == "player-0" { 0 } else { 1 };
        let host = test_instance(id, test_definition(id, type_line), owner);
        let host = admitted(&mut engine, host);
        engine.state.players[index].battlefield.push(host);
    }
    engine
        .check_state_based_actions_with_provider(None)
        .expect("state-based actions run");
    engine.state.players[0]
        .battlefield
        .iter()
        .any(|permanent| permanent.instance_id == "aura")
}

#[test]
fn aura_stays_on_a_permanent_matching_its_restriction() {
    assert!(on_battlefield_after_sba(
        aura_card(creature_restriction(), Some("host")),
        Some(("host", "Creature - Bear", "player-1")),
    ));
}

#[test]
fn aura_attached_to_an_object_failing_its_restriction_is_put_into_the_graveyard() {
    // The enchanted permanent stopped being a creature (CR 704.5m).
    assert!(!on_battlefield_after_sba(
        aura_card(creature_restriction(), Some("host")),
        Some(("host", "Land", "player-1")),
    ));
}

#[test]
fn aura_without_an_attachment_or_with_a_missing_host_is_put_into_the_graveyard() {
    assert!(!on_battlefield_after_sba(
        aura_card(creature_restriction(), None),
        None
    ));
    assert!(!on_battlefield_after_sba(
        aura_card(creature_restriction(), Some("gone")),
        None
    ));
}

#[test]
fn controller_relation_is_evaluated_for_the_aura_controller() {
    // Sibling combination: `Enchant creature you control` on an opponent's creature.
    let mut engine = test_engine(2);
    let mut definition = test_definition("aura", "Enchantment - Aura");
    let controller = json!({ "kind": "controllerOf", "object": { "kind": "self" } });
    definition.rules.push(enchant_rule(json!({
        "where": { "kind": "cardTypeContains", "value": "Creature" },
        "controller": controller,
    })));
    let mut aura = test_instance("aura", definition, "player-0");
    aura.attached_to = Some("theirs".to_string());
    let aura = admitted(&mut engine, aura);
    engine.state.players[0].battlefield.push(aura);
    let theirs = test_instance(
        "theirs",
        test_definition("theirs", "Creature - Bear"),
        "player-1",
    );
    let theirs = admitted(&mut engine, theirs);
    engine.state.players[1].battlefield.push(theirs);
    engine
        .check_state_based_actions_with_provider(None)
        .expect("state-based actions run");
    assert!(engine.state.players[0].battlefield.is_empty());
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
}

#[test]
fn aura_spell_whose_target_no_longer_matches_does_not_resolve() {
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield.push(test_instance(
        "host",
        test_definition("host", "Creature - Bear"),
        "player-1",
    ));
    let mut definition = test_definition("aura", "Enchantment - Aura");
    definition.rules.push(enchant_rule(creature_restriction()));
    let card = test_instance("aura", definition, "player-0");
    let target = TargetRef::Permanent {
        instance_id: "host".to_string(),
    };
    let stack_object = StackObject {
        id: "stack:aura".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([("enchantTarget".to_string(), target.clone())]),
    };
    assert!(engine.target_is_legal_for_stack_object("enchantTarget", &target, &stack_object));
    // The host stops being a creature in response (CR 608.2b).
    engine.state.players[1].battlefield[0].definition.type_line = "Land".to_string();
    assert!(!engine.target_is_legal_for_stack_object("enchantTarget", &target, &stack_object));
}

#[test]
fn enchant_permanent_has_a_null_restriction_and_accepts_any_permanent() {
    assert!(on_battlefield_after_sba(
        aura_card(json!({ "where": null }), Some("host")),
        Some(("host", "Land", "player-1")),
    ));
}
