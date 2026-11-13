use super::super::{GrantedKeyword, recognize_object_collection};
use super::*;
use crate::oracle::canonical::{NumericAtom, ObjectQueryNode};
use crate::oracle::characteristic_vocabulary::CardColor;
use crate::oracle::syntax::recognize_entities;

fn complete_query(text: &str) -> MachineResult<ObjectQueryNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-qualifier-scope", &tokens);
    let query = recognize_object_query(&mut cursor)?;
    cursor.finish()?;
    Ok(query)
}

fn card_type(value: ObjectCardType) -> SelectorPredicate {
    SelectorPredicate::CardType(value)
}

fn qualified(head: SelectorPredicate, keyword: GrantedKeyword) -> SelectorPredicate {
    SelectorPredicate::Intersection(vec![head, SelectorPredicate::HasKeyword(keyword)])
}

#[test]
fn trailing_keyword_qualifies_only_the_last_coordinated_object_head() {
    for (text, keyword) in [
        (
            "artifact, enchantment, or creature with flying",
            GrantedKeyword::Flying,
        ),
        (
            "artifact, enchantment, or creature with reach",
            GrantedKeyword::Reach,
        ),
        (
            "artifact, enchantment, or creature with vigilance",
            GrantedKeyword::Vigilance,
        ),
    ] {
        assert_eq!(
            complete_query(text).expect(text).predicate,
            SelectorPredicate::Union(vec![
                card_type(ObjectCardType::Artifact),
                card_type(ObjectCardType::Enchantment),
                qualified(card_type(ObjectCardType::Creature), keyword),
            ]),
            "{text}",
        );
    }
}

#[test]
fn single_head_keeps_existing_nested_intersection_and_qualifier_order() {
    let query = complete_query("artifact creature with flying with reach").unwrap();
    assert_eq!(
        query.predicate,
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::Intersection(vec![
                card_type(ObjectCardType::Artifact),
                card_type(ObjectCardType::Creature),
            ]),
            SelectorPredicate::HasKeyword(GrantedKeyword::Flying),
            SelectorPredicate::HasKeyword(GrantedKeyword::Reach),
        ])
    );
}

#[test]
fn local_keyword_and_shared_postfix_domain_have_independent_scopes() {
    let query = complete_query("artifact or enchantment spell with flying").unwrap();
    assert_eq!(
        query.predicate,
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::Union(vec![
                card_type(ObjectCardType::Artifact),
                qualified(
                    card_type(ObjectCardType::Enchantment),
                    GrantedKeyword::Flying
                ),
            ]),
            SelectorPredicate::ObjectKind(ObjectKind::Spell),
        ])
    );
}

#[test]
fn numeric_and_relative_qualifiers_keep_shared_query_scope() {
    let query = complete_query("artifact or creature with mana value 3 or less").unwrap();
    assert_eq!(
        query.predicate,
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::Union(vec![
                card_type(ObjectCardType::Artifact),
                card_type(ObjectCardType::Creature)
            ]),
            SelectorPredicate::NumericComparison {
                left: SelectorNumericProperty::ManaValue,
                operator: NumericComparisonOperator::LessThanOrEqual,
                right: NumericAtom::Integer(3),
            },
        ])
    );
    let query = complete_query("artifact or creature that's red or green").unwrap();
    assert_eq!(
        query.predicate,
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::Union(vec![
                card_type(ObjectCardType::Artifact),
                card_type(ObjectCardType::Creature)
            ]),
            SelectorPredicate::Union(vec![
                SelectorPredicate::Color(CardColor::Red),
                SelectorPredicate::Color(CardColor::Green)
            ]),
        ])
    );
}

#[test]
fn collection_preserves_earlier_local_qualifiers_and_shared_numeric_tail() {
    for text in [
        "artifacts with flying and creatures with reach",
        "artifacts with mana value 1 and creatures with mana value 3",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selector-qualifier-scope", &tokens);
        let query = recognize_object_collection(&mut cursor).expect(text);
        cursor
            .finish()
            .expect("complete independently qualified collection");
        assert!(
            matches!(query.predicate, SelectorPredicate::Union(ref branches)
            if branches.len() == 2 && branches.iter().all(|branch| matches!(branch, SelectorPredicate::Intersection(_)))),
            "{text}"
        );
    }
    let tokens =
        recognize_entities("artifacts, creatures, and enchantments with mana value X or less");
    let mut cursor = OracleTokenCursor::new("selector-qualifier-scope", &tokens);
    let query = recognize_object_collection(&mut cursor).unwrap();
    cursor.finish().unwrap();
    assert!(
        matches!(query.predicate, SelectorPredicate::Intersection(ref shared)
        if matches!(shared.first(), Some(SelectorPredicate::Union(branches)) if branches.len() == 3)
        && matches!(shared.get(1), Some(SelectorPredicate::NumericComparison { right: NumericAtom::VariableX, .. })))
    );
}

#[test]
fn terminal_controller_and_parent_comma_are_preserved() {
    let tokens = recognize_entities("artifact or creature with flying you control, draw");
    let mut cursor = OracleTokenCursor::new("selector-qualifier-scope", &tokens);
    let query = recognize_object_query_before_separator(&mut cursor).unwrap();
    assert_eq!(
        query.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert_eq!(
        query.predicate,
        SelectorPredicate::Union(vec![
            card_type(ObjectCardType::Artifact),
            qualified(card_type(ObjectCardType::Creature), GrantedKeyword::Flying),
        ])
    );
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn unsupported_shared_nominal_head_is_rejected_instead_of_guessing_scope() {
    let failure = complete_query("red or green creature with flying")
        .expect_err("a missing shared nominal-head node cannot authorize a guessed scope");
    assert_eq!(failure.state, "selector.predicate.qualifier-scope");
    assert!(failure.expected.contains("explicit object heads"));
}

#[test]
fn unknown_keyword_and_local_controller_continuation_still_fail_closed() {
    let failure = complete_query("artifact or creature with imaginary")
        .expect_err("unknown keyword is not a generic characteristic");
    assert_eq!(failure.state, "selector.keyword-qualifier.keyword");
    assert_eq!(failure.actual.as_deref(), Some("imaginary"));
    let failure = complete_query("artifact you control or creature")
        .expect_err("a branch-local controller cannot be hoisted over another branch");
    assert_eq!(failure.actual.as_deref(), Some("or"));
}
