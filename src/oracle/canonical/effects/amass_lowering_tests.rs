use super::*;

fn amass(count: NumericExpression) -> AmassEffectNode {
    AmassEffectNode {
        player: LocalPlayerReference::Controller,
        army_subtype: AmassArmySubtype::Orc,
        count,
    }
}

fn contains_kind(value: &Value, kind: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.get("kind").and_then(Value::as_str) == Some(kind)
                || object.values().any(|value| contains_kind(value, kind))
        }
        Value::Array(values) => values.iter().any(|value| contains_kind(value, kind)),
        _ => false,
    }
}

#[test]
fn procedural_action_never_uses_a_scalar_runtime_opcode() {
    let node = LocalEffectNode::Amass(amass(NumericExpression::from(1)));
    assert_eq!(
        lower_targetless_local_effect(&node, TargetlessDeclarationSupport::None).unwrap_err(),
        TargetlessLoweringError::UnsupportedEffect,
    );
    let effects =
        lower_targetless_local_effects(&node, TargetlessDeclarationSupport::None, Some("effect2"))
            .unwrap();
    assert!(!contains_kind(&json!(effects), "amass"));
}

#[test]
fn create_then_choose_one_then_counters_then_subtype_then_completion() {
    let effects = lower_amass_effects(
        &amass(NumericExpression::from(1)),
        TargetlessDeclarationSupport::None,
        Some("effect2"),
    )
    .unwrap();
    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0]["then"][0]["kind"], "createTokens");
    assert_eq!(effects[0]["then"][0]["token"]["power"], 0);
    assert_eq!(effects[0]["then"][0]["token"]["name"], "Orc Army Token");
    let chosen = &effects[1]["then"];
    assert_eq!(chosen[0]["kind"], "choosePermanents");
    assert_eq!(chosen[0]["minimum"]["value"], 1);
    assert_eq!(chosen[0]["maximum"]["value"], 1);
    assert_eq!(
        chosen[0]["candidates"]["where"]["operands"][0]["value"],
        "Creature"
    );
    assert_eq!(chosen[1]["kind"], "putCounters");
    assert_eq!(chosen[2]["effects"][0]["kind"], "addSubtypeToPermanent");
    assert_eq!(effects[2]["kind"], "emitAbilitySignal");
    assert_eq!(effects[2]["abilityId"], "amass");
    assert!(
        effects[1]["condition"].is_object(),
        "an impossible creation must not force an empty choice"
    );
}

#[test]
fn zero_still_chooses_and_signals_without_binding_to_successful_counters() {
    let first = lower_amass_effects(
        &amass(NumericExpression::from(0)),
        TargetlessDeclarationSupport::None,
        Some("effect0"),
    )
    .unwrap();
    let second = lower_amass_effects(
        &amass(NumericExpression::from(0)),
        TargetlessDeclarationSupport::None,
        Some("effect1"),
    )
    .unwrap();
    assert_ne!(first[1]["then"][0]["id"], second[1]["then"][0]["id"]);
    assert_eq!(first[1]["then"][1]["count"]["value"], 0);
    assert_eq!(
        first[1]["then"][1]["permanent"]["decisionId"],
        first[1]["then"][0]["id"]
    );
    assert!(first[1]["then"][1].get("bind").is_none());
    assert_eq!(first.last().unwrap()["abilityId"], "amass");
}

#[test]
fn variable_count_requires_the_existing_declaration_contract() {
    let node = amass(NumericExpression::Atom(NumericAtom::VariableX));
    assert_eq!(
        lower_amass_effects(&node, TargetlessDeclarationSupport::None, None).unwrap_err(),
        TargetlessLoweringError::RequiresDeclaration
    );
    let effects =
        lower_amass_effects(&node, TargetlessDeclarationSupport::XValue, Some("effect0")).unwrap();
    assert_eq!(effects[1]["then"][1]["count"], decision_result("xValue"));
}
