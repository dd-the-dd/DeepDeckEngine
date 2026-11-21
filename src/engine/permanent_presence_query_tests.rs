use super::*;
use rand::RngCore;

fn definition(id: &str, types: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":types,"rules":rules})).unwrap()
}
fn game() -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        151115,
    )
    .unwrap();
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].land_plays_remaining = 1;
    engine
}
fn source() -> CardInstance {
    card_instance("p0", &definition("entry", "Land", vec![]), 0)
}
fn condition() -> Value {
    json!({"kind":"controlsPermanent","player":{"kind":"controllerOf","object":{"kind":"self"}},
        "where":{"kind":"or","operands":[{"kind":"subtypeContains","value":"Island"},{"kind":"subtypeContains","value":"Swamp"}]}})
}
fn replacement() -> Value {
    json!({"kind":"replacementEffect","source":{"kind":"self"},"event":{"kind":"wouldEnterBattlefield","object":{"kind":"self"}},
        "condition":{"kind":"not","operand":condition()},
        "replacement":[{"kind":"setEnteringState","object":{"kind":"self"},"tapped":true}]})
}
fn witness(types: &str) -> CardInstance {
    card_instance("p0", &definition("witness", types, vec![]), 0)
}
fn grant(kind: &str, subtype: &str) -> CardInstance {
    let rule = json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":kind,"objects":{"kind":"permanents","where":{"kind":"nameEquals","value":"witness"}},"subtype":subtype}]});
    assert!(rule_is_executable(&rule));
    card_instance(
        "p0",
        &definition(
            &format!("grant-{kind}-{subtype}"),
            "Enchantment",
            vec![rule],
        ),
        0,
    )
}
fn result(e: &GameEngine) -> Result<bool, PresenceQueryError> {
    e.checked_entry_condition_with_source(&condition(), "p0", &BTreeMap::new(), &source())
}

#[test]
fn controlled_membership_uses_effective_basic_land_types_without_basic_supertype() {
    for (types, modifier, expected) in [
        ("Land - Island", None, true),
        ("Land - Swamp", None, true),
        ("Land - Plains", Some(("setLandSubtype", "Island")), true),
        ("Land - Island", Some(("setLandSubtype", "Mountain")), false),
        ("Land - Plains", Some(("addSubtype", "Island")), true),
    ] {
        let mut e = game();
        e.state.players[0].battlefield.push(witness(types));
        if let Some((kind, subtype)) = modifier {
            e.state.players[0].battlefield.push(grant(kind, subtype));
        }
        assert_eq!(result(&e).unwrap(), expected, "{types} {modifier:?}");
    }
}

#[test]
fn phased_and_opposing_objects_are_absent_and_real_controller_is_authoritative() {
    let mut e = game();
    let mut p = witness("Land - Island");
    p.flags.insert("phasedOut".into(), true);
    e.state.players[0].battlefield.push(p);
    assert!(!result(&e).unwrap());
    e.state.players[0].battlefield[0].flags.remove("phasedOut");
    e.state.players[0].battlefield[0].controller = "p1".into();
    assert!(
        !result(&e).unwrap(),
        "storage bucket must not supply controller"
    );
    e.state.players[0].battlefield[0].controller = "p0".into();
    assert!(result(&e).unwrap());
}

#[test]
fn characteristic_dependencies_and_unclosed_orders_are_explicit_errors() {
    let mut e = game();
    e.state.players[0]
        .battlefield
        .push(witness("Land - Plains"));
    e.state.players[0]
        .battlefield
        .push(grant("setLandSubtype", "Island"));
    e.state.players[0]
        .battlefield
        .push(grant("setLandSubtype", "Mountain"));
    assert_eq!(
        result(&e).unwrap_err().state,
        "presence.query.unclosed-layer-order"
    );
    e.state.players[0].battlefield.pop();
    e.state.players[0].battlefield[1].definition.rules[0]["condition"] = condition();
    assert_eq!(
        result(&e).unwrap_err().state,
        "presence.query.characteristic-dependency"
    );
    e.state.players[0].battlefield[1].definition.rules[0]
        .as_object_mut()
        .unwrap()
        .remove("condition");
    e.state.players[0].battlefield[1].definition.rules[0]["modifiers"][0]["objects"]["where"] =
        json!({"kind":"subtypeContains","value":"Plains"});
    assert_eq!(
        result(&e).unwrap_err().state,
        "presence.query.selector-dependency"
    );
}

#[test]
fn failed_land_entry_restores_hand_quota_identity_clocks_events_and_rng_then_retries() {
    let mut e = game();
    let entering = card_instance("p0", &definition("entry", "Land", vec![replacement()]), 0);
    e.state.players[0].hand.push(entering);
    e.admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let action = e
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|action| action.kind == ActionKind::PlayLand)
        .unwrap();
    e.state.players[0].battlefield.extend([
        witness("Land - Plains"),
        grant("setLandSubtype", "Island"),
        grant("setLandSubtype", "Mountain"),
    ]);
    e.admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let state = serde_json::to_value(&e.state).unwrap();
    let mut rng = e.rng.clone();
    let snapshots = e.quantity_snapshot_count();
    let error = e
        .apply_priority_action(&action, &mut RandomAi::seeded(4))
        .unwrap_err();
    assert!(
        error
            .message
            .contains("presence.query.unclosed-layer-order")
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), state);
    assert_eq!(e.rng.clone().next_u64(), rng.next_u64());
    assert_eq!(e.quantity_snapshot_count(), snapshots + 1);
    e.state.players[0].battlefield.pop();
    e.apply_priority_action(&action, &mut RandomAi::seeded(4))
        .unwrap();
    assert!(e.state.players[0].hand.is_empty());
    assert_eq!(e.state.players[0].land_plays_remaining, 0);
    assert!(
        !e.state.players[0]
            .battlefield
            .iter()
            .find(|p| p.definition.id == "entry")
            .unwrap()
            .tapped
    );
}

#[test]
fn ordered_result_boolean_fold_skips_suffix_but_preserves_evaluated_errors() {
    for (conjunction, first) in [(true, false), (false, true)] {
        let mut evaluated = 0;
        let result = try_boolean_operands([Ok(first), Err("unused")], conjunction, |value| {
            evaluated += 1;
            value
        });
        assert_eq!(result, Ok(first));
        assert_eq!(evaluated, 1);
    }
    assert_eq!(
        try_boolean_operands([Err("required"), Ok(true)], false, |v| v),
        Err("required")
    );
    assert_eq!(
        try_boolean_operands([Err("required"), Ok(false)], true, |v| v),
        Err("required")
    );
}

#[test]
fn entry_conditions_delegate_only_evaluated_presence_leaves() {
    let mut e = game();
    e.state.players[0].battlefield.extend([
        witness("Land - Plains"),
        grant("setLandSubtype", "Island"),
        grant("setLandSubtype", "Mountain"),
    ]);
    for (kind, left, expected) in [("and", false, false), ("or", true, true)] {
        let constant =
            json!({"kind":"compare","operator":"==","left":1,"right":if left {1} else {0}});
        let c = json!({"kind":kind,"operands":[constant,condition()]});
        assert_eq!(
            e.checked_entry_condition_with_source(&c, "p0", &BTreeMap::new(), &source())
                .unwrap(),
            expected
        );
    }
    for (kind, left) in [("and", true), ("or", false)] {
        let constant =
            json!({"kind":"compare","operator":"==","left":1,"right":if left {1} else {0}});
        let c = json!({"kind":kind,"operands":[constant,condition()]});
        assert_eq!(
            e.checked_entry_condition_with_source(&c, "p0", &BTreeMap::new(), &source())
                .unwrap_err()
                .state,
            "presence.query.unclosed-layer-order"
        );
    }
}

#[test]
fn private_predicate_fold_is_lazy_without_weakening_outer_structural_caps() {
    let mut e = game();
    let island = witness("Land - Island");
    e.state.players[0].battlefield.push(island.clone());
    for (kind, first, expected) in [
        (
            "or",
            json!({"kind":"subtypeContains","value":"Island"}),
            true,
        ),
        (
            "and",
            json!({"kind":"not","operand":{"kind":"subtypeContains","value":"Island"}}),
            false,
        ),
    ] {
        // Isolates the private fold's evaluation contract. The public filter
        // capability still rejects this malformed tree before evaluation.
        let filter = json!({"kind":kind,"operands":[first,{"kind":"unknown"}]});
        assert_eq!(
            e.checked_presence_filter(&island, &filter, 0).unwrap(),
            expected
        );
        let mut c = condition();
        c["where"] = filter;
        assert_eq!(
            e.checked_entry_condition_with_source(&c, "p0", &BTreeMap::new(), &source())
                .unwrap_err()
                .state,
            "presence.query.filter-domain"
        );
    }
    assert!(
        e.checked_presence_filter(&island, &json!({"kind":"and","operands":[]}), 0)
            .is_err()
    );
    assert!(
        e.checked_entry_condition_with_source(
            &json!({"kind":"or","operands":[]}),
            "p0",
            &BTreeMap::new(),
            &source()
        )
        .is_err()
    );
}

#[test]
fn unordered_existence_accepts_a_proved_witness_in_both_storage_orders() {
    let good = card_instance("p0", &definition("good", "Land - Island", vec![]), 0);
    for reverse in [false, true] {
        let mut e = game();
        let mut objects = vec![good.clone(), witness("Land - Plains")];
        if reverse {
            objects.reverse();
        }
        e.state.players[0].battlefield.extend(objects);
        e.state.players[0].battlefield.extend([
            grant("setLandSubtype", "Island"),
            grant("setLandSubtype", "Mountain"),
        ]);
        assert!(result(&e).unwrap(), "reverse={reverse}");
        e.state.players[0]
            .battlefield
            .retain(|p| p.definition.id != "good");
        assert_eq!(
            result(&e).unwrap_err().state,
            "presence.query.unclosed-layer-order"
        );
    }
}

#[test]
fn known_basic_land_types_use_the_same_case_policy_as_membership() {
    let mut e = game();
    e.state.players[0].battlefield.extend([
        witness("Land - Island"),
        grant("setLandSubtype", "Mountain"),
    ]);
    let mut c = condition();
    c["where"] = json!({"kind":"subtypeContains","value":"island"});
    let before = c.clone();
    assert!(
        !e.checked_entry_condition_with_source(&c, "p0", &BTreeMap::new(), &source())
            .unwrap()
    );
    assert_eq!(c, before, "classification does not rewrite raw IR strings");
    assert!(!basic_land_subtype(
        &json!({"kind":"subtypeContains","value":"Islandish"})
    ));
}
