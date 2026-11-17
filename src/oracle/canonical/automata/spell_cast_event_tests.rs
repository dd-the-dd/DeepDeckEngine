use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn controller_filtered_query_leaves_the_complete_body_to_its_parent() {
    let tokens = recognize_entities("Whenever you cast a creature spell, draw a card.");
    let mut cursor = OracleTokenCursor::new("spell-event-test", &tokens);
    let SpellCastEventAttempt::Recognized(event) = try_recognize_spell_cast_event(&mut cursor)
    else {
        panic!("a typed spell-cast event is expected");
    };
    assert_eq!(event.caster, SpellCastEventCaster::Controller);
    assert!(event.query.predicate.has_domain(ObjectKind::Spell));
    assert!(event.query.caster.is_none());
    assert!(event.ordinal.is_none());
    assert_eq!(cursor.peek(), Some("draw"));
}

#[test]
fn any_player_ordinal_binds_only_the_unfiltered_casters_turn_history() {
    let tokens = recognize_entities(
        "Whenever a player casts their second spell each turn, you lose 1 life.",
    );
    let mut cursor = OracleTokenCursor::new("spell-event-test", &tokens);
    let SpellCastEventAttempt::Recognized(event) = try_recognize_spell_cast_event(&mut cursor)
    else {
        panic!("an unfiltered ordinal event is expected");
    };
    assert_eq!(event.caster, SpellCastEventCaster::AnyPlayer);
    assert_eq!(
        event.query.predicate,
        ObjectPredicate::ObjectKind(ObjectKind::Spell)
    );
    assert_eq!(
        event.ordinal,
        Some(SpellCastEventOrdinal {
            value: 2,
            basis: SpellCastOrdinalBasis::AllSpellsByCasterThisTurn,
        })
    );
    assert_eq!(cursor.peek(), Some("you"));
}

#[test]
fn unrelated_event_restores_nonzero_checkpoint_and_prior_trace() {
    let tokens = recognize_entities("prefix Whenever you draw a card, gain 1 life.");
    let mut cursor = OracleTokenCursor::new("spell-event-test", &tokens);
    cursor.consume("prefix").unwrap();
    cursor.enter("caller-frame");
    let position = cursor.position();
    let trace = cursor.transitions().to_vec();
    assert!(matches!(
        try_recognize_spell_cast_event(&mut cursor),
        SpellCastEventAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), position);
    assert_eq!(&cursor.transitions()[..trace.len()], trace.as_slice());
    assert_eq!(cursor.transitions().last().unwrap().state, "rollback");
}

#[test]
fn filtered_ordinal_missing_domain_and_extra_caster_are_not_approximated() {
    for (text, state) in [
        (
            "Whenever you cast your second creature spell each turn, draw a card.",
            "event.spell-cast.ordinal-basis",
        ),
        (
            "Whenever you cast a creature, draw a card.",
            "event.spell-cast.spell-domain",
        ),
        (
            "Whenever you cast a creature spell you cast, draw a card.",
            "event.spell-cast.caster-identity",
        ),
        (
            "Whenever a player casts their second spell, draw a card.",
            "event.spell-cast.ordinal-period",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("spell-event-test", &tokens);
        let SpellCastEventAttempt::Unpromoted(failure) =
            try_recognize_spell_cast_event(&mut cursor)
        else {
            panic!("must retain a structured failure for {text}");
        };
        assert_eq!(failure.state, state);
    }
}

#[test]
fn atom_is_shared_and_only_an_explicit_consumer_can_preserve_filtered_history() {
    let tokens = recognize_entities("an opponent casts their second creature spell each turn,");
    let mut cursor = OracleTokenCursor::new("shared-spell-atom", &tokens);
    let event = recognize_spell_cast_event_atom(
        &mut cursor,
        SpellCastOrdinalSupport::MatchingSpellOrdinals,
    )
    .expect("the existing filtered-tax contract is explicit");
    assert_eq!(event.caster, SpellCastEventCaster::Opponent);
    assert_eq!(event.triggering_player, TriggeringPlayerBinding);
    assert_eq!(
        event.ordinal.unwrap().basis,
        SpellCastOrdinalBasis::MatchingSpellsByCasterThisTurn
    );
    assert_eq!(cursor.peek(), Some(","));
    let mut cursor = OracleTokenCursor::new("global-ordinal-only", &tokens);
    let failure = recognize_spell_cast_event_atom(
        &mut cursor,
        SpellCastOrdinalSupport::UnfilteredOrdinalsOnly,
    )
    .expect_err("a global-history consumer cannot drop its filter");
    assert_eq!(failure.state, "event.spell-cast.ordinal-basis");
}
