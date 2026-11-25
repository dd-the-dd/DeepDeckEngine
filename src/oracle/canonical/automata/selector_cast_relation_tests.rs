//! The cast relation of the shared object query and its origin (`spells
//! you cast from your graveyard`, CR 601.2a), and the keyword-ability
//! qualifier of a parameterized keyword (`creatures you control with
//! toxic`).
use super::*;
use crate::oracle::canonical::{
    CastOrigin, CastOriginZone, KeywordAbilityName, ObjectQualifier, ObjectQueryNode,
    ObjectRelation, recognize_spell_cast_collection, recognize_spell_cast_collection_with_origin,
};
use crate::oracle::syntax::recognize_entities;

fn graveyard() -> SelectorPredicate {
    SelectorPredicate::Relation(ObjectRelation::CastFrom(CastOrigin {
        zone: CastOriginZone::Graveyard,
        caster_owned: true,
    }))
}

#[test]
fn a_cast_origin_is_taken_only_by_a_parent_recording_the_cast_zone() {
    let tokens = recognize_entities("Spells you cast from your graveyard cost");
    let mut cursor = OracleTokenCursor::new("cast-origin-test", &tokens);
    let spells = recognize_spell_cast_collection_with_origin(&mut cursor).expect("origin read");
    assert_eq!(spells.caster, Some(SpellCasterScope::AbilityController));
    assert_eq!(
        spells.predicate,
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::ObjectKind(ObjectKind::Spell),
            graveyard(),
        ])
    );
    assert_eq!(cursor.peek(), Some("cost"));

    // Any other consumer refuses the origin at its `from`.
    let mut cursor = OracleTokenCursor::new("cast-origin-test", &tokens);
    let refused = recognize_spell_cast_collection(&mut cursor).expect_err("origin refused");
    assert_eq!(refused.state, "selector.cast-origin.consumer", "{refused}");
}

#[test]
fn origin_negation_and_alternatives_compose_the_same_atom() {
    let read = |text: &str| {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("cast-origin-test", &tokens);
        recognize_spell_cast_collection_with_origin(&mut cursor)
            .unwrap_or_else(|failure| panic!("{text}: {failure}"))
            .predicate
    };
    let hand = SelectorPredicate::Relation(ObjectRelation::CastFrom(CastOrigin {
        zone: CastOriginZone::Hand,
        caster_owned: true,
    }));
    assert_eq!(
        read("Spells you cast from anywhere other than your hand"),
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::ObjectKind(ObjectKind::Spell),
            SelectorPredicate::Not(Box::new(hand)),
        ])
    );
    let exile = SelectorPredicate::Relation(ObjectRelation::CastFrom(CastOrigin {
        zone: CastOriginZone::Exile,
        caster_owned: false,
    }));
    assert_eq!(
        read("Creature spells you cast from your graveyard or from exile"),
        SelectorPredicate::Intersection(vec![
            SelectorPredicate::Intersection(vec![
                SelectorPredicate::CardType(ObjectCardType::Creature),
                SelectorPredicate::ObjectKind(ObjectKind::Spell),
            ]),
            SelectorPredicate::Union(vec![graveyard(), exile]),
        ])
    );
}

#[test]
fn a_from_without_a_cast_relation_or_a_zone_stays_with_the_parent() {
    // `from` after a control relation is the parent's (`return ... from`).
    let tokens = recognize_entities("creatures you control from your graveyard");
    let mut cursor = OracleTokenCursor::new("cast-origin-test", &tokens);
    let query: ObjectQueryNode = recognize_object_query(&mut cursor).expect("query read");
    assert_eq!(
        query.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert_eq!(cursor.peek(), Some("from"));
    // `your exile` names no zone a spell is cast from.
    let tokens = recognize_entities("Spells you cast from your exile");
    let mut cursor = OracleTokenCursor::new("cast-origin-test", &tokens);
    recognize_spell_cast_collection_with_origin(&mut cursor).expect("relation read");
    assert_eq!(cursor.peek(), Some("from"));
}

#[test]
fn toxic_is_a_keyword_ability_qualifier_of_the_last_head() {
    let tokens = recognize_entities("creature you control with toxic");
    let mut cursor = OracleTokenCursor::new("toxic-test", &tokens);
    let query = recognize_object_query(&mut cursor).expect("toxic qualifier read");
    cursor.finish().expect("whole phrase read");
    assert!(matches!(
        &query.predicate,
        SelectorPredicate::Intersection(operands) if operands.contains(
            &SelectorPredicate::Qualifier(ObjectQualifier::KeywordAbility(KeywordAbilityName::Toxic))
        )
    ));
}
