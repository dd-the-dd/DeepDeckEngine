use crate::oracle::syntax::recognize_entities;

use super::super::{ObjectCardType, ObjectPredicate};
use super::{OracleTokenCursor, recognize_object_query};

fn predicate(text: &str) -> ObjectPredicate {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-shared-head-test", &tokens);
    let query = recognize_object_query(&mut cursor).unwrap_or_else(|f| panic!("{text}: {f:?}"));
    assert_eq!(cursor.position(), tokens.len(), "{text}");
    query.predicate
}

fn subtype(value: &str) -> ObjectPredicate {
    ObjectPredicate::Subtype(value.to_string())
}

fn creature() -> ObjectPredicate {
    ObjectPredicate::CardType(ObjectCardType::Creature)
}

#[test]
fn coordinated_prenominal_subtypes_share_the_creature_head() {
    // Prosperous Thief: type Creature AND (subtype Ninja OR subtype Rogue).
    for text in ["Ninja or Rogue creatures", "Ninja or Rogue creature"] {
        assert_eq!(
            predicate(text),
            ObjectPredicate::Intersection(vec![
                ObjectPredicate::Union(vec![subtype("Ninja"), subtype("Rogue")]),
                creature(),
            ]),
            "{text}"
        );
    }
}

#[test]
fn colors_and_postnominal_keywords_compose_with_the_shared_head() {
    let ObjectPredicate::Intersection(operands) = predicate("white or blue creatures with flying")
    else {
        panic!("shared head intersection");
    };
    // (white OR blue) AND creature, then the keyword qualifies that one head.
    let [headed, keyword] = operands.as_slice() else {
        panic!("headed branch and keyword: {operands:?}");
    };
    let ObjectPredicate::Intersection(headed) = headed else {
        panic!("shared head intersection: {headed:?}");
    };
    assert!(matches!(&headed[0], ObjectPredicate::Union(colors) if colors.len() == 2));
    assert_eq!(headed[1], creature());
    assert!(matches!(keyword, ObjectPredicate::HasKeyword(_)));
}

#[test]
fn branches_with_their_own_head_remain_independent() {
    // A card type, or a plural subtype noun, heads its own branch.
    for text in [
        "artifacts or Rogue creatures",
        "Ninjas or Rogue creatures",
        "creature or artifact",
    ] {
        let ObjectPredicate::Union(branches) = predicate(text) else {
            panic!("{text}: independent branches stay a union");
        };
        assert_eq!(branches.len(), 2, "{text}");
    }
}
