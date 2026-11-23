use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::super::{LocalEffectNode, TriggerBodyNode};
use super::{recognize_saga_chapter, recognize_saga_progression_reminder};

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
fn a_quoted_grant_is_an_ordinary_program_body() {
    // The retired closed `Grant` body: `This Saga gains "..."` is the shared
    // keyword-grant clause with a quoted ability and no duration.
    let tokens = recognize_entities("I, III \u{2014} This Saga gains \"{T}: Add {G}.\"");
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    let chapter = recognize_saga_chapter(&mut cursor, &[], None).expect("chapter parses");
    assert_eq!(chapter.chapters, vec![1, 3]);
    let TriggerBodyNode::Program(program) = &chapter.body else {
        panic!("a quoted grant is a program body");
    };
    assert!(matches!(
        program.inner_sequence().effects.as_slice(),
        [LocalEffectNode::TemporaryKeywordGrant(grant)] if grant.abilities.len() == 1
    ));
}

#[test]
fn malformed_chapter_list_fails_before_body_dispatch() {
    let tokens = recognize_entities("II, \u{2014} This Saga gains \"{T}: Add {G}.\"");
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
    assert!(matches!(chapter.body, TriggerBodyNode::Program(_)));
    let tokens = recognize_entities("III \u{2014} Exile this Saga.");
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    let chapter = recognize_saga_chapter(&mut cursor, &[], None).expect("chapter parses");
    assert!(matches!(chapter.body, TriggerBodyNode::Program(_)));
    cursor.finish().expect("the whole chapter is read");
}

#[test]
fn a_chapter_body_with_modal_markers_is_read_as_modes() {
    let tokens = recognize_entities(
        "I \u{2014} Choose one \u{2014} \u{2022} You gain 3 life. \u{2022} Draw a card.",
    );
    let mut cursor = OracleTokenCursor::new("saga-chapter-test", &tokens);
    let chapter = recognize_saga_chapter(&mut cursor, &[], None).expect("chapter parses");
    assert!(matches!(chapter.body, TriggerBodyNode::Modal(ref modal) if modal.modes.len() == 2));
}
