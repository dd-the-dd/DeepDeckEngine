use super::super::{
    AddManaEffectNode, ManaChoiceDomain, ObjectControllerScope, recognize_add_mana_effect,
};
use crate::oracle::{canonical::OracleTokenCursor, syntax::recognize_entities};
fn read(text: &str) -> AddManaEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("mana-type-reference-test", &tokens);
    let effect = recognize_add_mana_effect(&mut cursor).expect("shared mana child");
    cursor.finish().expect("full child consumption");
    effect
}
#[test]
fn mana_type_reference_preserves_amount_and_primitive_value() {
    for (amount, word) in [(1, "one"), (3, "three"), (2, "2")] {
        let effect = read(&format!(
            "Add {word} mana of any type that a land you control could produce"
        ));
        let AddManaEffectNode::ScopedColor(node) = &effect else {
            panic!("typed scoped reference")
        };
        assert_eq!(node.amount, amount);
        assert_eq!(
            node.domain,
            ManaChoiceDomain::LandTypesControlledByAbilityController
        );
        let lowered = crate::oracle::canonical::effects::lower_add_mana_effect(&effect);
        assert_eq!(
            lowered["mana"],
            serde_json::json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":amount})
        );
    }
}
#[test]
fn mana_type_reference_does_not_broaden_scope_or_partial_suffix() {
    for text in [
        "Add one mana of any type",
        "Add one mana of any type that a land an opponent controls could produce",
        "Add one mana of any type that an artifact you control could produce",
        "Add one mana of any type that a land you control can produce",
        "Add one mana of any type that a land you control could produce except blue",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("mana-type-near-miss", &tokens);
        assert!(
            recognize_add_mana_effect(&mut cursor)
                .and_then(|_| cursor.finish())
                .is_err(),
            "{text}"
        );
    }
}
#[test]
fn color_reference_retains_old_opponent_domain_and_any_color() {
    let AddManaEffectNode::ScopedColor(node) =
        read("Add two mana of any color that a land an opponent controls could produce")
    else {
        panic!("existing scoped color")
    };
    assert_eq!(node.amount, 2);
    assert_eq!(
        node.domain,
        ManaChoiceDomain::LandColorsControlledBy(
            ObjectControllerScope::OpponentOfAbilityController
        )
    );
    assert!(matches!(
        read("Add one mana of any color"),
        AddManaEffectNode::AnyColor(_)
    ));
}

#[test]
fn mana_type_amount_i32_max_is_parser_only_and_exact() {
    let AddManaEffectNode::ScopedColor(node) =
        read("Add 2147483647 mana of any type that a land you control could produce")
    else {
        panic!("scoped type")
    };
    assert_eq!(node.amount, i64::from(i32::MAX));
}
#[test]
fn mana_type_amount_above_runtime_quantity_fails_before_lowering() {
    let tokens =
        recognize_entities("Add 2147483648 mana of any type that a land you control could produce");
    let mut cursor = OracleTokenCursor::new("mana-type-overflow", &tokens);
    let failure = recognize_add_mana_effect(&mut cursor)
        .expect_err("above i32 MAX must not become native type production");
    assert_eq!(failure.state, "effect.add-any-color-mana.type-amount");
}
