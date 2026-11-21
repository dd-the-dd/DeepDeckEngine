use crate::oracle::syntax::recognize_entities;

use super::super::{GrantedKeyword, KeywordAtomNode, ResourcePaymentNode};
use super::{OracleTokenCursor, recognize_standalone_keyword_ability};

fn recognize(text: &str) -> super::super::super::MachineResult<Vec<KeywordAtomNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("simple-keyword-test", &tokens);
    recognize_standalone_keyword_ability(&mut cursor).map(|list| list.keywords)
}

#[test]
fn exalted_is_a_standalone_parameterless_keyword() {
    assert_eq!(
        recognize("Exalted").expect("standalone Exalted parses"),
        vec![KeywordAtomNode::Parameterless(GrantedKeyword::Exalted)]
    );
}

#[test]
fn canonical_exalted_reminder_is_validated_by_tokens() {
    assert_eq!(
        recognize(
            "Exalted (Whenever a creature you control attacks alone, that creature gets +1/+1 until end of turn.)",
        )
        .expect("Exalted reminder parses"),
        vec![KeywordAtomNode::Parameterless(GrantedKeyword::Exalted)]
    );
}

#[test]
fn registered_parameterless_reminders_share_one_optional_child() {
    let cases = [
        (
            "Cascade (When you cast this spell, exile cards from the top of your library until you exile a nonland card that costs less. You may cast it without paying its mana cost. Put the exiled cards on the bottom in a random order.)",
            GrantedKeyword::Cascade,
        ),
        (
            "Menace (This creature can't be blocked except by two or more creatures.)",
            GrantedKeyword::Menace,
        ),
        (
            "Flash (You may cast this spell any time you could cast an instant.)",
            GrantedKeyword::Flash,
        ),
        (
            "Prowess (Whenever you cast a noncreature spell, this creature gets +1/+1 until end of turn.)",
            GrantedKeyword::Prowess,
        ),
        (
            "Trample (This creature can deal excess combat damage to the player or planeswalker it's attacking.)",
            GrantedKeyword::Trample,
        ),
        (
            "Vigilance (Attacking doesn't cause this creature to tap.)",
            GrantedKeyword::Vigilance,
        ),
        (
            "Reach (This creature can block creatures with flying.)",
            GrantedKeyword::Reach,
        ),
        (
            "Deathtouch (Any amount of damage this deals to a creature is enough to destroy it.)",
            GrantedKeyword::Deathtouch,
        ),
        (
            "Lifelink (Damage dealt by this creature also causes you to gain that much life.)",
            GrantedKeyword::Lifelink,
        ),
        (
            "First strike (This creature deals combat damage before creatures without first strike.)",
            GrantedKeyword::FirstStrike,
        ),
        (
            "Haste (This creature can attack and {T} as soon as it comes under your control.)",
            GrantedKeyword::Haste,
        ),
        (
            "Double strike (This creature deals both first-strike and regular combat damage.)",
            GrantedKeyword::DoubleStrike,
        ),
        (
            "Defender (This creature can't attack.)",
            GrantedKeyword::Defender,
        ),
        (
            "Hexproof (This creature can't be the target of spells or abilities your opponents control.)",
            GrantedKeyword::Hexproof,
        ),
        (
            "Indestructible (Effects that say \"destroy\" don't destroy this artifact.)",
            GrantedKeyword::Indestructible,
        ),
        (
            "Indestructible (Damage and effects that say \"destroy\" don't destroy this creature.)",
            GrantedKeyword::Indestructible,
        ),
        (
            "Persist (When this creature dies, if it had no -1/-1 counters on it, return it to the battlefield under its owner's control with a -1/-1 counter on it.)",
            GrantedKeyword::Persist,
        ),
    ];

    for (text, expected) in cases {
        assert_eq!(
            recognize(text).unwrap_or_else(|failure| panic!("{text}: {failure}")),
            vec![KeywordAtomNode::Parameterless(expected)],
            "{text}",
        );
    }
}

#[test]
fn reminder_belongs_to_the_final_atom_in_a_keyword_list() {
    assert_eq!(
        recognize("Flying, vigilance (Attacking doesn't cause this creature to tap.)",)
            .expect("the list parent leaves the reminder to its final keyword atom"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Vigilance),
        ],
    );
}

#[test]
fn ward_reminder_validates_its_typed_subject_and_printed_cost() {
    let keywords = recognize(
        "Flying, ward {2} (Whenever this enchantment becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)",
    )
    .expect("Ward reminder composes with a preceding keyword");

    assert_eq!(keywords.len(), 2);
    match &keywords[1] {
        KeywordAtomNode::Ward {
            cost: ResourcePaymentNode::Mana(cost),
        } => assert_eq!(cost.as_oracle_cost(), "{2}"),
        actual => panic!("expected typed Ward atom, got {actual:?}"),
    }
}

#[test]
fn ward_reminder_cannot_change_the_printed_cost() {
    let failure = recognize(
        "Ward {2} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {3}.)",
    )
    .expect_err("different reminder cost must fail");

    assert_eq!(failure.state, "keyword.ward.reminder.cost-match");
    assert_eq!(
        failure.expected,
        "a reminder cost equal to the printed Ward cost"
    );
}

#[test]
fn arbitrary_parenthetical_is_not_silently_ignored() {
    let failure = recognize("Menace (Draw a card.)")
        .expect_err("unregistered reminder wording must remain unsupported");

    assert_eq!(failure.state, "keyword.reminder.body");
    assert_eq!(failure.actual.as_deref(), Some("Draw"));
    assert_eq!(failure.expected, "This");
}

#[test]
fn persist_reminder_cannot_change_its_typed_counter_value() {
    let failure = recognize(
        "Persist (When this creature dies, if it had no -2/-2 counters on it, return it to the battlefield under its owner's control with a -1/-1 counter on it.)",
    )
    .expect_err("a different condition counter must fail");

    assert_eq!(failure.state, "keyword.persist.reminder.condition-counter");
    assert_eq!(failure.actual.as_deref(), Some("-2/-2"));
    assert_eq!(failure.expected, "the Persist -1/-1 condition counter");
}

#[test]
fn parameterless_keyword_list_reuses_the_shared_registry() {
    assert_eq!(
        recognize("Flying, trample").expect("parameterless list parses"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Trample),
        ]
    );
}

#[test]
fn mana_ward_is_a_typed_atom_inside_the_shared_list() {
    let keywords = recognize("Flying, vigilance, lifelink, ward {2}")
        .expect("parameterless keywords and typed Ward compose");

    assert_eq!(keywords.len(), 4);
    assert_eq!(
        keywords[..3],
        [
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Vigilance),
            KeywordAtomNode::Parameterless(GrantedKeyword::Lifelink),
        ]
    );
    match &keywords[3] {
        KeywordAtomNode::Ward {
            cost: ResourcePaymentNode::Mana(cost),
        } => assert_eq!(cost.as_oracle_cost(), "{2}"),
        actual => panic!("expected typed Ward atom, got {actual:?}"),
    }
}

#[test]
fn bare_ward_is_not_accepted_as_a_parameterless_keyword() {
    let failure = recognize("Flying, ward").expect_err("Ward requires a typed cost");

    assert_eq!(failure.state, "keyword.ward.cost");
    assert_eq!(failure.expected, "a mana cost for Ward");
}

#[test]
fn reminder_cannot_change_the_exalted_modifier() {
    let failure = recognize(
        "Exalted (Whenever a creature you control attacks alone, that creature gets +2/+2 until end of turn.)",
    )
    .expect_err("different modifier must fail");

    assert_eq!(failure.actual.as_deref(), Some("+2/+2"));
    assert_eq!(failure.expected, "the Exalted +1/+1 modifier");
}

#[test]
fn reminder_requires_the_attacks_alone_event() {
    let failure = recognize(
        "Exalted (Whenever a creature you control attacks together, that creature gets +1/+1 until end of turn.)",
    )
    .expect_err("different attack cardinality must fail");

    assert_eq!(failure.actual.as_deref(), Some("together"));
    assert_eq!(failure.expected, "alone");
}
