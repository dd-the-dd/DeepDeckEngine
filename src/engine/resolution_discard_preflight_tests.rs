use super::*;

#[test]
fn discard_risk_is_found_inside_a_structured_program_before_its_first_action() {
    assert!(requires_checkpoint(
        &json!({"kind":"spellAbility","effects":[
            {"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}},
            {"kind":"conditionalEffects","then":[{"kind":"discardCards","count":1}]}
        ]})
    ));
}

#[test]
fn discard_words_are_not_decision_nodes_and_existing_permissions_still_work() {
    assert!(!requires_checkpoint(
        &json!({"kind":"spellAbility","name":"discardCards",
        "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":2}}]})
    ));
    assert!(requires_checkpoint(
        &json!({"kind":"optionalEffects","effects":[]})
    ));
}
