use crate::oracle::syntax::recognize_entities;

use super::super::{GrantedKeyword, KeywordAtomNode};
use super::{OracleTokenCursor, recognize_standalone_keyword_ability};

fn recognize(text: &str) -> super::super::super::MachineResult<Vec<KeywordAtomNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("keyword-reminder-registry-test", &tokens);
    recognize_standalone_keyword_ability(&mut cursor).map(|list| list.keywords)
}

/// Every wording below is printed on real cards in the local Scryfall data.
const PRINTED_REMINDERS: &[(&str, GrantedKeyword)] = &[
    (
        "Flying (This creature can't be blocked except by creatures with flying or reach.)",
        GrantedKeyword::Flying,
    ),
    (
        "Infect (This creature deals damage to creatures in the form of -1/-1 counters and to players in the form of poison counters.)",
        GrantedKeyword::Infect,
    ),
    (
        "Shadow (This creature can block or be blocked by only creatures with shadow.)",
        GrantedKeyword::Shadow,
    ),
    (
        "Fear (This creature can't be blocked except by artifact creatures and/or black creatures.)",
        GrantedKeyword::Fear,
    ),
    (
        "Shroud (This creature can't be the target of spells or abilities.)",
        GrantedKeyword::Shroud,
    ),
    (
        "Horsemanship (This creature can't be blocked except by creatures with horsemanship.)",
        GrantedKeyword::Horsemanship,
    ),
    (
        "Undying (When this creature dies, if it had no +1/+1 counters on it, return it to the battlefield under its owner's control with a +1/+1 counter on it.)",
        GrantedKeyword::Undying,
    ),
    (
        "Intimidate (This creature can't be blocked except by artifact creatures and/or creatures that share a color with it.)",
        GrantedKeyword::Intimidate,
    ),
    (
        "Skulk (This creature can't be blocked by creatures with greater power.)",
        GrantedKeyword::Skulk,
    ),
    (
        "Trample (This creature can deal excess combat damage to the player it's attacking.)",
        GrantedKeyword::Trample,
    ),
    (
        "Trample (This creature can deal excess combat damage to the player she's attacking.)",
        GrantedKeyword::Trample,
    ),
    (
        "Trample (This creature can deal excess combat damage to the player they're attacking.)",
        GrantedKeyword::Trample,
    ),
    (
        "Haste (This creature can attack and {T} as soon as he comes under your control.)",
        GrantedKeyword::Haste,
    ),
    (
        "Haste (This creature can attack as soon as it comes under your control.)",
        GrantedKeyword::Haste,
    ),
    (
        "Flash (You may cast this card any time you could cast an instant.)",
        GrantedKeyword::Flash,
    ),
    (
        "Indestructible (Damage and effects that say \"destroy\" don't destroy this creature. If its toughness is 0 or less, it still dies.)",
        GrantedKeyword::Indestructible,
    ),
    (
        "Indestructible (Damage and effects that say \"destroy\" don't destroy this creature. If its toughness is 0 or less, it's still put into its owner's graveyard.)",
        GrantedKeyword::Indestructible,
    ),
];

#[test]
fn printed_reminders_validate_against_their_own_keyword_atom() {
    for (text, keyword) in PRINTED_REMINDERS {
        assert_eq!(
            recognize(text).unwrap_or_else(|failure| panic!("{text}: {failure}")),
            vec![KeywordAtomNode::Parameterless(*keyword)],
            "{text}"
        );
    }
}

#[test]
fn a_reminder_composes_on_the_final_atom_of_a_keyword_list() {
    assert_eq!(
        recognize(
            "Flying, shadow (This creature can block or be blocked by only creatures with shadow.)"
        )
        .expect("reminder belongs to the final atom"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Shadow),
        ]
    );
}

#[test]
fn a_reminder_of_another_keyword_or_an_altered_wording_fails_explicitly() {
    let wrong_keyword =
        recognize("Flying (This creature can't be blocked except by creatures with horsemanship.)")
            .expect_err("flying must not accept the horsemanship reminder");
    assert_eq!(wrong_keyword.state, "keyword.reminder.body");
    assert_eq!(wrong_keyword.actual.as_deref(), Some("horsemanship"));

    let altered = recognize("Undying (When this creature dies, if it had no -1/-1 counters on it, return it to the battlefield under its owner's control with a +1/+1 counter on it.)")
        .expect_err("undying names +1/+1 counters");
    assert_eq!(altered.state, "keyword.undying.reminder.condition-counter");

    let tail = recognize("Indestructible (Damage and effects that say \"destroy\" don't destroy this creature. If its toughness is 0 or less, it explodes.)")
        .expect_err("unknown indestructible tail");
    assert_eq!(tail.state, "keyword.indestructible.reminder.zero-toughness");
}
