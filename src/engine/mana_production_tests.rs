use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line, "manaCost": "{0}",
        "isCommander": false, "isGamePiece": false, "isSideboard": false,
        "isToken": false, "rules": rules,
    }))
    .expect("bounded synthetic source definition")
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Land", vec![]);
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        601,
    )
    .expect("bounded synthetic game")
}

fn mana_rule(costs: Vec<Value>) -> Value {
    json!({ "kind": "manaAbility", "source": { "kind": "self" }, "costs": costs,
        "effects": [{ "kind": "addMana", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "mana": "{G}" }] })
}

fn source(rule: Value) -> CardInstance {
    let mut source = card_instance("player-0", &definition("producer", "Land", vec![rule]), 0);
    source.instance_id = "producer-instance".into();
    source
}

#[test]
fn shared_mana_planner_preserves_typed_resolver_failure() {
    let engine = engine();
    let source = source(mana_rule(vec![
        json!({ "kind": "tap", "object": { "kind": "self" } }),
    ]));
    let error = engine
        .try_plan_mana_production(
            &source,
            None,
            true,
            true,
            &BTreeSet::from([source.instance_id.clone()]),
            ManaProductionMode::Counterfactual,
            &mut |_, _, _, _| Err::<Vec<Vec<String>>, _>("projection-state-unsupported"),
        )
        .expect_err("resolver failure must not become empty choices");
    assert_eq!(error, "projection-state-unsupported");
}

#[test]
fn shared_mana_planner_ignores_unpayable_costs_only_in_counterfactual_mode() {
    let engine = engine();
    let source = source(mana_rule(vec![
        json!({ "kind": "tap", "object": { "kind": "self" } }),
        json!({ "kind": "payLife", "amount": { "kind": "integer", "value": 30 } }),
        json!({ "kind": "payMana", "manaCost": "{9}" }),
    ]));
    let mut resolve = |_: &Value, _: &str, _: &BTreeSet<String>, _: Option<&CardInstance>| {
        Ok::<_, &'static str>(vec![vec!["G".into()]])
    };
    let visited = BTreeSet::from([source.instance_id.clone()]);
    let actual = engine
        .try_plan_mana_production(
            &source,
            None,
            true,
            true,
            &visited,
            ManaProductionMode::Activation,
            &mut resolve,
        )
        .expect("supported actual projection");
    assert!(actual.is_empty(), "30life cannot be paid at20");
    let hypothetical = engine
        .try_plan_mana_production(
            &source,
            None,
            true,
            true,
            &visited,
            ManaProductionMode::Counterfactual,
            &mut resolve,
        )
        .expect("payment is irrelevant under106.7");
    assert_eq!(hypothetical.len(), 1);
    assert_eq!(hypothetical[0].symbols, vec!["G"]);
}

#[test]
fn shared_mana_planner_does_not_drop_a_granted_production_error() {
    let mut engine = engine();
    let source = source(mana_rule(vec![
        json!({ "kind": "tap", "object": { "kind": "self" } }),
    ]));
    let modifier = json!({ "kind": "staticAbility", "source": { "kind": "self" },
        "activeWhile": { "kind": "inZone", "object": { "kind": "self" }, "zone": { "kind": "battlefield" } },
        "modifiers": [{ "kind": "grantManaAbility", "objects": { "kind": "permanents", "where": { "kind": "cardTypeContains", "value": "Land" } }, "mana": "{U}" }] });
    assert!(
        rule_is_executable(&modifier),
        "supporting modifier must be a real runtime contract"
    );
    engine.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("grant", "Enchantment", vec![modifier]),
        0,
    ));
    let error = engine
        .try_plan_mana_production(
            &source,
            None,
            true,
            true,
            &BTreeSet::from([source.instance_id.clone()]),
            ManaProductionMode::Counterfactual,
            &mut |value: &Value, _, _, _| {
                if value == "{U}" {
                    Err("granted-source-projection-unsupported")
                } else {
                    Ok(vec![vec!["G".into()]])
                }
            },
        )
        .expect_err("cannot return only the printed G branch after a grant error");
    assert_eq!(error, "granted-source-projection-unsupported");
}

#[test]
fn shared_mana_planner_keeps_attached_trigger_output_with_its_own_source() {
    let mut engine = engine();
    let source = source(mana_rule(vec![
        json!({ "kind": "tap", "object": { "kind": "self" } }),
    ]));
    let trigger = json!({ "kind": "triggeredAbility", "source": { "kind": "self" },
        "event": { "kind": "attachedPermanentManaAbilityActivated", "object": { "kind": "self" } },
        "effects": [{ "kind": "addMana", "player": { "kind": "controllerOf", "object": { "kind": "triggeringPermanent" } }, "mana": "{U}" }] });
    assert!(rule_is_executable(&trigger));
    let mut attachment = card_instance(
        "player-0",
        &definition("attachment", "Enchantment", vec![trigger]),
        0,
    );
    attachment.attached_to = Some(source.instance_id.clone());
    engine.state.players[0]
        .battlefield
        .extend([source.clone(), attachment]);
    let mut resolve = |value: &Value, _: &str, _: &BTreeSet<String>, _: Option<&CardInstance>| {
        if value == "{U}" {
            Err("other-object-trigger")
        } else {
            Ok(vec![vec!["G".into()]])
        }
    };
    let visited = BTreeSet::from([source.instance_id.clone()]);
    let actual = engine
        .try_plan_mana_production(
            &source,
            None,
            true,
            true,
            &visited,
            ManaProductionMode::Activation,
            &mut resolve,
        )
        .expect_err("actual activation still includes attached mana trigger");
    assert_eq!(actual, "other-object-trigger");
    let hypothetical = engine
        .try_plan_mana_production(
            &source,
            None,
            true,
            true,
            &visited,
            ManaProductionMode::Counterfactual,
            &mut resolve,
        )
        .expect("another object's output is not this permanent's type");
    assert_eq!(hypothetical.len(), 1);
    assert_eq!(hypothetical[0].symbols, vec!["G"]);
}
