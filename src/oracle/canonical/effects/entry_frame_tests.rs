use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities, resolve_counter_name_spans};

fn attempt(type_line: &str, text: &str) -> ReplacementFrameAttempt {
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
    try_parse_replacement_frame(&input)
}

fn rule(type_line: &str, text: &str) -> Value {
    match attempt(type_line, text) {
        ReplacementFrameAttempt::Parsed(draft) => draft.rule,
        ReplacementFrameAttempt::Committed(failure) => panic!("{text}: {failure}"),
        ReplacementFrameAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

fn failure_state(type_line: &str, text: &str) -> &'static str {
    match attempt(type_line, text) {
        ReplacementFrameAttempt::Committed(failure) => failure.state,
        ReplacementFrameAttempt::Parsed(_) => panic!("{text}: parsed"),
        ReplacementFrameAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

#[test]
fn vivid_land_composes_tapped_state_and_counters_in_one_program() {
    let rule = rule(
        "Land",
        "This land enters tapped with two charge counters on it.",
    );
    assert_eq!(rule["kind"], "replacementEffect");
    assert_eq!(rule["event"]["kind"], "wouldEnterBattlefield");
    assert_eq!(rule["replacement"][0]["kind"], "setEnteringState");
    assert_eq!(rule["replacement"][1]["kind"], "putEnteringCounters");
    assert_eq!(rule["replacement"][1]["counter"], "charge");
    assert!(rule.get("decisions").is_none());
}

#[test]
fn thriving_land_stores_a_color_other_than_the_excluded_one() {
    let rule = rule(
        "Land",
        "This land enters tapped. As it enters, choose a color other than white.",
    );
    assert_eq!(rule["decisions"][0]["kind"], "chooseColor");
    assert_eq!(rule["decisions"][0]["id"], "chosenColor");
    assert_eq!(rule["decisions"][0]["options"], json!(["U", "B", "R", "G"]));
    assert_eq!(rule["replacement"][0]["kind"], "setEnteringState");
    assert_eq!(
        rule["replacement"][1],
        json!({ "kind": "storeDecision", "decisionId": "chosenColor" })
    );
}

#[test]
fn each_choice_domain_lowers_to_its_stored_decision() {
    for (text, kind, id) in [
        (
            "As this artifact enters, choose a color.",
            "chooseColor",
            "chosenColor",
        ),
        (
            "As this land enters, choose a creature type.",
            "chooseCreatureType",
            "chosenCreatureType",
        ),
        (
            "As this enters, choose a basic land type.",
            "chooseBasicLandType",
            "chosenBasicLandType",
        ),
        (
            "As this artifact enters, choose a card type.",
            "chooseCardType",
            "chosenCardType",
        ),
    ] {
        let rule = rule("Artifact Land", text);
        assert_eq!(rule["decisions"][0]["kind"], kind, "{text}");
        assert_eq!(rule["decisions"][0]["id"], id, "{text}");
        assert_eq!(rule["replacement"][0]["decisionId"], id, "{text}");
    }
}

#[test]
fn clone_and_vesuva_choose_a_battlefield_permanent_to_copy() {
    let clone = rule(
        "Creature - Shapeshifter",
        "You may have this creature enter as a copy of any creature on the battlefield.",
    );
    assert_eq!(clone["decisions"][0]["kind"], "chooseBattlefieldPermanent");
    assert_eq!(clone["decisions"][0]["optional"], true);
    assert_eq!(clone["replacement"][0]["kind"], "copyEnteringPermanent");
    assert_eq!(clone["replacement"][0]["tapped"], false);

    let vesuva = rule(
        "Land",
        "You may have this land enter tapped as a copy of any land on the battlefield.",
    );
    assert_eq!(vesuva["replacement"][0]["tapped"], true);
    assert_eq!(vesuva["replacement"].as_array().map(Vec::len), Some(1));

    let mirror = rule(
        "Creature - Shapeshifter",
        "You may have this creature enter as a copy of a creature you control, except it has haste.",
    );
    assert_eq!(
        mirror["decisions"][0]["controlledByEnteringController"],
        true
    );
    assert_eq!(mirror["replacement"][0]["grantKeywords"], json!(["haste"]));
}

#[test]
fn a_mandatory_copy_stays_mandatory() {
    let rule = rule(
        "Creature - Shapeshifter",
        "This creature enters as a copy of a creature you control.",
    );
    assert_eq!(rule["decisions"][0]["optional"], false);
}

#[test]
fn reveal_or_tapped_land_counts_the_revealed_card() {
    let rule = rule(
        "Land - Plains Island",
        "As this land enters, you may reveal a Plains or Island card from your hand. If you don't, this land enters tapped.",
    );
    assert_eq!(rule["decisions"][0]["kind"], "chooseRevealCard");
    let conditional = &rule["replacement"][0];
    assert_eq!(conditional["kind"], "conditional");
    assert_eq!(
        conditional["condition"]["left"]["kind"],
        "countDecisionCardsMatching"
    );
    assert_eq!(conditional["then"][0]["kind"], "setEnteringState");
}

#[test]
fn an_optional_entry_cost_outcome_lowers_through_the_shared_entering_state() {
    // A fallback with a counter and an `If you do` outcome compose through
    // the same entering-state operations as the plain entry frame.
    let declined = rule(
        "Creature - Dragon",
        "As this creature enters, you may reveal a Dragon card from your hand. If you don't, this creature enters tapped with a -1/-1 counter on it.",
    );
    let conditional = &declined["replacement"][0];
    assert_eq!(conditional["condition"]["operator"], "==");
    assert_eq!(conditional["then"][0]["kind"], "setEnteringState");
    assert_eq!(conditional["then"][1]["kind"], "putEnteringCounters");
    assert_eq!(conditional["then"][1]["counter"], "-1/-1");
    let performed = rule(
        "Creature - Dragon",
        "As this creature enters, you may reveal a Dragon card from your hand. If you do, this creature enters with two +1/+1 counters on it.",
    );
    let conditional = &performed["replacement"][0];
    assert_eq!(conditional["condition"]["operator"], ">");
    assert_eq!(conditional["then"][0]["kind"], "putEnteringCounters");
}

#[test]
fn an_optional_entry_cost_without_an_entry_runtime_fails_explicitly() {
    assert_eq!(
        failure_state(
            "Creature - Dragon",
            "As this creature enters, you may discard a Dragon card. If you don't, this creature enters tapped.",
        ),
        "engine.gap.resolution-cost-action"
    );
}

#[test]
fn a_copy_exception_without_a_runtime_primitive_fails_explicitly() {
    assert_eq!(
        failure_state(
            "Creature - Shapeshifter",
            "You may have this creature enter as a copy of any creature on the battlefield, except it's an artifact in addition to its other types.",
        ),
        "replacement.entry-frame.copy-exception.runtime"
    );
}

#[test]
fn an_entry_frame_needs_a_permanent_face() {
    assert_eq!(
        failure_state("Instant", "As this enters, choose a color."),
        "replacement.entry-frame.validation"
    );
}

#[test]
fn a_resolution_choice_is_not_an_entry_choice() {
    assert!(matches!(
        attempt(
            "Instant",
            "Choose a color. Creatures of the chosen color get +1/+1 until end of turn."
        ),
        ReplacementFrameAttempt::NotClaimed
    ));
}
