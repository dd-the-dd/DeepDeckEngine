use crate::oracle::syntax::{NameLinkKind, ResolvedNameLink, recognize_entities};

use super::{
    LocalObjectReference, ObjectCardType, OracleTokenCursor, SourceEntryState,
    recognize_source_entry_state,
};

fn current_card_link(
    tokens: &[crate::oracle::model::RecognizedEntity],
    canonical_name: &str,
) -> ResolvedNameLink {
    let token_end = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("enters"))
        .expect("the entry verb follows the linked current-card name");
    ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: canonical_name.to_string(),
        raw: tokens[..token_end]
            .iter()
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        token_start: 0,
        token_end,
        span_start: tokens[0].span_start,
        span_end: tokens[token_end - 1].span_end,
    }
}

#[test]
fn typed_source_entry_tapped_retains_the_explicit_land_type() {
    let tokens = recognize_entities("This land enters tapped.");
    let mut cursor = OracleTokenCursor::new("test.source-entry-tapped", &tokens);

    let node = recognize_source_entry_state(&mut cursor, &[], None)
        .expect("source-entry replacement parses");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert_eq!(node.source_type, Some(ObjectCardType::Land));
    assert_eq!(node.state, SourceEntryState::Tapped);
}

#[test]
fn prepared_entry_reuses_the_state_machine_for_another_permanent_type() {
    let tokens = recognize_entities("This creature enters prepared.");
    let mut cursor = OracleTokenCursor::new("test.source-entry-prepared", &tokens);

    let node = recognize_source_entry_state(&mut cursor, &[], None)
        .expect("prepared source-entry replacement parses");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert_eq!(node.source_type, Some(ObjectCardType::Creature));
    assert_eq!(node.state, SourceEntryState::Prepared);
}

#[test]
fn prepared_entry_consumes_the_exact_canonical_prepare_reminder() {
    let tokens = recognize_entities(
        "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)",
    );
    let mut cursor = OracleTokenCursor::new("test.source-entry-prepare-reminder", &tokens);

    let node = recognize_source_entry_state(&mut cursor, &[], None)
        .expect("canonical Prepare reminder parses completely");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert_eq!(node.source_type, Some(ObjectCardType::Creature));
    assert_eq!(node.state, SourceEntryState::Prepared);
}

#[test]
fn prepare_reminder_reuses_the_subautomaton_for_another_permanent_type() {
    let tokens = recognize_entities(
        "This artifact enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)",
    );
    let mut cursor = OracleTokenCursor::new("test.artifact-entry-prepare-reminder", &tokens);

    let node = recognize_source_entry_state(&mut cursor, &[], None)
        .expect("canonical Prepare reminder is independent of permanent type");

    assert_eq!(node.source_type, Some(ObjectCardType::Artifact));
    assert_eq!(node.state, SourceEntryState::Prepared);
}

#[test]
fn prepared_entry_still_accepts_the_absence_of_reminder_text() {
    let tokens = recognize_entities("This enchantment enters prepared.");
    let mut cursor = OracleTokenCursor::new("test.source-entry-prepared-without-reminder", &tokens);

    let node = recognize_source_entry_state(&mut cursor, &[], None)
        .expect("Prepare reminder is optional for prepared permanents");

    assert_eq!(node.source_type, Some(ObjectCardType::Enchantment));
    assert_eq!(node.state, SourceEntryState::Prepared);
}

#[test]
fn altered_prepare_reminder_fails_inside_the_reminder_subautomaton() {
    let tokens = recognize_entities(
        "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so prepares it.)",
    );
    let mut cursor = OracleTokenCursor::new("test.source-entry-altered-prepare-reminder", &tokens);

    let failure = recognize_source_entry_state(&mut cursor, &[], None)
        .expect_err("altered Prepare reminder must not be silently accepted");

    assert_eq!(
        failure.state,
        "replacement.source-entry-state.prepare-reminder.unprepare"
    );
    assert_eq!(failure.expected, "unprepares");
    assert_eq!(failure.actual.as_deref(), Some("prepares"));
}

#[test]
fn prepare_reminder_is_not_accepted_for_other_entering_states() {
    let tokens = recognize_entities(
        "This land enters tapped. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)",
    );
    let mut cursor = OracleTokenCursor::new("test.tapped-entry-prepare-reminder", &tokens);

    let failure = recognize_source_entry_state(&mut cursor, &[], None)
        .expect_err("Prepare reminder is valid only for the prepared state");

    assert_eq!(failure.state, "replacement.source-entry-state.complete");
    assert_eq!(failure.expected, "end of input");
    assert_eq!(failure.actual.as_deref(), Some("("));
}

#[test]
fn claimed_state_entry_reports_extra_syntax_at_the_local_terminator() {
    let tokens = recognize_entities("This artifact enters prepared twice.");
    let mut cursor = OracleTokenCursor::new("test.source-entry-near-miss", &tokens);

    let failure = recognize_source_entry_state(&mut cursor, &[], None)
        .expect_err("extra state syntax is not silently accepted");

    assert_eq!(failure.state, "replacement.source-entry-state.terminator");
    assert_eq!(failure.actual.as_deref(), Some("twice"));
}

#[test]
fn nonpermanent_source_type_is_rejected_locally() {
    let tokens = recognize_entities("This instant enters tapped.");
    let mut cursor = OracleTokenCursor::new("test.invalid-source-entry-tapped", &tokens);

    let failure = recognize_source_entry_state(&mut cursor, &[], None)
        .expect_err("instant cannot enter the battlefield tapped");

    assert_eq!(failure.actual.as_deref(), Some("instant"));
    assert!(failure.expected.contains("permanent card type"));
}

#[test]
fn linked_current_card_name_reuses_the_source_reference_machine() {
    let text = "Clockwork Archive enters tapped.";
    let tokens = recognize_entities(text);
    let links = [current_card_link(&tokens, "Clockwork Archive")];
    let mut cursor = OracleTokenCursor::new("test.named-source-entry-tapped", &tokens);

    let node = recognize_source_entry_state(&mut cursor, &links, None)
        .expect("a linked current-card name parses as the entering source");

    assert_eq!(node.source, LocalObjectReference::SourceSelf);
    assert_eq!(node.source_type, None);
    assert_eq!(node.state, SourceEntryState::Tapped);
}

#[test]
fn unlinked_printed_name_is_not_guessed_from_unknown_tokens() {
    let tokens = recognize_entities("Clockwork Archive enters tapped.");
    let mut cursor = OracleTokenCursor::new("test.unlinked-source-entry-tapped", &tokens);

    let failure = recognize_source_entry_state(&mut cursor, &[], None)
        .expect_err("a printed name must be resolved by the shared name linker");

    assert_eq!(failure.state, "reference.source.start");
    assert_eq!(failure.token_index, 0);
    assert!(failure.expected.contains("current-card name"));
}
