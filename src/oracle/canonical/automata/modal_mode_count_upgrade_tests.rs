use crate::oracle::syntax::recognize_entities;

use super::{OracleTokenCursor, recognize_effect_sequence_modal};

/// Positive card shape (the commander `Will` cycle): the header keeps its
/// exact count and gains a conditional larger maximum.
#[test]
fn a_permitted_mode_count_upgrade_keeps_the_minimum_and_raises_the_maximum() {
    let tokens = recognize_entities(
        "Choose one. If you control a commander as you cast this spell, you may choose both instead. \u{2022} Draw a card. \u{2022} You gain 3 life.",
    );
    let mut cursor = OracleTokenCursor::new("test.modal-upgrade", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect("a permitted mode-count upgrade is part of the header");
    assert_eq!(modal.cardinality.minimum, 1);
    assert_eq!(modal.cardinality.maximum, 1);
    let upgrade = modal.upgrade.expect("the upgrade is retained");
    assert_eq!(upgrade.maximum, 2);
    assert_eq!(modal.modes.len(), 2);
}

/// Sibling: an untimed condition and a numeric upgraded count over three
/// modes (`you may choose two instead`).
#[test]
fn an_untimed_condition_and_a_numbered_upgrade_compose() {
    let tokens = recognize_entities(
        "Choose one. If you control a Wizard, you may choose two instead. \u{2022} Draw a card. \u{2022} You gain 3 life. \u{2022} Destroy target artifact.",
    );
    let mut cursor = OracleTokenCursor::new("test.modal-upgrade", &tokens);
    let modal = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect("the shared condition and a numbered count compose");
    assert_eq!(modal.upgrade.map(|upgrade| upgrade.maximum), Some(2));
    assert_eq!(modal.cardinality.maximum, 1);
}

/// Near-misses: a mandatory upgrade would also raise the minimum, and an
/// upgrade that does not exceed the header count is meaningless; both fail
/// in their own states instead of being approximated.
#[test]
fn mandatory_or_non_increasing_upgrades_fail_explicitly() {
    let tokens = recognize_entities(
        "Choose one. If this spell was kicked, choose both instead. \u{2022} Draw a card. \u{2022} You gain 3 life.",
    );
    let mut cursor = OracleTokenCursor::new("test.modal-upgrade", &tokens);
    let failure = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect_err("a mandatory upgrade is not lowered");
    assert_eq!(failure.state, "modal.header.upgrade.permission");

    let tokens = recognize_entities(
        "Choose two. If you control a Wizard, you may choose two instead. \u{2022} Draw a card. \u{2022} You gain 3 life. \u{2022} Destroy target artifact.",
    );
    let mut cursor = OracleTokenCursor::new("test.modal-upgrade", &tokens);
    let failure = recognize_effect_sequence_modal(&mut cursor, &[], None)
        .expect_err("an upgrade must exceed the header count");
    assert_eq!(failure.state, "modal.effect-sequence.complete");
}
