use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, NameLinkKind, ResolvedNameLink, recognize_entities};

fn parse_named_source(
    name: &str,
    type_line: &str,
    text: &str,
) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let token_end = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("enters"))
        .expect("the entry verb follows the current-card name");
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: name.to_string(),
        raw: tokens[..token_end]
            .iter()
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        token_start: 0,
        token_end,
        span_start: tokens[0].span_start,
        span_end: tokens[token_end - 1].span_end,
    }];
    let input = AbilityInput {
        face_name: name,
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &links,
        name_resolution_failure: None,
    };
    parse_promoted_source_entry_state(&input)
}

fn parse_typed_source(type_line: &str, text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let input = AbilityInput {
        face_name: "Mismatched Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    parse_promoted_source_entry_state(&input)
}

#[test]
fn named_artifact_source_lowers_to_the_existing_entry_state_primitive() {
    let draft = parse_named_source(
        "Clockwork Archive",
        "Artifact",
        "Clockwork Archive enters tapped.",
    )
    .expect("a linked named permanent source lowers");

    assert_eq!(draft.rule["kind"], "replacementEffect");
    assert_eq!(draft.rule["event"]["kind"], "wouldEnterBattlefield");
    assert_eq!(draft.rule["event"]["object"]["kind"], "self");
    assert_eq!(draft.rule["replacement"][0]["kind"], "setEnteringState");
    assert_eq!(draft.rule["replacement"][0]["tapped"], true);
}

#[test]
fn named_nonpermanent_face_is_rejected_by_the_parent_context() {
    let failure = parse_named_source(
        "Delayed Insight",
        "Instant",
        "Delayed Insight enters tapped.",
    )
    .expect_err("an instant cannot become a battlefield-entry replacement source");

    assert_eq!(failure.state, "replacement.source-entry-state.validation");
    assert!(failure.expected.contains("permanent card face"));
}

#[test]
fn typed_permanent_noun_cannot_override_a_nonpermanent_face() {
    let failure = parse_typed_source("Sorcery", "This land enters tapped.")
        .expect_err("typed Oracle syntax cannot turn a sorcery face into a permanent");

    assert_eq!(failure.state, "replacement.source-entry-state.validation");
    assert!(failure.expected.contains("permanent card face"));
}

#[test]
fn conditional_named_entry_is_not_claimed_by_the_unconditional_family() {
    let name = "Clockwork Harbor";
    let type_line = "Artifact Land";
    let text = "Clockwork Harbor enters tapped unless you control an Island.";
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let token_end = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("enters"))
        .expect("the entry verb follows the source name");
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: name.to_string(),
        raw: name.to_string(),
        token_start: 0,
        token_end,
        span_start: tokens[0].span_start,
        span_end: tokens[token_end - 1].span_end,
    }];
    let input = AbilityInput {
        face_name: name,
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &links,
        name_resolution_failure: None,
    };

    assert!(!claims_promoted_source_entry_state_family(&input));

    let failure = parse_named_source(
        "Clockwork Harbor",
        "Artifact Land",
        "Clockwork Harbor enters tapped unless you control an Island.",
    )
    .expect_err("the unconditional machine must reject trailing conditions");

    assert_eq!(failure.state, "replacement.source-entry-state.terminator");
    assert_eq!(failure.actual.as_deref(), Some("unless"));
}

#[test]
fn tapped_sentence_followed_by_another_replacement_is_not_claimed() {
    let text = "This land enters tapped. As it enters, choose a color.";
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    let input = AbilityInput {
        face_name: "Thriving Isle",
        face_type_line: "Land",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };

    assert!(!claims_promoted_source_entry_state_family(&input));
}
