//! Shared object-query atoms: block status adjectives, coordinated type
//! adjectives over one head, card names with epithets and alternatives,
//! separately headed card alternatives, and the declared-target relation of
//! a stack object to its own targets.

use super::super::{DamagedPlayer, GrantedKeyword, ObjectRelation, ObjectStatus};
use super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn noun_query(text: &str, context: ObjectQueryContext) -> (RecognizedObjectQuery, Option<String>) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-query-atoms-test", &tokens);
    let query = recognize_object_noun_query(&mut cursor, context).expect("the noun phrase parses");
    (query, cursor.peek().map(str::to_string))
}

#[test]
fn blocked_and_unblocked_are_status_adjectives_of_the_shared_query() {
    for (text, status) in [
        ("blocked creature", ObjectStatus::Blocked),
        ("unblocked creatures", ObjectStatus::Unblocked),
    ] {
        let (query, rest) = noun_query(text, ObjectQueryContext::ORDINARY);
        assert_eq!(rest, None);
        assert_eq!(
            query.predicate(),
            &ObjectPredicate::Intersection(vec![
                ObjectPredicate::Status(status),
                ObjectPredicate::CardType(ObjectCardType::Creature),
            ])
        );
    }
    assert_eq!(ObjectStatus::Blocked.ir_atom(), ("isBlocked", false));
    assert_eq!(
        ObjectStatus::Unblocked.ir_atom(),
        ("isUnblockedAttacker", false)
    );
    // An unregistered status keeps the atom diagnostic.
    let tokens = recognize_entities("saddled creature");
    let mut cursor = OracleTokenCursor::new("object-query-atoms-test", &tokens);
    let failure = recognize_object_noun_query(&mut cursor, ObjectQueryContext::ORDINARY)
        .expect_err("an unknown status is not an object predicate");
    assert_eq!(failure.state, "selector.predicate.start");
}

#[test]
fn coordinated_type_adjectives_share_one_head_but_headed_branches_do_not() {
    let shared = ObjectPredicate::Intersection(vec![
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Instant),
            ObjectPredicate::CardType(ObjectCardType::Sorcery),
        ]),
        ObjectPredicate::ObjectKind(ObjectKind::Card),
    ]);
    for text in ["instant and sorcery cards", "instant and/or sorcery cards"] {
        let (query, rest) = noun_query(text, ObjectQueryContext::ORDINARY);
        assert_eq!(rest, None, "{text}");
        assert_eq!(query.predicate(), &shared, "{text}");
    }
    // `instant cards and sorcery cards`: the first headed branch ends at its
    // noun; the `and` stays with the parent (a set, a sequence, a list).
    let (query, rest) = noun_query(
        "instant cards and sorcery cards",
        ObjectQueryContext::ORDINARY,
    );
    assert_eq!(rest.as_deref(), Some("and"));
    assert_eq!(
        query.predicate(),
        &ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Instant),
            ObjectPredicate::ObjectKind(ObjectKind::Card),
        ])
    );
    // A type without a following domain noun is not this coordination.
    let (_, rest) = noun_query(
        "creature and planeswalker you control",
        ObjectQueryContext::ORDINARY,
    );
    assert_eq!(rest.as_deref(), Some("and"));
}

#[test]
fn search_selector_reads_coordinated_types_names_and_keywords() {
    let read = |text: &str, plural: bool| {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("object-query-atoms-test", &tokens);
        let predicate =
            recognize_search_card_alternatives(&mut cursor, plural).expect("card phrase parses");
        (predicate, cursor.peek().map(str::to_string))
    };
    let (predicate, rest) = read("instant and/or sorcery cards in", true);
    assert_eq!(rest.as_deref(), Some("in"));
    // The search selector owns its card noun; the types are one union.
    assert_eq!(
        predicate,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Instant),
            ObjectPredicate::CardType(ObjectCardType::Sorcery),
        ])
    );

    let (predicate, rest) = read("card named Jace, Arcane Strategist, reveal", false);
    assert_eq!(rest.as_deref(), Some(","));
    assert_eq!(
        predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::ObjectKind(ObjectKind::Card),
            ObjectPredicate::Relation(ObjectRelation::Named("Jace, Arcane Strategist".to_string())),
        ])
    );

    let (predicate, _) = read("card named Festering Newt or Bubbling Cauldron,", false);
    assert!(matches!(predicate, ObjectPredicate::Intersection(ref items)
    if items.contains(&ObjectPredicate::Union(vec![
        ObjectPredicate::Relation(ObjectRelation::Named("Festering Newt".to_string())),
        ObjectPredicate::Relation(ObjectRelation::Named("Bubbling Cauldron".to_string())),
    ]))));

    // `named Halvar, God of Battle or an Equipment card`: the second branch
    // is a headed card phrase, not another name.
    let (predicate, rest) = read(
        "card named Halvar, God of Battle or an Equipment card,",
        false,
    );
    assert_eq!(rest.as_deref(), Some(","));
    assert!(matches!(predicate, ObjectPredicate::Union(ref branches) if branches.len() == 2));

    let (predicate, _) = read("instant card or a card with flash,", false);
    assert!(matches!(predicate, ObjectPredicate::Union(ref branches)
    if branches[1] == ObjectPredicate::Intersection(vec![
        ObjectPredicate::ObjectKind(ObjectKind::Card),
        ObjectPredicate::HasKeyword(GrantedKeyword::Flash),
    ])));

    // A singular `a X and/or a Y` names two slots: the union stops.
    let (_, rest) = read(
        "card named Alpine Watchdog and/or a card named Igneous Cur,",
        false,
    );
    assert_eq!(rest.as_deref(), Some("and/or"));
}

#[test]
fn spell_targets_relation_is_read_only_for_declared_target_candidates() {
    let text = "spell that targets you or a creature you control.";
    let (query, rest) = noun_query(text, ObjectQueryContext::DECLARED_TARGET);
    assert_eq!(rest.as_deref(), Some("."));
    let ObjectPredicate::Intersection(items) = query.predicate() else {
        panic!("spell head and relation")
    };
    let Some(ObjectPredicate::Relation(ObjectRelation::Targets(targeted))) = items.last() else {
        panic!("targets relation")
    };
    assert_eq!(targeted.players, Some(DamagedPlayer::AbilityController));
    let permanents = targeted.permanents.as_ref().expect("permanent alternative");
    assert_eq!(
        permanents.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert!(query.predicate().is_in_domain(ObjectKind::Spell));
    assert!(!query.predicate().supports_domain(ObjectKind::Permanent));

    // An ordinary query (a spell-cast event subject) leaves `that targets`
    // to its parent's own typed reader.
    let (_, rest) = noun_query(text, ObjectQueryContext::ORDINARY);
    assert_eq!(rest.as_deref(), Some("that"));
    // `targets only a single ...` is a count of targets, not this relation.
    let (_, rest) = noun_query(
        "spell that targets only a single creature.",
        ObjectQueryContext::DECLARED_TARGET,
    );
    assert_eq!(rest.as_deref(), Some("that"));
}
