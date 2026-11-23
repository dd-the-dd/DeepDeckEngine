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

#[test]
fn invariant_nouns_agree_with_either_number() {
    // `Equipment`, `Merfolk` and `Plains` have no distinct plural surface in
    // Oracle lists, so they never make a list's number disagree.
    for text in [
        "Auras, Equipment, or Vehicles you control",
        "Elf, Merfolk, or Goblin you control",
        "Forests, Plains, or Islands you control",
    ] {
        let (predicate, next) = read(text);
        assert!(
            matches!(&predicate, SelectorPredicate::Union(branches) if branches.len() == 3),
            "{text}: {predicate:?}"
        );
        assert_eq!(next, None, "{text}");
    }
}

#[test]
fn the_member_after_or_agrees_with_the_run_too() {
    // `an artifact, creature or planeswalkers you control get`: the plural
    // member after `or` does not belong to the singular list before it.
    let (predicate, next) = read("artifact, creature or planeswalkers you control get +1/+1");
    assert_eq!(predicate, card_type(ObjectCardType::Artifact));
    assert_eq!(next.as_deref(), Some(","));
}

#[test]
fn the_modifier_scope_refusal_precedes_the_number_check() {
    // `noncreature, nonland permanents or artifacts`: refused explicitly,
    // never shortened to a bare `noncreature`.
    let tokens = recognize_entities("noncreature, nonland permanents or artifacts");
    let mut cursor = OracleTokenCursor::new("transactional-coordinator", &tokens);
    let failure = recognize_object_query(&mut cursor).expect_err("unproved modifier scope");
    assert_eq!(
        failure.expected,
        "a negative-modifier list whose head is not coordinated with another branch"
    );
}

#[test]
fn a_trailing_domain_noun_takes_the_list_number_not_the_member_number() {
    // Corpus audit: `creature, land, and/or planeswalker cards` and `Vampire,
    // Cleric, and/or Demon spells` are lists whose last member carries the
    // shared plural domain noun.
    for text in [
        "creature, land, and/or planeswalker cards",
        "Vampire, Cleric, and/or Demon spells",
        "Mutant, Ninja, or Turtle spells",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("transactional-coordinator", &tokens);
        recognize_object_query(&mut cursor).expect(text);
        assert_eq!(cursor.peek(), None, "{text}");
    }
}
