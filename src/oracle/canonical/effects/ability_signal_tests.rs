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
