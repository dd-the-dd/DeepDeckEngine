use super::lower_registered_ability_signal;

#[test]
fn lowers_only_explicit_rules_backed_completion_signals() {
    let player = serde_json::json!({ "kind": "abilityController" });
    assert_eq!(
        lower_registered_ability_signal("surveil", player.clone()),
        Some(serde_json::json!({
            "kind": "emitAbilitySignal",
            "abilityId": "surveil",
            "player": player,
        }))
    );
    assert!(lower_registered_ability_signal("revolt", serde_json::json!({})).is_none());
    assert!(lower_registered_ability_signal("unknown", serde_json::json!({})).is_none());
}

#[test]
fn amass_and_explore_register_one_completion_after_their_program() {
    for identity in ["amass", "explore"] {
        let player = serde_json::json!({ "kind": "abilityController" });
        assert_eq!(
            lower_registered_ability_signal(identity, player.clone()),
            Some(serde_json::json!({
                "kind": "emitAbilitySignal",
                "abilityId": identity,
                "player": player,
            }))
        );
        let registration = crate::oracle::ability_catalog::lookup_registry(identity)
            .expect("rules-backed action registration");
        assert_eq!(registration.semantics.signal.cardinality, "oncePerAction");
        assert_eq!(
            registration.semantics.signal.zero_quantity.as_deref(),
            Some(if identity == "amass" {
                "emit"
            } else {
                "notApplicable"
            })
        );
    }
}
