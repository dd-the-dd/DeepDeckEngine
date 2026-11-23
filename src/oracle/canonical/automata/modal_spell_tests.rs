use crate::oracle::syntax::{NameLinkKind, ResolvedNameLink, recognize_entities};

use super::super::{EffectProgramNode, LocalEffectNode};
use super::{
    ModeCountNode, OracleTokenCursor, modal_body_starts, recognize_effect_sequence_modal,
    recognize_modal_header,
};

fn first_sequence_effects(program: &EffectProgramNode) -> &[LocalEffectNode] {
    program.inner_sequence().effects.as_slice()
}

#[test]
fn the_header_frame_reads_every_printed_mode_count() {
    for (text, count, repeated) in [
        (
            "Choose one \u{2014} \u{2022}",
            ModeCountNode::Exactly(1),
            false,
        ),
        (
            "Choose one or both \u{2014} \u{2022}",
            ModeCountNode::OneOrBoth,
            false,
        ),
        (
            "Choose one or more \u{2014} \u{2022}",
            ModeCountNode::AtLeast(1),
            false,
        ),
        (
            "Choose up to two \u{2014} \u{2022}",
            ModeCountNode::UpTo(2),
            false,
        ),
        (
            "Choose any number \u{2014} \u{2022}",
            ModeCountNode::AnyNumber,
            false,
        ),
        (
            "Choose three. You may choose the same mode more than once. \u{2022}",
            ModeCountNode::Exactly(3),
            true,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.modal-header", &tokens);
        let header = recognize_modal_header(&mut cursor).expect(text);
        assert_eq!(header.count, count, "{text}");
        assert_eq!(header.allow_repeated, repeated, "{text}");
        assert_eq!(cursor.peek(), Some("\u{2022}"), "{text}");
    }
}

#[test]
fn an_article_is_never_a_mode_count() {
    // parse_number_word maps `a` to one; the header refuses it.
    let tokens = recognize_entities("Choose a creature \u{2014} \u{2022} Draw a card.");
    let mut cursor = OracleTokenCursor::new("test.modal-header", &tokens);
    let failure = recognize_modal_header(&mut cursor).expect_err("an article");
    assert_eq!(failure.state, "modal.header.mode-count");
    // Without a bullet the body is not modal at all.
    let tokens = recognize_entities("Choose a creature you control.");
    let cursor = OracleTokenCursor::new("test.modal-probe", &tokens);
    assert!(!modal_body_starts(&cursor));
}

#[test]
fn effect_sequence_parent_delegates_any_literal_number_of_collection_modes() {
    let tokens = recognize_entities(
        "Choose three — • Destroy all artifacts. • Destroy all enchantments. • Destroy all creatures.",
    );
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect("three ordinary effect-program modes parse");

    assert_eq!(modal.cardinality.minimum, 3);
    assert_eq!(modal.cardinality.maximum, 3);
    assert!(!modal.cardinality.allow_repeated);
    assert_eq!(modal.modes.len(), 3);
    assert!(modal.modes.iter().all(|mode| {
        matches!(
            first_sequence_effects(mode),
            [LocalEffectNode::DestroyCollection(_)]
        )
    }));
}

#[test]
fn effect_sequence_parent_reports_trailing_mode_tokens_from_the_child() {
    let tokens =
        recognize_entities("Choose one — • Destroy all artifacts. Sing. • Destroy all creatures.");
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal", &tokens);
    let failure = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect_err("a bullet must be one fully consumed effect program");

    assert_eq!(failure.automaton, "modal-effect-sequence-mode");
    assert_eq!(failure.actual.as_deref(), Some("Sing"));
}

#[test]
fn mode_subcursor_rebases_current_card_name_links() {
    let tokens = recognize_entities(
        "Choose one — • Example Blast deals 3 damage to target creature. • Destroy target artifact.",
    );
    let token_start = tokens
        .iter()
        .position(|token| token.raw == "Example")
        .expect("name begins");
    let token_end = token_start + 2;
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Example Blast".to_string(),
        raw: "Example Blast".to_string(),
        token_start,
        token_end,
        span_start: tokens[token_start].span_start,
        span_end: tokens[token_end - 1].span_end,
    }];
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal-name", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &links, None)
        .expect("the bullet-local source link must retain its meaning");

    assert!(matches!(
        first_sequence_effects(&modal.modes[0]),
        [LocalEffectNode::DealDamage(_)]
    ));
    assert!(matches!(
        first_sequence_effects(&modal.modes[1]),
        [LocalEffectNode::ZoneChange(_)]
    ));
}

#[test]
fn formerly_primitive_modes_are_ordinary_programs() {
    // The retired primitive modal family: damage, a temporary grant and
    // exile modes are all read by the shared effect program.
    let tokens = recognize_entities(
        "Choose one — • This spell deals 3 damage to any target. • Target creature gains flying until end of turn. • Exile target creature with mana value 2 or less.",
    );
    let mut cursor = OracleTokenCursor::new("test.effect-sequence-modal", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect("formerly primitive modes parse");
    assert_eq!(modal.modes.len(), 3);
    assert!(matches!(
        first_sequence_effects(&modal.modes[0]),
        [LocalEffectNode::DealDamage(_)]
    ));
    assert!(matches!(
        first_sequence_effects(&modal.modes[2]),
        [LocalEffectNode::ZoneChange(_)]
    ));
}
