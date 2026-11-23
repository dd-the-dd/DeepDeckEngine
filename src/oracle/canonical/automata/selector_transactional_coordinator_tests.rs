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

#[test]
fn a_run_of_negative_modifiers_without_a_head_is_not_a_list() {
    // `noncreature, nonland` has no noun to modify: the comma is returned
    // instead of composing a union of two bare modifiers.
    let (predicate, next) = read("noncreature, nonland, draw a card");
    assert_eq!(
        predicate,
        SelectorPredicate::Not(Box::new(card_type(ObjectCardType::Creature)))
    );
    assert_eq!(next.as_deref(), Some(","));
}

#[test]
fn modifiers_of_one_head_followed_by_or_are_refused() {
    // `nonartifact, nonblack creature or planeswalker`: the modifiers' scope
    // over the `or` branch is unproved, so the list fails explicitly instead
    // of becoming a union with a bare `nonartifact`.
    let tokens = recognize_entities("nonartifact, nonblack creature or planeswalker");
    let mut cursor = OracleTokenCursor::new("transactional-coordinator", &tokens);
    let failure = recognize_object_query(&mut cursor).expect_err("unproved modifier scope");
    assert_eq!(
        failure.expected,
        "a negative-modifier list whose head is not coordinated with another branch"
    );
    // Near miss: the explicit `, or` union of modifiers keeps its reading.
    let (predicate, next) = read("noncreature, or nonland permanent");
    assert!(matches!(predicate, SelectorPredicate::Intersection(_)));
    assert_eq!(next, None);
}

#[test]
fn an_or_closing_members_of_different_number_is_the_parent_clause() {
    // `If you control an artifact, creatures or planeswalkers you control
    // get ...`: one list's members share their number, so the comma and the
    // `or` belong to the following clause.
    let (predicate, next) = read("artifact, creatures or planeswalkers you control get +1/+1");
    assert_eq!(predicate, card_type(ObjectCardType::Artifact));
    assert_eq!(next.as_deref(), Some(","));
    // Near miss: members of the same number still form the list.
    let (predicate, next) = read("artifacts, creatures or planeswalkers you control get +1/+1");
    assert!(
        matches!(&predicate, SelectorPredicate::Union(branches) if branches.len() == 3),
        "{predicate:?}"
    );
    assert_eq!(next.as_deref(), Some("get"));
}
