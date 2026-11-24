use super::*;
use crate::oracle::syntax::recognize_entities;

fn read(text: &str) -> ChildOutcome<EventReplacementNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("event-replacement-test", &tokens);
    try_recognize_event_replacement(&mut cursor)
}

#[test]
fn a_replaced_draw_reads_its_exception_and_program() {
    for (text, except_first_in_draw_step, count) in [
        (
            "If you would draw a card, draw two cards instead.",
            false,
            2,
        ),
        (
            "If you would draw a card except the first one you draw in each of your draw steps, draw two cards instead.",
            true,
            2,
        ),
        (
            "If you would draw a card, draw three cards instead.",
            false,
            3,
        ),
    ] {
        let ChildOutcome::Matched(node) = read(text) else {
            panic!("{text}");
        };
        assert_eq!(
            node.event,
            ReplacedEventNode::Draw {
                player: LocalPlayerReference::Controller,
                except_first_in_draw_step,
            },
            "{text}"
        );
        assert_eq!(
            node.program,
            ReplacementProgramNode::Draw { count },
            "{text}"
        );
    }
}

#[test]
fn an_unsupported_draw_program_or_condition_fails_after_the_event() {
    for (text, state) in [
        (
            "If you would draw a card while your library has no cards in it, you win the game instead.",
            "replacement.event.separator",
        ),
        (
            "If you would draw a card, you may put a study counter on this enchantment instead.",
            "replacement.program.draw",
        ),
        (
            "If you would draw a card, draw a card instead.",
            "replacement.program.draw.count",
        ),
    ] {
        match read(text) {
            ChildOutcome::Committed(failure) => assert_eq!(failure.state, state, "{text}"),
            _ => panic!("expected a committed failure: {text}"),
        }
    }
}

#[test]
fn other_replaced_events_and_subjects_are_left_to_their_readers() {
    for text in [
        "If a creature an opponent controls would die, exile it instead.",
        "If an opponent would draw two or more cards, instead you and that player each draw a card.",
        "If you would gain life, you gain that much life plus 1 instead.",
    ] {
        assert!(matches!(read(text), ChildOutcome::NoMatch(_)), "{text}");
    }
}
