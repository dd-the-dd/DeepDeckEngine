use super::*;

fn rule(effects: Vec<Value>) -> Value {
    json!({ "kind": "manaAbility", "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }], "effects": effects })
}
fn add(mana: Value) -> Value {
    json!({ "kind": "addMana", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "mana": mana })
}
fn gain() -> Value {
    json!({ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "amount": { "kind": "integer", "value": 1 } })
}

#[test]
fn undefined_zero_and_types_are_distinct_and_say_nothing_about_legality() {
    let undefined = ManaTypeProjection::from_types(BTreeSet::new(), NoManaReason::UndefinedType);
    let zero = ManaTypeProjection::from_types(BTreeSet::new(), NoManaReason::ZeroAmount);
    assert_ne!(undefined, zero);
    let colorless = ManaTypeProjection::from_types(
        BTreeSet::from([ManaType::Colorless]),
        NoManaReason::UndefinedType,
    );
    assert_ne!(colorless, undefined);
    assert_eq!(ManaType::from_symbol("C"), Some(ManaType::Colorless));
    assert_eq!(ManaType::from_symbol("c"), None);
    assert_eq!(ManaType::from_symbol("S"), None);
}

#[test]
fn pure_production_retains_original_node_and_nonproduction_followups() {
    let rule = rule(vec![add(json!("{G}")), gain()]);
    let program = certify_pure_mana_program("source-7", 2, &rule).expect("pure mana prefix");
    assert_eq!(program.source_instance_id, "source-7");
    assert_eq!(program.rule_index, 2);
    assert!(std::ptr::eq(program.mana, &rule["effects"][0]["mana"]));
    assert_eq!(program.followups.len(), 1);
}

#[test]
fn supported_runtime_ir_can_still_require_explicit_projection_caps() {
    for (effects, expected) in [
        (
            vec![gain(), add(json!("{G}"))],
            ManaProjectionUnsupportedState::StatefulPrefix,
        ),
        (
            vec![add(json!("{G}")), add(json!("{U}"))],
            ManaProjectionUnsupportedState::NestedOrRepeatedProduction,
        ),
        (
            vec![
                add(json!("{G}")),
                json!({ "kind": "optionalEffects", "player": { "kind": "controllerOf", "object": { "kind": "self" } }, "effects": [add(json!("{U}"))] }),
            ],
            ManaProjectionUnsupportedState::NestedOrRepeatedProduction,
        ),
    ] {
        let rule = rule(effects);
        assert!(
            mana_rule_supported(&rule),
            "expose real executable-IR projection gap"
        );
        let failure = certify_pure_mana_program("real-source", 4, &rule)
            .expect_err("must not project a subset");
        assert_eq!(failure.state, expected);
        assert_eq!(failure.source_instance_id, "real-source");
        assert_eq!(failure.rule_index, 4);
        assert!(
            failure
                .into_engine_error()
                .message
                .contains("unsupported counterfactual mana projection")
        );
    }
}

#[test]
fn nonmana_producer_envelope_is_a_projection_cap_not_invalid_runtime_ir() {
    let mut rule = rule(vec![add(json!("{G}"))]);
    rule["kind"] = json!("activatedAbility");
    assert!(rule_is_executable(&rule));
    let failure = certify_pure_mana_program("targeted-producer", 3, &rule)
        .expect_err("context must not silently skip other producer envelope families");
    assert_eq!(
        failure.state,
        ManaProjectionUnsupportedState::UnsupportedProducerEnvelope
    );
}
