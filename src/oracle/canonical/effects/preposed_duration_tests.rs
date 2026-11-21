use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};
use serde_json::Value;

use crate::oracle::canonical::dispatch::{CanonicalParseOutcome, parse_canonical_rule};

fn parse(text: &str) -> Result<Value, String> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Generic Modifier",
        face_type_line: "Instant",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule(&input, "spellAbility") {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft.rule),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => {
            Err(format!("{}: {}", failure.state, failure.expected))
        }
        CanonicalParseOutcome::Unsupported => Err("unsupported".to_string()),
    }
}

/// The preposed form must lower to exactly the IR of the postposed form.
#[test]
fn preposed_duration_lowers_to_the_postposed_ir() {
    for (preposed, postposed) in [
        (
            "Until end of turn, target creature gets +2/+2.",
            "Target creature gets +2/+2 until end of turn.",
        ),
        (
            "Until end of turn, target creature gets +1/+1 and gains flying.",
            "Target creature gets +1/+1 and gains flying until end of turn.",
        ),
        (
            "Until end of turn, target creature gains deathtouch.",
            "Target creature gains deathtouch until end of turn.",
        ),
        (
            "Draw a card. Until end of turn, target creature gets +2/+2.",
            "Draw a card. Target creature gets +2/+2 until end of turn.",
        ),
        (
            "Until end of turn, target creature gets +2/+2. Untap it.",
            "Target creature gets +2/+2 until end of turn. Untap it.",
        ),
    ] {
        let expected = parse(postposed).expect(postposed);
        let actual = parse(preposed).expect(preposed);
        assert_eq!(actual, expected, "{preposed}");
    }
}

/// New combinations of already supported concepts: a plural continuous
/// subject, another duration kind, and a sentence-bounded scope.
#[test]
fn preposed_duration_scope_ends_at_the_sentence_boundary() {
    let rule = parse("Until your next turn, target creature gets -2/-0. Draw a card.")
        .expect("preposed next-turn duration");
    assert_eq!(rule["effects"][0]["duration"]["kind"], "untilNextTurn");
    assert_eq!(rule["effects"][1]["kind"], "drawCards");

    let rule = parse(
        "Until end of turn, creatures you control gain trample. Until your next turn, target creature gets -1/-1.",
    )
    .expect("two independent preposed sentences");
    assert_eq!(
        rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
    assert_eq!(rule["effects"][1]["duration"]["kind"], "untilNextTurn");
}

#[test]
fn preposed_duration_requires_duration_bearing_continuous_effects() {
    for text in [
        "Until end of turn, draw a card.",
        "Until end of turn, target creature gets +2/+2 and you draw a card.",
        "Until end of turn, target creature gets +2/+2 until end of turn.",
        "Until end of turn, target creature gains flying until your next turn.",
    ] {
        assert!(parse(text).is_err(), "{text} must fail explicitly");
    }
}

/// The duration scopes over each action joined by `and` (CR 611.2a); the same
/// shape without a preposed duration never lets one trailing duration cover
/// an earlier action.
#[test]
fn preposed_duration_scopes_over_every_action_joined_by_and() {
    for text in [
        "Until end of turn, target creature gets +1/+1 and target creature you control gains lifelink.",
        "Until end of turn, target creature gets +1/+1 and target creature you control gets -1/-1.",
    ] {
        let rule = parse(text).expect(text);
        let effects = rule["effects"].as_array().expect("effects");
        assert_eq!(effects.len(), 2, "{text}");
        for effect in effects {
            assert_eq!(
                effect["duration"]["kind"], "untilEndOfCurrentTurn",
                "{text}"
            );
        }
    }
    assert!(
        parse("Target creature gets +1/+1 and target creature you control gets -1/-1 until end of turn.")
            .is_err()
    );
}
