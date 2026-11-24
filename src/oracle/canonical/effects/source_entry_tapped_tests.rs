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
fn conditional_named_entry_composes_the_shared_condition_program() {
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

    // The controlled-presence exception keeps its earlier, dedicated family
    // (dispatch order); this family owns every other `if|unless` clause.
    assert!(claims_promoted_conditional_entry_family(&input));
    assert!(claims_promoted_source_entry_state_family(&input));

    // Strangled Cemetery's exception, read by the shared condition program.
    let draft = parse_named_source(
        "Clockwork Harbor",
        "Artifact Land",
        "Clockwork Harbor enters tapped unless a player has 13 or less life.",
    )
    .expect("a shared player-state condition");
    assert_eq!(draft.rule["condition"]["kind"], "not");
    assert_eq!(
        draft.rule["condition"]["operand"]["kind"],
        "playerQuantifier"
    );

    // The entry machine owns the `if|unless` introducer, then delegates the
    // proposition: the shared monarch designation atom needs no entry code.
    let draft = parse_named_source(
        "Clockwork Harbor",
        "Artifact Land",
        "Clockwork Harbor enters tapped unless you're the monarch.",
    )
    .expect("the shared monarch designation atom");
    assert_eq!(draft.rule["condition"]["kind"], "not");
    // An unsupported proposition still fails in the shared child: the
    // monarch designation is read in the present tense only.
    let failure = parse_named_source(
        "Clockwork Harbor",
        "Artifact Land",
        "Clockwork Harbor enters tapped unless you were the monarch.",
    )
    .expect_err("a past monarch designation is not read");
    assert!(failure.state.starts_with("condition."), "{}", failure.state);
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

#[test]
fn turn_conditioned_entry_composes_the_shared_turn_proposition_and_polarity() {
    // The turn proposition has one primitive per polarity; negation folds
    // between them, so both introducers yield the same IR.
    let not_your_turn = serde_json::json!({"kind": "notYourTurn"});
    let your_turn = serde_json::json!({"kind": "duringControllerTurn", "player": controller()});
    for (text, expected) in [
        // Eddymurk Crab (real Oracle text).
        (
            "This creature enters tapped if it's not your turn.",
            &not_your_turn,
        ),
        // `unless P` is `if not P`: same IR, different introducer.
        (
            "This creature enters tapped unless it's your turn.",
            &not_your_turn,
        ),
        ("This artifact enters tapped if it's your turn.", &your_turn),
        (
            "This land enters tapped unless it's not your turn.",
            &your_turn,
        ),
    ] {
        let draft = parse_typed_source("Artifact Creature Land", text).expect(text);
        assert_eq!(&draft.rule["condition"], expected, "{text}");
        assert_eq!(draft.rule["replacement"][0]["kind"], "setEnteringState");
        assert_eq!(draft.rule["replacement"][0]["tapped"], true);
    }
    // Unconditional entry has no condition field.
    let draft = parse_typed_source("Creature", "This creature enters tapped.").unwrap();
    assert!(draft.rule.get("condition").is_none());
}

#[test]
fn turn_conditioned_entry_near_misses_fail_explicitly() {
    for text in [
        "This creature enters prepared if it's not your turn.",
        "This creature enters tapped if it's not your upkeep.",
        "This creature enters tapped if it's not your turn and you were the monarch.",
        "This creature enters tapped if not your turn.",
    ] {
        assert!(parse_typed_source("Creature", text).is_err(), "{text}");
    }
}
