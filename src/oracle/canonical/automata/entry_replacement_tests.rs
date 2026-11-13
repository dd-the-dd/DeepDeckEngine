use crate::oracle::syntax::recognize_entities;

use super::super::{
    ObjectCardType, ObjectCastStatus, ObjectPredicate, ReplacementDestination,
    ReplacementObjectBinding,
};
use super::{
    OracleTokenCursor, recognize_exile_instead_replacement, recognize_object_not_cast_condition,
    recognize_would_enter_battlefield_event,
};

fn recognize(
    text: &str,
) -> super::super::super::MachineResult<super::WouldEnterBattlefieldEventNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("entry-replacement-test", &tokens);
    cursor.consume("if")?;
    let event = recognize_would_enter_battlefield_event(&mut cursor)?;
    let condition = recognize_object_not_cast_condition(&mut cursor, event.binding)?;
    let replacement = recognize_exile_instead_replacement(&mut cursor, event.binding)?;
    cursor.consume(".")?;
    cursor.finish()?;
    assert_eq!(condition.object, event.binding);
    assert_eq!(condition.status, ObjectCastStatus::NotCast);
    assert_eq!(replacement.object, event.binding);
    assert_eq!(replacement.destination, ReplacementDestination::Exile);
    Ok(event)
}

#[test]
fn implicit_battlefield_entry_composes_independent_typed_children() {
    let event =
        recognize("If a nontoken creature would enter and it wasn't cast, exile it instead.")
            .expect("implicit battlefield entry replacement parses");

    assert_eq!(event.binding, ReplacementObjectBinding::EventObject);
    assert_eq!(
        event.object,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
            ObjectPredicate::CardType(ObjectCardType::Creature),
        ])
    );
}

#[test]
fn object_type_and_explicit_battlefield_destination_can_vary() {
    let event = recognize(
        "If a nontoken artifact would enter the battlefield and it wasn't cast, exile it instead.",
    )
    .expect("explicit battlefield sibling parses");

    assert_eq!(
        event.object,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::IsToken)),
            ObjectPredicate::CardType(ObjectCardType::Artifact),
        ])
    );
}

#[test]
fn anaphor_and_provenance_near_misses_fail_at_their_own_state() {
    let tokens =
        recognize_entities("If a nontoken creature would enter and it was cast, exile it instead.");
    let mut cursor = OracleTokenCursor::new("entry-replacement-test", &tokens);
    cursor.consume("if").expect("envelope starts");
    let event = recognize_would_enter_battlefield_event(&mut cursor).expect("event parses");

    let failure = recognize_object_not_cast_condition(&mut cursor, event.binding)
        .expect_err("positive cast status must not become a not-cast condition");

    assert_eq!(failure.actual.as_deref(), Some("was"));
    assert_eq!(failure.expected, "negativePredicateWord");
}

#[test]
fn nonpermanent_entry_subject_is_rejected_by_domain_validation() {
    let tokens = recognize_entities("an instant would enter the battlefield");
    let mut cursor = OracleTokenCursor::new("entry-replacement-test", &tokens);

    let failure = recognize_would_enter_battlefield_event(&mut cursor)
        .expect_err("instant is not a battlefield permanent domain");

    assert_eq!(failure.expected, "a permanent object predicate");
}
