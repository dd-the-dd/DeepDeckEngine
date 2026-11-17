use crate::oracle::syntax::recognize_entities;

use super::super::{GrantedKeyword, KeywordAtomNode};
use super::{OracleTokenCursor, recognize_standalone_keyword_ability};

fn recognize(text: &str) -> super::super::super::MachineResult<Vec<KeywordAtomNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("changeling-keyword-test", &tokens);
    recognize_standalone_keyword_ability(&mut cursor).map(|list| list.keywords)
}

#[test]
fn changeling_with_its_registered_reminder_is_a_parameterless_keyword() {
    assert_eq!(
        recognize("Changeling (This card is every creature type.)")
            .expect("Changeling reminder parses"),
        vec![KeywordAtomNode::Parameterless(GrantedKeyword::Changeling)]
    );
    assert_eq!(GrantedKeyword::Changeling.canonical_name(), "changeling");
}

#[test]
fn changeling_composes_in_a_keyword_list_with_the_reminder_on_the_final_atom() {
    assert_eq!(
        recognize("Flying, changeling, vigilance").expect("bare list parses"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Changeling),
            KeywordAtomNode::Parameterless(GrantedKeyword::Vigilance),
        ]
    );
    assert_eq!(
        recognize("Deathtouch, changeling (This card is every creature type.)")
            .expect("reminder belongs to the final atom"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Deathtouch),
            KeywordAtomNode::Parameterless(GrantedKeyword::Changeling),
        ]
    );
}

#[test]
fn changeling_reminder_cannot_change_its_wording() {
    let failure = recognize("Changeling (This card is every land type.)")
        .expect_err("altered reminder must fail explicitly");
    assert_eq!(failure.state, "keyword.reminder.body");
    assert_eq!(failure.actual.as_deref(), Some("land"));
}
