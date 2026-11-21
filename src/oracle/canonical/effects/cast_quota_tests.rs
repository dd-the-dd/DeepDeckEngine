use super::*;
use crate::oracle::syntax::recognize_entities;

fn lowered(text: &str, type_line: &str) -> MachineResult<CanonicalRuleDraft> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("quota-lowering", &tokens);
    let CastQuotaRecognition::Parsed(node) = try_recognize_cast_quota(&mut cursor) else {
        panic!("test input must have a complete typed quota: {text}")
    };
    lower_cast_quota(&mut cursor, node, type_line)
}

#[test]
fn exact_noncreature_quota_emits_existing_atomic_modifier() {
    let draft = lowered(
        "Each player can't cast more than one noncreature spell each turn.",
        "Enchantment",
    )
    .unwrap();
    assert_eq!(
        draft.rule,
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"castLimitPerTurn","players":{"kind":"eachPlayer"},"where":{"kind":"not","operand":{"kind":"cardTypeContains","value":"Creature"}},"maximum":{"kind":"integer","value":1}}]})
    );
}

#[test]
fn unsupported_populations_nonpositive_amounts_and_spell_faces_are_capped() {
    for text in [
        "Your opponents can't cast more than one noncreature spell each turn.",
        "Each player can't cast more than 0 noncreature spells each turn.",
        "Each player can't cast more than X noncreature spells each turn.",
        "Each player can't cast more than one red spell each turn.",
    ] {
        assert!(lowered(text, "Enchantment").is_err(), "{text}");
    }
    assert!(
        lowered(
            "Each player can't cast more than one noncreature spell each turn.",
            "Instant"
        )
        .is_err()
    );
}

#[test]
fn history_support_recursively_rejects_unsupported_children_even_under_negation() {
    let supported = json!({"kind":"cardTypeContains","value":"Creature"});
    let unsupported = json!({"kind":"colorContains","value":"R"});
    for bad in [
        unsupported.clone(),
        json!({"kind":"not","operand":unsupported.clone()}),
        json!({"kind":"and","operands":[supported.clone(),unsupported.clone()]}),
        json!({"kind":"or","operands":[supported.clone(),unsupported.clone()]}),
        json!({"kind":"not","operand":{"kind":"or","operands":[supported.clone(),unsupported]}}),
    ] {
        assert!(!quota_history_filter_supported(&bad), "{bad}");
    }
    assert!(quota_history_filter_supported(
        &json!({"kind":"and","operands":[supported.clone(),{"kind":"not","operand":supported}]})
    ));
    assert!(quota_history_filter_supported(&Value::Null));
    assert_eq!(
        lowered(
            "Each player can't cast more than one spell each turn.",
            "Enchantment"
        )
        .unwrap()
        .rule["modifiers"][0]["where"],
        Value::Null
    );
}

#[test]
fn each_player_loss_lowers_to_one_simultaneous_loss_over_the_population() {
    let node = QuantifiedPlayerActionNode {
        players: PlayerSetReferenceNode::EachPlayer,
        excluding_controller: false,
        actions: vec![QuantifiedPlayerAction::Life(LifeActionNode {
            action: TargetPlayerLifeAction::Lose,
            amount: NumericExpression::Atom(NumericAtom::Integer(1)),
        })],
    };
    assert_eq!(
        lower_quantified_player_action(&node, TargetlessDeclarationSupport::None).unwrap(),
        json!({"kind":"loseLife","player":{"kind":"eachPlayer"},"amount":{"kind":"integer","value":1}})
    );
}
