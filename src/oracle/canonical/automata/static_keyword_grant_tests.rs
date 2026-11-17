use crate::oracle::canonical::{ObjectCardType, ObjectPredicate, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

use super::recognize_static_riot_grant;

#[test]
fn nontoken_creature_set_delegates_to_the_shared_selector() {
    let tokens = recognize_entities(
        "Nontoken creatures you control have riot. (They enter with your choice of a +1/+1 counter or haste.)",
    );
    let mut cursor = OracleTokenCursor::new("static-riot-grant-test", &tokens);
    let grant = recognize_static_riot_grant(&mut cursor).expect("grant parses");

    assert_eq!(
        grant.objects.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
            ObjectPredicate::CardType(ObjectCardType::Creature),
        ])
    );
    assert!(grant.keyword.reminder.is_some());
}

#[test]
fn other_permanent_type_reuses_the_same_grant_envelope() {
    let tokens = recognize_entities("Nontoken artifacts you control have riot.");
    let mut cursor = OracleTokenCursor::new("static-riot-grant-test", &tokens);
    let grant = recognize_static_riot_grant(&mut cursor).expect("grant parses");

    assert_eq!(
        grant.objects.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
            ObjectPredicate::CardType(ObjectCardType::Artifact),
        ])
    );
}
