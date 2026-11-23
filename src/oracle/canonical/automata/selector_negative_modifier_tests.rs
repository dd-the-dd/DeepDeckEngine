use super::*;
use crate::oracle::syntax::recognize_entities;

fn query(text: &str) -> SelectorPredicate {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("negative-modifier-query", &tokens);
    let query = recognize_object_query(&mut cursor).expect(text);
    cursor.finish().expect("complete selector");
    query.predicate
}

fn not_type(value: ObjectCardType) -> SelectorPredicate {
    SelectorPredicate::Not(Box::new(SelectorPredicate::CardType(value)))
}

#[test]
fn negative_comma_modifiers_constrain_one_permanent_head() {
    assert_eq!(
        query("noncreature, nonland permanent"),
        SelectorPredicate::Intersection(vec![
            not_type(ObjectCardType::Creature),
            not_type(ObjectCardType::Land),
            SelectorPredicate::ObjectKind(ObjectKind::Permanent),
        ])
    );
}

#[test]
fn arbitrary_length_negative_lists_keep_a_single_head() {
    assert_eq!(
        query("noncreature, nonland, nonartifact permanent"),
        SelectorPredicate::Intersection(vec![
            not_type(ObjectCardType::Creature),
            not_type(ObjectCardType::Land),
            not_type(ObjectCardType::Artifact),
            SelectorPredicate::ObjectKind(ObjectKind::Permanent),
        ])
    );
}

#[test]
fn explicit_union_and_positive_head_lists_are_not_negative_conjunctions() {
    for text in [
        "noncreature or nonland permanent",
        "noncreature, or nonland permanent",
        "noncreature, and/or nonland permanent",
    ] {
        // An explicit union of negative modifiers shares the one head:
        // permanent AND (noncreature OR nonland), never a conjunction.
        assert_eq!(
            query(text),
            SelectorPredicate::Intersection(vec![
                SelectorPredicate::Union(vec![
                    not_type(ObjectCardType::Creature),
                    not_type(ObjectCardType::Land),
                ]),
                SelectorPredicate::ObjectKind(ObjectKind::Permanent),
            ]),
            "{text}"
        );
    }
    // An unclosed list of positive heads is not a union: its comma is
    // returned to the parent (roadmap #12).
    let tokens = recognize_entities("artifact, creature");
    let mut cursor = OracleTokenCursor::new("negative-modifier-query", &tokens);
    let first = recognize_object_query(&mut cursor).expect("first selector");
    assert_eq!(
        first.predicate,
        SelectorPredicate::CardType(ObjectCardType::Artifact)
    );
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn positive_separately_headed_negative_branches_remain_a_union() {
    assert_eq!(
        query("noncreature permanent, or nonland permanent"),
        SelectorPredicate::Union(vec![
            SelectorPredicate::Intersection(vec![
                not_type(ObjectCardType::Creature),
                SelectorPredicate::ObjectKind(ObjectKind::Permanent),
            ]),
            SelectorPredicate::Intersection(vec![
                not_type(ObjectCardType::Land),
                SelectorPredicate::ObjectKind(ObjectKind::Permanent),
            ]),
        ])
    );
}

#[test]
fn parent_comma_boundary_is_preserved() {
    // A comma that does not continue the list belongs to the parent.
    let tokens = recognize_entities("noncreature permanent, draw a card");
    let mut cursor = OracleTokenCursor::new("negative-modifier-boundary", &tokens);
    let first = recognize_object_query(&mut cursor).expect("first selector");
    assert_eq!(
        first.predicate,
        SelectorPredicate::Intersection(vec![
            not_type(ObjectCardType::Creature),
            SelectorPredicate::ObjectKind(ObjectKind::Permanent),
        ])
    );
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn battlefield_domain_caps_still_reject_cards_and_unheaded_negatives() {
    for text in [
        "Destroy target noncreature, nonland",
        "Destroy target noncreature, nonland card",
        "Destroy target noncreature, nonland spell",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("negative-modifier-domain", &tokens);
        let failure = super::super::recognize_zone_change_effect(&mut cursor).expect_err(text);
        assert_eq!(failure.expected, "a permanent target selector", "{text}");
    }
}
