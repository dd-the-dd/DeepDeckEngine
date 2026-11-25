use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities, resolve_counter_name_spans};

fn parse(type_line: &str, text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let mut tokens = recognize_oracle_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let input = AbilityInput {
        face_name: "Test Card",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    assert!(claims_promoted_source_entry_counters_family(&input));
    parse_promoted_source_entry_counters(&input)
}

fn replacement(draft: &CanonicalRuleDraft) -> &Value {
    assert_eq!(draft.rule["event"]["kind"], "wouldEnterBattlefield");
    &draft.rule["replacement"][0]
}

#[test]
fn article_quantity_places_one_plus_counter() {
    let draft = parse(
        "Creature - Rat",
        "This creature enters with a +1/+1 counter on it.",
    )
    .expect("entry counter");
    let op = replacement(&draft);
    assert_eq!(op["kind"], "putEnteringCounters");
    assert_eq!(op["counter"], "+1/+1");
    assert_eq!(op["count"], json!({"kind":"integer","value":1}));
}

#[test]
fn literal_negative_counters_on_a_creature() {
    let draft = parse(
        "Creature - Elemental",
        "This creature enters with two -1/-1 counters on it.",
    )
    .expect("entry counter");
    assert_eq!(replacement(&draft)["counter"], "-1/-1");
    assert_eq!(
        replacement(&draft)["count"],
        json!({"kind":"integer","value":2})
    );
}

#[test]
fn named_counter_on_a_non_creature_source() {
    let draft = parse(
        "Artifact",
        "This artifact enters with three film counters on it.",
    )
    .expect("entry counter");
    assert_eq!(replacement(&draft)["counter"], "film");
    assert_eq!(
        replacement(&draft)["count"],
        json!({"kind":"integer","value":3})
    );
}

#[test]
fn variable_quantity_reads_the_declared_x() {
    let draft = parse(
        "Creature - Hydra",
        "This creature enters with X +1/+1 counters on it.",
    )
    .expect("entry counter");
    assert_eq!(
        replacement(&draft)["count"],
        json!({"kind": "decisionResult", "decisionId": "xValue"})
    );
}

#[test]
fn for_each_scales_the_article_quantity_with_a_shared_count() {
    let draft = parse(
        "Creature - Human",
        "This creature enters with a +1/+1 counter on it for each other creature you control.",
    )
    .expect("entry counter");
    let count = &replacement(&draft)["count"];
    assert_eq!(count["kind"], "countPermanents");
    assert_eq!(count["excludeSource"], true);
}

#[test]
fn for_each_multiplies_a_literal_quantity_and_names_the_counter() {
    let draft = parse(
        "Artifact",
        "This artifact enters with two charge counters on it for each land you control.",
    )
    .expect("entry counter");
    let count = &replacement(&draft)["count"];
    assert_eq!(count["kind"], "multiply");
    assert_eq!(count["left"], json!({"kind":"integer","value":2}));
    assert_eq!(replacement(&draft)["counter"], "charge");
}

#[test]
fn near_miss_quantity_equal_to_fails_at_the_counter_name() {
    let failure = parse(
        "Creature - Construct",
        "This creature enters with a number of +1/+1 counters on it equal to its mana value.",
    )
    .expect_err("an equality quantity is not a supported entry quantity");
    assert!(
        failure.to_string().contains("counter-name.first-part"),
        "{failure}"
    );
}

/// Rigging Runner: the trailing condition is kept on the replacement and
/// checked as the permanent enters (CR 614.1c), never dropped.
#[test]
fn trailing_condition_gates_the_entry_replacement() {
    let draft = parse(
        "Creature - Human",
        "This creature enters with a +1/+1 counter on it if you attacked this turn.",
    )
    .expect("conditional entry counter");
    assert_eq!(replacement(&draft)["kind"], "putEnteringCounters");
    assert_eq!(draft.rule["condition"]["kind"], "compare");
    assert_eq!(
        draft.rule["condition"]["left"]["event"], "attackerDeclared",
        "{}",
        draft.rule
    );
}

#[test]
fn near_miss_unsupported_trailing_condition_is_not_dropped() {
    // The cast-from-hand record is a shared condition atom (CR 601.2a).
    let draft = parse(
        "Creature - Human",
        "This creature enters with a +1/+1 counter on it if you cast it from your hand.",
    )
    .expect("a cast-record entry condition");
    assert_eq!(draft.rule["condition"]["kind"], "wasCastFromHand");
    // Near miss: a cast zone the record does not name stays explicit.
    let failure = parse(
        "Creature - Human",
        "This creature enters with a +1/+1 counter on it if you cast it from your graveyard.",
    )
    .expect_err("an unread condition has no entry-counter representation");
    assert!(failure.to_string().contains("condition"), "{failure}");
}

#[test]
fn near_miss_noncreature_literal_zero_is_rejected() {
    assert!(
        parse(
            "Artifact",
            "This artifact enters with 0 charge counters on it."
        )
        .is_err()
    );
}
