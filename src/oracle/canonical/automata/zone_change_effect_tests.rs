use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalEffectNode, ObjectCardType, ObjectKind, ObjectPredicate, TargetSelector, ZoneChangeAction,
};
use super::{OracleTokenCursor, recognize_zone_change_effect};

fn recognize(text: &str) -> LocalEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("zone-change-test", &tokens);
    recognize_zone_change_effect(&mut cursor).expect("zone change parses")
}

#[test]
fn negative_card_type_qualifies_an_explicit_permanent_domain() {
    let LocalEffectNode::ZoneChange(effect) = recognize("Exile target nonland permanent") else {
        panic!("expected a zone change");
    };

    assert_eq!(effect.action, ZoneChangeAction::Exile);
    let TargetSelector::Object(predicate) = &effect.target.slots[0].selector else {
        panic!("expected an object predicate");
    };
    assert_eq!(
        predicate,
        &ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(ObjectCardType::Land))),
            ObjectPredicate::ObjectKind(ObjectKind::Permanent),
        ])
    );
}

#[test]
fn a_negative_type_does_not_establish_the_permanent_domain_by_itself() {
    let tokens = recognize_entities("Exile target nonland");
    let mut cursor = OracleTokenCursor::new("zone-change-test", &tokens);
    let failure = recognize_zone_change_effect(&mut cursor)
        .expect_err("a negative characteristic alone is not a permanent target domain");

    assert_eq!(failure.expected, "a permanent target selector");
}

#[test]
fn a_card_domain_is_not_silently_treated_as_a_battlefield_permanent() {
    let tokens = recognize_entities("Exile target nonland card");
    let mut cursor = OracleTokenCursor::new("zone-change-test", &tokens);
    let failure = recognize_zone_change_effect(&mut cursor)
        .expect_err("a card-zone selector is not a battlefield permanent selector");

    assert_eq!(failure.expected, "a permanent target selector");
}
