use crate::oracle::{canonical::OracleTokenCursor, syntax::recognize_entities};

use super::super::{
    AddManaEffectNode, ManaChoiceDomain, ObjectControllerScope, recognize_add_mana_effect,
};

fn recognize(text: &str) -> AddManaEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("mana-domain-test", &tokens);
    let effect = recognize_add_mana_effect(&mut cursor).expect("mana effect parses");
    cursor.finish().expect("complete mana effect");
    effect
}

#[test]
fn opposing_land_color_domain_is_typed_from_the_shared_selector() {
    let AddManaEffectNode::ScopedColor(effect) =
        recognize("Add one mana of any color that a land an opponent controls could produce")
    else {
        panic!("expected a scoped color effect");
    };
    assert_eq!(effect.amount, 1);
    assert_eq!(
        effect.domain,
        ManaChoiceDomain::LandColorsControlledBy(
            ObjectControllerScope::OpponentOfAbilityController
        )
    );
}

#[test]
fn scoped_color_amount_reuses_the_numeric_child() {
    let AddManaEffectNode::ScopedColor(effect) =
        recognize("Add three mana of any color that a land an opponent controls could produce")
    else {
        panic!("expected a scoped color effect");
    };
    assert_eq!(effect.amount, 3);
}

#[test]
fn adjacent_object_and_controller_domains_fail_at_local_states() {
    for (text, state) in [
        (
            "Add one mana of any color that an artifact an opponent controls could produce",
            "effect.add-land-color-domain.land-selector.validation",
        ),
        (
            "Add one mana of any color that a land you control could produce",
            "effect.add-land-color-domain.controller-scope",
        ),
        (
            "Add one mana of any color that a land an opponent controls can produce",
            "effect.add-land-color-domain.capability",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("mana-domain-test", &tokens);
        let failure = recognize_add_mana_effect(&mut cursor).expect_err("near miss fails");
        assert_eq!(failure.state, state, "{text}");
    }
}
