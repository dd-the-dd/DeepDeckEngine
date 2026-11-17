use crate::oracle::syntax::recognize_entities;

use super::recognize_destroy_permanent_collection_effect;
use crate::oracle::canonical::{
    ObjectControllerScope, ObjectKind, OracleTokenCursor, SelectorPredicate,
};

fn recognize(text: &str) -> super::super::DestroyPermanentCollectionEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.destroy-collection", &tokens);
    let node = recognize_destroy_permanent_collection_effect(&mut cursor)
        .expect("collection destruction parses");
    assert_eq!(cursor.position(), tokens.len());
    node
}

#[test]
fn nonland_permanents_of_opponents_use_shared_query_and_controller_scope() {
    let node = recognize("Destroy all nonland permanents your opponents control");
    assert!(node.objects.predicate.has_domain(ObjectKind::Permanent));
    assert_eq!(
        node.objects.controller,
        Some(ObjectControllerScope::OpponentOfAbilityController)
    );
}

#[test]
fn collection_accepts_unbounded_selector_branches() {
    let node = recognize(
        "Destroy all artifacts, enchantments, creatures, and planeswalkers your opponents control",
    );
    assert!(node.objects.predicate.has_domain(ObjectKind::Permanent));
    assert_eq!(
        node.objects.controller,
        Some(ObjectControllerScope::OpponentOfAbilityController)
    );
}

#[test]
fn trailing_numeric_qualifier_scopes_every_coordinated_type() {
    let node =
        recognize("Destroy each artifact, creature, and enchantment with mana value X or less");
    let SelectorPredicate::Intersection(shared) = node.objects.predicate else {
        panic!("shared qualifier must surround the coordinated type union");
    };
    assert!(matches!(shared.first(), Some(SelectorPredicate::Union(types)) if types.len() == 3));
    assert!(matches!(
        shared.get(1),
        Some(SelectorPredicate::NumericComparison { .. })
    ));
}

#[test]
fn qualifier_on_an_earlier_branch_remains_branch_local() {
    let node = recognize("Destroy all artifacts with flying and creatures");
    let SelectorPredicate::Union(branches) = node.objects.predicate else {
        panic!("branch-local qualifier must not surround the complete union");
    };
    assert!(matches!(
        branches.first(),
        Some(SelectorPredicate::Intersection(operands))
            if operands.iter().any(|operand| matches!(operand, SelectorPredicate::HasKeyword(_)))
    ));
    assert!(matches!(
        branches.get(1),
        Some(SelectorPredicate::CardType(_))
    ));
}

#[test]
fn independently_qualified_branches_keep_both_filters_local() {
    let node = recognize("Destroy all artifacts with mana value 1 and creatures with mana value 3");
    let SelectorPredicate::Union(branches) = node.objects.predicate else {
        panic!("independently qualified branches must remain a union");
    };
    assert_eq!(branches.len(), 2);
    assert!(branches.iter().all(|branch| matches!(
        branch,
        SelectorPredicate::Intersection(operands)
            if operands.iter().any(|operand| matches!(operand, SelectorPredicate::NumericComparison { .. }))
    )));
}

#[test]
fn controller_on_an_intermediate_branch_stops_before_the_connector() {
    let tokens = recognize_entities("Destroy all artifacts you control and creatures");
    let mut cursor = OracleTokenCursor::new("test.destroy-collection", &tokens);
    let node = recognize_destroy_permanent_collection_effect(&mut cursor)
        .expect("the first controlled collection is structurally valid");
    assert_eq!(
        node.objects.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert_eq!(cursor.peek(), Some("and"));
}

#[test]
fn unsupported_collection_quantifier_fails_at_the_quantifier_state() {
    let tokens = recognize_entities("Destroy every creature");
    let mut cursor = OracleTokenCursor::new("test.destroy-collection", &tokens);
    let failure = recognize_destroy_permanent_collection_effect(&mut cursor)
        .expect_err("unsupported quantifier must not be approximated as each");
    assert_eq!(failure.state, "destroy.collection.quantifier");
    assert_eq!(failure.actual.as_deref(), Some("every"));
}

#[test]
fn cards_without_a_permanent_domain_fail_visibly() {
    let tokens = recognize_entities("Destroy all cards");
    let mut cursor = OracleTokenCursor::new("test.destroy-collection", &tokens);
    let failure = recognize_destroy_permanent_collection_effect(&mut cursor)
        .expect_err("cards are not an explicit permanent collection");
    assert_eq!(failure.state, "destroy.collection.permanent-domain");
    assert!(failure.expected.contains("battlefield-permanent"));
}

#[test]
fn cards_from_a_source_zone_do_not_cross_into_the_permanent_domain() {
    let tokens = recognize_entities("Destroy all creature cards in your graveyard");
    let mut cursor = OracleTokenCursor::new("test.destroy-collection", &tokens);
    let failure = recognize_destroy_permanent_collection_effect(&mut cursor)
        .expect_err("graveyard cards are not battlefield permanents");
    assert_eq!(failure.state, "destroy.collection.permanent-domain");
    assert_eq!(failure.actual.as_deref(), Some("in"));
}

#[test]
fn plural_controller_relation_rejects_unknown_ownership_word() {
    let tokens = recognize_entities("Destroy all creatures your opponents own");
    let mut cursor = OracleTokenCursor::new("test.destroy-collection", &tokens);
    let failure = recognize_destroy_permanent_collection_effect(&mut cursor)
        .expect_err("ownership is not a controller relation");
    assert_eq!(
        failure.state,
        "selector.controller-qualifier.relation-or-negation"
    );
    assert_eq!(failure.actual.as_deref(), Some("own"));
}
