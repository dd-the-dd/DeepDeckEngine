use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalObjectReference, NumericExpression, NumericObjectProperty, ObjectCardType, ObjectKind,
    ObjectPredicate, PlayerPredicate, TriggeringPlayerBinding,
};
use super::{
    OracleTokenCursor, recognize_dynamic_mana_payment, recognize_ordinal_filtered_spell_cast_event,
    recognize_ordinal_spell_tax_draw_triggered_ability,
};

#[test]
fn ordinal_spell_event_retains_per_player_ordinal_and_negative_filter() {
    let tokens = recognize_entities("an opponent casts their first noncreature spell each turn");
    let mut cursor = OracleTokenCursor::new("ordinal-spell-event-test", &tokens);

    let event = recognize_ordinal_filtered_spell_cast_event(&mut cursor).expect("event parses");

    assert_eq!(event.caster, PlayerPredicate::Opponent);
    assert_eq!(event.ordinal, 1);
    assert_eq!(
        event.filter,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(
                ObjectCardType::Creature
            ))),
            ObjectPredicate::ObjectKind(ObjectKind::Spell),
        ])
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn ordinal_spell_event_reuses_the_shared_composed_object_query() {
    let tokens =
        recognize_entities("an opponent casts their second instant or sorcery spell each turn");
    let mut cursor = OracleTokenCursor::new("ordinal-spell-event-test", &tokens);

    let event = recognize_ordinal_filtered_spell_cast_event(&mut cursor)
        .expect("composed spell predicate parses");

    assert_eq!(event.ordinal, 2);
    assert_eq!(
        event.filter,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Instant),
                ObjectPredicate::CardType(ObjectCardType::Sorcery),
            ]),
            ObjectPredicate::ObjectKind(ObjectKind::Spell),
        ])
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn dynamic_payment_resolves_source_toughness_as_numeric_expression() {
    let tokens =
        recognize_entities("unless that player pays {X}, where X is this permanent's toughness");
    let mut cursor = OracleTokenCursor::new("dynamic-payment-test", &tokens);

    let payment = recognize_dynamic_mana_payment(&mut cursor, Some(TriggeringPlayerBinding))
        .expect("dynamic payment parses");

    assert_eq!(
        payment.amount,
        NumericExpression::ObjectProperty {
            object: LocalObjectReference::SourceSelf,
            property: NumericObjectProperty::Toughness,
        }
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn exact_ordinal_spell_tax_draw_ability_consumes_every_token() {
    let tokens = recognize_entities(
        "Whenever an opponent casts their first noncreature spell each turn, draw a card unless that player pays {X}, where X is this creature's power.",
    );
    let mut cursor = OracleTokenCursor::new("ordinal-spell-tax-draw-test", &tokens);

    let ability = recognize_ordinal_spell_tax_draw_triggered_ability(&mut cursor)
        .expect("exact ability parses");

    assert_eq!(ability.event.ordinal, 1);
    assert_eq!(
        ability.payment.amount,
        NumericExpression::ObjectProperty {
            object: LocalObjectReference::SourceSelf,
            property: NumericObjectProperty::Power,
        }
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn non_turn_period_is_a_committed_structured_failure() {
    let tokens = recognize_entities(
        "Whenever an opponent casts their first noncreature spell each game, draw a card unless that player pays {X}, where X is this creature's power.",
    );
    let mut cursor = OracleTokenCursor::new("ordinal-spell-tax-draw-test", &tokens);

    let failure = recognize_ordinal_spell_tax_draw_triggered_ability(&mut cursor)
        .expect_err("unsupported period must fail");

    assert_eq!(failure.state, "event.ordinal-spell-cast.period");
    assert_eq!(failure.actual.as_deref(), Some("game"));
    assert_eq!(failure.expected, "turn");
}
