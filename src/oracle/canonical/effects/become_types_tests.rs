use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};
use serde_json::{Value, json};

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

fn subtype_change(rule: &Value, index: usize) -> &Value {
    let effect = &rule["effects"][index];
    assert_eq!(effect["kind"], "modifySubtypeSet", "{effect}");
    effect
}

/// A real card shape: a subject ellipsis after `and`, a preposed duration
/// shared by both verbs, and the replacement meaning of a bare subtype.
#[test]
fn counter_untap_stats_and_subtype_replacement_share_one_preposed_duration() {
    let rule = parse(
        "Put a +1/+1 counter on target creature you control. Untap it. Until end of turn, it gets +1/+1 and becomes a Doctor. (It loses all other creature types.)",
    )
    .expect("stats and replacement subtype compose");
    let effects = rule["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 4, "{effects:?}");
    assert_eq!(effects[0]["kind"], "putCounters");
    assert_eq!(effects[2]["kind"], "modifyPowerToughness");
    let change = subtype_change(&rule, 3);
    assert_eq!(change["mode"], "replace");
    assert_eq!(change["subtypes"], json!(["Doctor"]));
    assert_eq!(
        change["duration"],
        json!({ "kind": "untilEndOfCurrentTurn" })
    );
    assert_eq!(change["object"], rule["effects"][2]["object"]);
}

/// A previously unused combination: `in addition to its other types` adds
/// instead of replacing, several subtypes, a postposed duration, and a keyword
/// list attached to the shared subject through verb coordination.
#[test]
fn additive_subtypes_and_keyword_grant_share_the_subject() {
    let rule = parse(
        "Until end of turn, target creature you control becomes an Avatar in addition to its other types and gains flying, first strike, and lifelink.",
    )
    .expect("additive subtype with keyword grant");
    let change = subtype_change(&rule, 0);
    assert_eq!(change["mode"], "add");
    assert_eq!(change["subtypes"], json!(["Avatar"]));
    assert_eq!(
        change["duration"],
        json!({ "kind": "untilEndOfCurrentTurn" })
    );
    assert_eq!(rule["effects"][1]["kind"], "grantKeyword");
    assert_eq!(rule["effects"][1]["object"], change["object"]);

    let rule = parse("Until end of turn, target creature becomes a Zombie Wizard.")
        .expect("replacement with two subtypes");
    let change = subtype_change(&rule, 0);
    assert_eq!(change["mode"], "replace");
    assert_eq!(change["subtypes"], json!(["Zombie", "Wizard"]));
}

#[test]
fn preposed_and_postposed_durations_lower_identically() {
    assert_eq!(
        parse("Until end of turn, target creature becomes a Doctor.").unwrap(),
        parse("Target creature becomes a Doctor until end of turn.").unwrap(),
    );
}

#[test]
fn card_type_change_requires_the_additive_clause() {
    let rule = parse(
        "Until end of turn, target creature becomes an artifact in addition to its other types and gains indestructible.",
    )
    .expect("additive card type");
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "addCardType");
    assert_eq!(effect["cardType"], "Artifact");
    assert_eq!(effect["retainExistingTypes"], true);
    assert_eq!(rule["effects"][1]["kind"], "grantKeyword");

    let failure = parse("Until end of turn, target creature becomes an artifact.")
        .expect_err("replacing card types has no runtime primitive");
    assert!(
        failure.contains("in addition to its other types"),
        "{failure}"
    );
}

/// Near misses stay explicit failures instead of being approximated.
#[test]
fn unsupported_type_change_shapes_fail_explicitly() {
    for text in [
        // The base power/toughness tail is a different characteristic.
        "Until end of turn, target creature becomes a Doctor with base power and toughness 1/1.",
        // A color word is not a type list.
        "Until end of turn, target creature becomes blue.",
        // A noncreature subtype cannot use the creature-subtype primitive.
        "Until end of turn, target land becomes a Forest.",
        // Two durations for one continuous effect.
        "Until end of turn, target creature becomes a Doctor until your next turn.",
        // A duration that the runtime cannot end.
        "Until your next turn, target creature becomes a Doctor.",
        // A trailing duration of a coordinated chain is not propagated back.
        "Target creature becomes a Doctor and gains flying until end of turn.",
        // Elision needs a preceding permanent subject.
        "Draw a card and becomes a Doctor.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}
