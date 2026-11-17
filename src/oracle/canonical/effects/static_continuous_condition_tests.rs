use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

fn parse(text: &str, face_type_line: &str) -> StaticContinuousParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_static_continuous_effect(&AbilityInput {
        face_name: "Generic Static",
        face_type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed_rule(text: &str) -> Value {
    match parse(text, "Creature - Test") {
        StaticContinuousParseAttempt::Parsed(draft) => draft.rule,
        StaticContinuousParseAttempt::Rejected(failure) => panic!("{text}: rejected: {failure}"),
        StaticContinuousParseAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

fn rejected_state(text: &str) -> &'static str {
    match parse(text, "Creature - Test") {
        StaticContinuousParseAttempt::Rejected(failure) => failure.state,
        StaticContinuousParseAttempt::Parsed(_) => panic!("{text}: unexpectedly parsed"),
        StaticContinuousParseAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

fn count_at_least(left: Value, minimum: i64) -> Value {
    json!({ "kind": "compare", "operator": ">=", "left": left, "right": integer(minimum) })
}

#[test]
fn trailing_condition_gates_a_source_keyword_grant() {
    let rule =
        parsed_rule("This creature has vigilance as long as you control a Jace planeswalker.");
    assert_eq!(rule["modifiers"][0]["kind"], "grantKeyword");
    assert_eq!(rule["modifiers"][0]["keyword"], "vigilance");
    assert_eq!(rule["condition"]["kind"], "compare");
    assert_eq!(rule["condition"]["left"]["kind"], "countPermanents");
    assert_eq!(rule["condition"]["left"]["excludeSource"], false);
    assert_eq!(rule["condition"]["right"], integer(1));
    // The condition belongs to the static rule, not to one modifier.
    assert!(rule["modifiers"][0].get("condition").is_none());
}

#[test]
fn leading_condition_gates_every_predicate_of_the_static_ability() {
    let rule = parsed_rule(
        "As long as you control an artifact, this creature gets +1/+0 and has deathtouch.",
    );
    assert_eq!(rule["modifiers"].as_array().map(Vec::len), Some(2));
    assert_eq!(rule["modifiers"][0]["kind"], "modifyPowerToughness");
    assert_eq!(rule["modifiers"][1]["keyword"], "deathtouch");
    assert_eq!(rule["condition"]["left"]["where"]["value"], "Artifact");
}

#[test]
fn state_and_history_atoms_compose_with_the_same_static_frame() {
    // Source state, with the keyword reminder after the sentence end.
    let rule = parsed_rule(
        "This creature has hexproof as long as it's untapped. (It can't be the target of spells or abilities your opponents control.)",
    );
    assert_eq!(
        rule["condition"],
        json!({ "kind": "not", "operand": { "kind": "isTapped", "object": { "kind": "self" } } })
    );
    let rule = parsed_rule("This creature has flying as long as it's modified.");
    assert_eq!(rule["condition"], json!({ "kind": "sourceIsModified" }));

    // Controller history this turn.
    for (text, event) in [
        (
            "This creature gets +2/+0 as long as you've drawn two or more cards this turn.",
            "cardDrawn",
        ),
        (
            "This creature gets +2/+0 as long as you've cast two or more spells this turn.",
            "spellCast",
        ),
    ] {
        let condition = parsed_rule(text)["condition"].clone();
        assert_eq!(condition["left"]["kind"], "countEventsThisTurn", "{text}");
        assert_eq!(condition["left"]["event"], event, "{text}");
        assert_eq!(condition["right"], integer(2), "{text}");
    }
    let rule = parsed_rule("This creature gets +2/+0 as long as you gained life this turn.");
    assert_eq!(
        rule["condition"],
        count_at_least(
            json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
            1
        )
    );
}

#[test]
fn graveyard_counts_use_the_shared_card_selector_and_ability_words_are_labels() {
    let rule = parsed_rule(
        "Threshold \u{2014} This creature gets +1/+0 and has flying as long as there are seven or more cards in your graveyard.",
    );
    assert_eq!(rule["condition"]["left"]["kind"], "countCards");
    assert_eq!(rule["condition"]["left"]["where"], Value::Null);
    assert_eq!(rule["condition"]["right"], integer(7));
    let rule = parsed_rule(
        "As long as there are two or more creature cards in your graveyard, this creature gets +2/+1.",
    );
    assert_eq!(rule["condition"]["left"]["where"]["value"], "Creature");
    let rule =
        parsed_rule("This creature gets +1/+1 as long as there's a Lesson card in your graveyard.");
    assert_eq!(rule["condition"]["left"]["where"]["value"], "Lesson");
    assert_eq!(rule["condition"]["right"], integer(1));
}

#[test]
fn another_excludes_the_source_and_conditions_gate_collection_subjects() {
    let rule = parsed_rule("This creature has haste as long as you control another Goblin.");
    assert_eq!(rule["condition"]["left"]["excludeSource"], true);
    let rule =
        parsed_rule("As long as you have an enduring story, creatures you control get +1/+1.");
    assert_eq!(rule["condition"]["kind"], "hasEnduringStory");
    assert_eq!(rule["modifiers"][0]["objects"]["kind"], "permanents");
    // A previously unused combination: Boolean composition over two atoms.
    let rule = parsed_rule(
        "This creature gets +1/+1 as long as it's untapped or you control an artifact.",
    );
    assert_eq!(rule["condition"]["kind"], "or");
}

#[test]
fn unsupported_conditions_fail_at_a_precise_state() {
    assert_eq!(
        rejected_state("This creature has flying as long as the moon is full."),
        "condition.static.atom-kind"
    );
    // `it` is not the source when the subject is a collection.
    assert_eq!(
        rejected_state("Creatures you control get +1/+1 as long as it's untapped."),
        "condition.static.source-state.subject"
    );
    assert_eq!(
        rejected_state("This creature gets +1/+1 as long as it's your turn."),
        "condition.static.source-state.property"
    );
    // One condition per static ability; a trailing predicate is not guessed.
    assert_eq!(
        rejected_state(
            "As long as you control an artifact, this creature gets +1/+0 as long as you control a land."
        ),
        "static.continuous.connector"
    );
    assert_eq!(
        rejected_state(
            "This creature gets +1/+0 as long as you control an artifact and has flying."
        ),
        "condition.static.atom-kind"
    );
}
