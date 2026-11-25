//! The comparative and relational `with` qualifiers of the shared object
//! query, its contracted-copula complement with articles, and the card zone
//! history relative clause.

use super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn read_query(text: &str) -> MachineResult<super::super::ObjectQueryNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-comparative-test", &tokens);
    recognize_object_query(&mut cursor)
}

#[test]
fn explicit_anchor_compares_the_candidate_with_the_source_property() {
    let query = read_query("creatures with greater power than this creature can't")
        .expect("explicit comparative anchor parses");
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands) if operands.iter().any(|operand| matches!(
            operand,
            ObjectPredicate::NumericComparison {
                left: SelectorNumericProperty::Power,
                operator: NumericComparisonOperator::GreaterThan,
                right,
            } if matches!(right.as_ref(), super::super::NumericExpression::ObjectProperty {
                object: super::super::LocalObjectReference::SourceSelf,
                property: super::super::NumericObjectProperty::Power,
            })
        ))
    ));
}

#[test]
fn inclusive_comparative_reads_equal_or_lesser() {
    let query = read_query("creature with equal or lesser toughness than this creature dies")
        .expect("inclusive comparative parses");
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands) if operands.iter().any(|operand| matches!(
            operand,
            ObjectPredicate::NumericComparison {
                left: SelectorNumericProperty::Toughness,
                operator: NumericComparisonOperator::LessThanOrEqual,
                ..
            }
        ))
    ));
}

#[test]
fn anchorless_comparative_without_a_subject_is_refused() {
    // No clause subject, antecedent or event object: nothing to compare with.
    assert!(read_query("creatures with greater power can't").is_err());
}

#[test]
fn keyword_after_with_stays_a_keyword_qualifier() {
    let query = read_query("creatures with flying can't").expect("keyword qualifier parses");
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands)
            if operands.iter().any(|operand| matches!(operand, ObjectPredicate::HasKeyword(_)))
    ));
}

#[test]
fn same_name_relation_reads_a_bound_anchor() {
    let query = read_query("creature with the same name as this creature gets")
        .expect("name relation parses");
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands) if operands.iter().any(|operand| matches!(
            operand,
            ObjectPredicate::Relation(super::super::ObjectRelation::Related(related))
                if related.kind == super::super::ObjectRelationKind::Shares(
                    super::super::SharedCharacteristic::Name,
                )
        ))
    ));
}

#[test]
fn contracted_copula_takes_articles_on_each_alternative() {
    let query = read_query("creature you control that's a Wolf or a Werewolf gets")
        .expect("articled copular disjunction parses");
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands) if operands.iter().any(|operand| matches!(
            operand,
            ObjectPredicate::Union(members)
                if members.contains(&ObjectPredicate::Subtype("Wolf".to_string()))
                    && members.contains(&ObjectPredicate::Subtype("Werewolf".to_string()))
        ))
    ));
    let query = read_query("legendary permanent that's an artifact, creature, or enchantment.")
        .expect("articled serial copular list parses");
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands) if operands.iter().any(|operand| matches!(
            operand,
            ObjectPredicate::Union(members) if members.len() == 3
        ))
    ));
}

#[test]
fn copular_complement_with_its_own_controller_is_refused() {
    assert!(read_query("creature that's a Wolf an opponent controls gets").is_err());
}

#[test]
fn zone_history_clause_is_a_card_fact() {
    let tokens = recognize_entities("that were put there from the battlefield this turn.");
    let mut cursor = OracleTokenCursor::new("selector-zone-history-test", &tokens);
    assert!(super::super::card_zone_relative_clause_starts(&cursor));
    let predicate =
        super::super::recognize_relative_clause(&mut cursor).expect("zone history parses");
    assert_eq!(
        predicate,
        ObjectPredicate::Status(super::super::ObjectStatus::PutThereFromBattlefieldThisTurn)
    );
    assert!(predicate.supports_domain(ObjectKind::Card));
    assert!(!predicate.supports_domain(ObjectKind::Permanent));
    assert_eq!(cursor.peek(), Some("."));
}
