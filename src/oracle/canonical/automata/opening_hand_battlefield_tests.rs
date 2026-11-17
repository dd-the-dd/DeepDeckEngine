use crate::oracle::syntax::{NameLinkKind, ResolvedNameLink, recognize_entities};

use super::super::LocalObjectReference;
use super::{OracleTokenCursor, recognize_opening_hand_battlefield};

const EXACT: &str =
    "If this card is in your opening hand, you may begin the game with it on the battlefield.";

fn current_name_link(
    tokens: &[crate::oracle::model::RecognizedEntity],
    first_word: &str,
    canonical_name: &str,
) -> ResolvedNameLink {
    let token_start = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case(first_word))
        .expect("linked name begins in the token stream");
    let token_end = tokens[token_start..]
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("is"))
        .map(|offset| token_start + offset)
        .expect("linked name ends before the source-state verb");
    ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: canonical_name.to_string(),
        raw: tokens[token_start..token_end]
            .iter()
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        token_start,
        token_end,
        span_start: tokens[token_start].span_start,
        span_end: tokens[token_end - 1].span_end,
    }
}

#[test]
fn this_card_and_its_pronoun_share_one_source_binding() {
    let tokens = recognize_entities(EXACT);
    let mut cursor = OracleTokenCursor::new("opening-hand-battlefield-test", &tokens);

    let node = recognize_opening_hand_battlefield(&mut cursor, &[], None)
        .expect("the generic this-card form parses");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert_eq!(node.entering_object, node.source);
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn linked_card_name_and_gendered_pronoun_reuse_the_reference_machine() {
    let text = "If Quicksilver, Brash Blur is in your opening hand, you may begin the game with him on the battlefield.";
    let tokens = recognize_entities(text);
    let links = [current_name_link(
        &tokens,
        "Quicksilver",
        "Quicksilver, Brash Blur",
    )];
    let mut cursor = OracleTokenCursor::new("opening-hand-battlefield-test", &tokens);

    let node = recognize_opening_hand_battlefield(&mut cursor, &links, None)
        .expect("a linked current-card name and pronoun parse");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert_eq!(node.entering_object, node.source);
}

#[test]
fn unsupported_destination_fails_at_the_local_zone_state() {
    let tokens = recognize_entities(
        "If this card is in your opening hand, you may begin the game with it in your graveyard.",
    );
    let mut cursor = OracleTokenCursor::new("opening-hand-battlefield-test", &tokens);

    let failure = recognize_opening_hand_battlefield(&mut cursor, &[], None)
        .expect_err("a different relation and destination must not be approximated");

    assert_eq!(failure.state, "pregame.opening-hand.destination-relation");
    assert_eq!(failure.actual.as_deref(), Some("in"));
}
