use super::*;

const DAY_OF_BLACK_SUN_ORACLE: &str = "Each creature with mana value X or less loses all abilities until end of turn. Destroy those creatures.";

fn ability_loss_then_destroy_rule() -> Value {
    let request = OracleCardParseRequest {
        card_name: "Day of Black Sun".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{X}{B}{B}".to_string()),
        oracle_text: Some(DAY_OF_BLACK_SUN_ORACLE.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("ability-loss-destroy-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("the spell should compile to primitive IR");
    assert!(rule_is_executable(&rule));
    rule
}

fn permanent_with_mana_value(
    id: &str,
    type_line: &str,
    mana_cost: &str,
    owner: &str,
) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    test_instance(id, definition, owner)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn all_qualifying_creatures_lose_indestructible_before_the_bound_set_is_destroyed() {
    let rule = ability_loss_then_destroy_rule();
    let mut indestructible =
        permanent_with_mana_value("small-indestructible", "Creature", "{2}", "player-1");
    indestructible.definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "indestructible" },
    }));
    let mut death_watcher =
        permanent_with_mana_value("small-death-watcher", "Creature", "{1}", "player-1");
    death_watcher.definition.rules.push(json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "permanentDied",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "where": { "kind": "cardTypeContains", "value": "Creature" },
            "excludeSource": true,
        },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 1 },
        }],
    }));
    assert!(
        death_watcher
            .definition
            .rules
            .iter()
            .all(rule_is_executable)
    );
    let large_creature = permanent_with_mana_value("large-creature", "Creature", "{3}", "player-1");
    let small_artifact = permanent_with_mana_value("small-artifact", "Artifact", "{1}", "player-1");

    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        indestructible,
        death_watcher,
        large_creature,
        small_artifact,
    ];
    let stack_object = StackObject {
        id: "stack:ability-loss-destroy".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "ability-loss-destroy-spell",
            test_definition("ability-loss-destroy-spell", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::from([("xValue".to_string(), Value::from(2))]),
        targets: BTreeMap::new(),
    };

    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut stack_object.decisions.clone(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the primitive sequence resolves");

    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "small-indestructible"),
        "ability loss must happen before destruction so indestructible cannot save the creature"
    );
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "small-death-watcher")
    );
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "large-creature"),
        "the bound set must exclude creatures above X"
    );
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "small-artifact"),
        "the bound set must retain the creature criterion"
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "permanentDestroyed"
            && event.card_instance_id.as_deref() == Some("small-indestructible")
    }));
    assert!(
        engine.state.stack.is_empty(),
        "the second qualifying creature must already have lost its death-watching ability when the first creature dies"
    );
}
