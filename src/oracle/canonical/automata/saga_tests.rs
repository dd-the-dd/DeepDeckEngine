use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{
    SagaChapterBodyNode, SagaGrantedAbilityNode, recognize_saga_chapter,
    recognize_saga_progression_reminder,
};

#[test]
fn reminder_retains_typed_final_chapter() {
    let tokens = recognize_entities(
        "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after IV.)",
    );
    let mut cursor = OracleTokenCursor::new("saga-reminder-test", &tokens);
    let reminder = recognize_saga_progression_reminder(&mut cursor).expect("reminder parses");
    assert_eq!(reminder.final_chapter, 4);
}

#[test]
fn noncontiguous_chapters_delegate_green_mana_to_the_native_child() {
    let tokens = recognize_entities("I, III — This Saga gains \"{T}: Add {G}.\"");
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    let chapter = recognize_saga_chapter(&mut cursor, &[], None).expect("chapter parses");
    assert_eq!(chapter.chapters, vec![1, 3]);
    assert!(matches!(
        chapter.body,
        SagaChapterBodyNode::Grant(SagaGrantedAbilityNode::Mana(_))
    ));
}

#[test]
fn malformed_chapter_list_fails_before_body_dispatch() {
    let tokens = recognize_entities("II, — This Saga gains \"{T}: Add {G}.\"");
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    let failure = recognize_saga_chapter(&mut cursor, &[], None).expect_err("must fail");
    assert_eq!(failure.state, "saga.chapter.roman");
}

#[test]
fn any_other_chapter_body_is_the_shared_effect_program() {
    let tokens = recognize_entities("II \u{2014} Draw a card, then discard a card.");
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    let chapter = recognize_saga_chapter(&mut cursor, &[], None).expect("chapter parses");
    assert_eq!(chapter.chapters, vec![2]);
    assert!(matches!(chapter.body, SagaChapterBodyNode::Program(_)));
    // `this Saga` without a quoted ability is an ordinary reference, not the
    // native grant form.
    let tokens = recognize_entities("III \u{2014} Exile this Saga.");
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    if let Ok(chapter) = recognize_saga_chapter(&mut cursor, &[], None) {
        assert!(matches!(chapter.body, SagaChapterBodyNode::Program(_)));
    }
}
