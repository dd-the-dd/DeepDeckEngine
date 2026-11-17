use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{recognize_search_card_selector, ObjectCardType, ObjectKind, ObjectPredicate};

fn exact_cost(raw: &str) -> ObjectPredicate {
    let tokens = recognize_entities(raw);
    let mut cursor = OracleTokenCursor::new("exact-cost-test", &tokens);
    ObjectPredicate::ExactManaCost(
        super::recognize_mana_cost(&mut cursor).expect("test mana cost parses"),
    )
}

#[test]
fn card_type_and_exact_cost_union_are_independent_predicates() {
    let tokens = recognize_entities("enchantment card with mana cost {2} or {3}");
    let mut cursor = OracleTokenCursor::new("search-card-selector-test", &tokens);
    let predicate = recognize_search_card_selector(&mut cursor, false).expect("selector parses");
    cursor.finish().expect("selector consumes all tokens");

    assert_eq!(
        predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
            ObjectPredicate::Union(vec![exact_cost("{2}"), exact_cost("{3}")]),
        ])
    );
}

#[test]
fn exact_cost_qualifier_requires_a_real_or_branch() {
    let tokens = recognize_entities("artifact card with mana cost {0} or");
    let mut cursor = OracleTokenCursor::new("search-card-selector-test", &tokens);
    let failure = recognize_search_card_selector(&mut cursor, false).expect_err("must fail");
    assert_eq!(failure.state, "cost.mana.start");
}

#[test]
fn bare_card_is_the_explicit_card_domain_predicate() {
    let tokens = recognize_entities("card");
    let mut cursor = OracleTokenCursor::new("search-card-selector-test", &tokens);
    let predicate = recognize_search_card_selector(&mut cursor, false).expect("selector parses");
    cursor.finish().expect("selector consumes all tokens");
    assert_eq!(predicate, ObjectPredicate::ObjectKind(ObjectKind::Card));
}

#[test]
fn card_type_or_list_is_one_boolean_union() {
    let tokens = recognize_entities("artifact or enchantment card");
    let mut cursor = OracleTokenCursor::new("search-card-selector-test", &tokens);
    let predicate = recognize_search_card_selector(&mut cursor, false).expect("selector parses");
    cursor.finish().expect("selector consumes all tokens");
    assert_eq!(
        predicate,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ])
    );
}
