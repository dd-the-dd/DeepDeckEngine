use super::*;

#[test]
fn destroy_permanent_requires_a_boolean_regeneration_prohibition() {
    let effect_without_prohibition = json!({
        "kind": "destroyPermanent",
        "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
    });
    assert!(effect_supported(&effect_without_prohibition));

    for value in [Value::Bool(false), Value::Bool(true)] {
        let mut effect = effect_without_prohibition.clone();
        effect["cannotRegenerate"] = value;
        assert!(effect_supported(&effect));
    }

    let mut malformed = effect_without_prohibition;
    malformed["cannotRegenerate"] = Value::String("yes".to_string());
    assert!(!effect_supported(&malformed));
}
