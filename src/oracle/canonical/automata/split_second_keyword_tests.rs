//! CR 702.61a Split second as a registered parameterless keyword.
use crate::oracle::syntax::recognize_entities;

use super::super::{GrantedKeyword, KeywordAtomNode};
use super::{OracleTokenCursor, recognize_standalone_keyword_ability};

fn recognize(text: &str) -> super::super::super::MachineResult<Vec<KeywordAtomNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("split-second-keyword-test", &tokens);
    recognize_standalone_keyword_ability(&mut cursor).map(|list| list.keywords)
}

#[test]
fn split_second_with_and_without_its_reminder() {
    for text in [
        "Split second",
        "Split second (As long as this spell is on the stack, players can't cast spells or activate abilities that aren't mana abilities.)",
    ] {
        assert_eq!(
            recognize(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}")),
            vec![KeywordAtomNode::Parameterless(GrantedKeyword::SplitSecond)]
        );
    }
    assert_eq!(GrantedKeyword::SplitSecond.canonical_name(), "splitSecond");
    // Unused combination: the shared keyword list composes it with flash.
    assert_eq!(
        recognize("Flash, split second").expect("keyword list"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Flash),
            KeywordAtomNode::Parameterless(GrantedKeyword::SplitSecond),
        ]
    );
}

#[test]
fn split_second_near_misses_fail() {
    assert!(recognize("Split").is_err());
    assert!(
        recognize(
            "Split second (As long as this spell is on the stack, players can't cast spells.)"
        )
        .is_err(),
        "a truncated reminder is not the registered reminder"
    );
}
