use super::*;
use crate::oracle::syntax::recognize_entities;

/// Reads one query and returns its predicate with the first unread token.
fn read(text: &str) -> (SelectorPredicate, Option<String>) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("transactional-coordinator", &tokens);
    let query = recognize_object_query(&mut cursor).expect(text);
    (query.predicate, cursor.peek().map(str::to_string))
}

fn card_type(value: ObjectCardType) -> SelectorPredicate {
    SelectorPredicate::CardType(value)
}

#[test]
fn a_closed_serial_list_commits_before_the_parent_separator() {
    // `If you control an artifact, creature, or enchantment, draw a card`:
    // the list is the query's; the comma before `draw` is the parent's.
    for text in [
        "artifact, creature, or enchantment, draw a card",
        "artifact, creature or enchantment, draw a card",
        "artifact, creature, and/or enchantment, draw a card",
    ] {
        let (predicate, next) = read(text);
        assert_eq!(
            predicate,
            SelectorPredicate::Union(vec![
                card_type(ObjectCardType::Artifact),
                card_type(ObjectCardType::Creature),
                card_type(ObjectCardType::Enchantment),
            ]),
            "{text}"
        );
        assert_eq!(next.as_deref(), Some(","), "{text}");
    }
}

#[test]
fn an_unclosed_comma_run_returns_its_comma_to_the_parent() {
    for (text, next) in [
        // A following clause whose subject is an object phrase.
        ("Goblin, creatures you control get +1/+1", ","),
        // A cost list and an effect list.
        ("creature, discard a card", ","),
        ("creature, then draw a card", ","),
        ("artifact, and you gain 3 life", ","),
    ] {
        let (predicate, actual) = read(text);
        assert!(
            !matches!(predicate, SelectorPredicate::Union(_)),
            "{text}: {predicate:?}"
        );
        assert_eq!(actual.as_deref(), Some(next), "{text}");
    }
}

#[test]
fn or_commits_only_before_another_object_phrase() {
    // `target creature or player`: `player` is not an object phrase, so the
    // connector is left for the target-list parent.
    let (predicate, next) = read("creature or player");
    assert_eq!(predicate, card_type(ObjectCardType::Creature));
    assert_eq!(next.as_deref(), Some("or"));
    let (predicate, next) = read("creature or planeswalker");
    assert_eq!(
        predicate,
        SelectorPredicate::Union(vec![
            card_type(ObjectCardType::Creature),
            card_type(ObjectCardType::Planeswalker),
        ])
    );
    assert_eq!(next, None);
}

#[test]
fn negative_modifiers_of_one_head_need_no_closing_connector() {
    let (predicate, next) = read("nonartifact, nonblack creature");
    assert_eq!(next, None);
    assert!(
        matches!(&predicate, SelectorPredicate::Intersection(operands) if operands.len() == 3),
        "{predicate:?}"
    );
}

#[test]
fn negated_statuses_also_modify_one_head() {
    // Unlikely Alliance: `nonattacking, nonblocking creature` is one
    // creature that is neither attacking nor blocking.
    let (predicate, next) = read("nonattacking, nonblocking creature gets +0/+2");
    assert_eq!(next.as_deref(), Some("gets"));
    assert!(
        matches!(&predicate, SelectorPredicate::Intersection(operands) if operands.len() == 3),
        "{predicate:?}"
    );
}
