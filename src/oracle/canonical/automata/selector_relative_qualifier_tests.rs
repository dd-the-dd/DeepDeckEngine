use super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::characteristic_vocabulary::CardColor;
use crate::oracle::syntax::recognize_entities;

#[test]
fn relative_characteristic_composes_with_the_base_query() {
    let tokens = recognize_entities("spell you cast that's red or green costs");
    let mut cursor = OracleTokenCursor::new("selector-relative-test", &tokens);
    let query =
        recognize_spell_cast_query(&mut cursor).expect("relative spell-caster query parses");

    assert_eq!(query.caster, Some(SpellCasterScope::AbilityController));
    assert!(matches!(
        query.predicate,
        ObjectPredicate::Intersection(ref operands)
            if operands.iter().any(|operand| matches!(operand,
                ObjectPredicate::Union(colors)
                    if colors.contains(&ObjectPredicate::Color(CardColor::Red))
                        && colors.contains(&ObjectPredicate::Color(CardColor::Green))))
    ));
    assert_eq!(cursor.peek(), Some("costs"));
}

#[test]
fn relative_query_preserves_a_parent_separator() {
    let tokens = recognize_entities("creature that's red or green, draw");
    let mut cursor = OracleTokenCursor::new("selector-relative-separator-test", &tokens);
    let query =
        recognize_object_query(&mut cursor).expect("relative query before separator parses");

    assert!(matches!(query.predicate, ObjectPredicate::Intersection(_)));
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn empty_relative_and_second_controller_are_rejected() {
    let tokens = recognize_entities("creature that's enters");
    let mut cursor = OracleTokenCursor::new("selector-relative-negative-test", &tokens);
    assert!(recognize_object_query(&mut cursor).is_err());
    // A comma before `that's` is not a list connector: it is left to the
    // parent, which has no relative clause to attach.
    let tokens = recognize_entities("creature, that's red or green enters");
    let mut cursor = OracleTokenCursor::new("selector-relative-negative-test", &tokens);
    let query = recognize_object_query(&mut cursor).expect("the head alone");
    assert!(matches!(query.predicate, ObjectPredicate::CardType(_)));
    assert_eq!(cursor.peek(), Some(","));

    let tokens = recognize_entities("spell you cast that's red you control costs");
    let mut cursor = OracleTokenCursor::new("selector-relative-caster-negative-test", &tokens);
    assert!(recognize_spell_cast_query(&mut cursor).is_err());
}

#[test]
fn default_object_query_never_reinterprets_cast_as_control() {
    let tokens = recognize_entities("spell you cast costs");
    let mut cursor = OracleTokenCursor::new("selector-control-vs-cast-test", &tokens);
    let failure = recognize_object_query(&mut cursor)
        .expect_err("ordinary object queries only recognize controller relations");

    assert_eq!(
        failure.state,
        "selector.controller-qualifier.relation-or-negation"
    );
    assert_eq!(failure.actual.as_deref(), Some("cast"));
}
