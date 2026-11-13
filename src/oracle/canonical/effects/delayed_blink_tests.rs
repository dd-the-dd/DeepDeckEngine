use crate::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT: &str = "Whenever Phelia attacks, exile up to one other target nonland permanent. At the beginning of the next end step, return that card to the battlefield under its owner's control. If it entered under your control, put a +1/+1 counter on Phelia.";

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn exact_attack_trigger_lowers_to_independent_primitives() {
    let parsed = parse(
        "Phelia, Exuberant Shepherd",
        "Legendary Creature — Dog",
        EXACT,
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native rule");
    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(rule["event"]["kind"], "declaredAttacker");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"], 1);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["excludeSource"],
        true
    );
    assert_eq!(rule["effects"][0]["kind"], "exilePermanent");
    assert_eq!(rule["effects"][0]["bind"], "delayedExiledPermanent");
    assert_eq!(rule["effects"][1]["condition"]["kind"], "bindingNotEmpty");
    assert_eq!(rule["effects"][1]["then"][0]["kind"], "bind");
    let delayed = &rule["effects"][1]["then"][1];
    assert_eq!(delayed["kind"], "installDelayedStepTrigger");
    assert_eq!(delayed["step"], "endStep");
    assert_eq!(
        delayed["captureBindings"],
        json!(["delayedExiledPermanent", "delayedExiledOwner"])
    );
    assert_eq!(delayed["effects"][0]["kind"], "moveCards");
    assert_eq!(delayed["effects"][0]["to"]["player"]["kind"], "boundValue");
    assert_eq!(delayed["effects"][1]["condition"]["kind"], "playersEqual");
    assert_eq!(delayed["effects"][1]["then"][0]["kind"], "putCounters");
    assert_eq!(delayed["effects"][1]["then"][0]["counter"], "+1/+1");
}

#[test]
fn sibling_changes_source_selector_and_counter_without_new_routing() {
    let text = "Whenever this artifact attacks, exile up to one other target creature. At the beginning of the next end step, return that card to the battlefield under its owner's control. If it entered under your control, put two charge counters on this artifact.";
    let parsed = parse("Generalized Vessel", "Artifact Creature — Construct", text);
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][1]["then"][1]["effects"][1]["then"][0]["counter"],
        "charge"
    );
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][1]["then"][1]["effects"][1]["then"][0]["count"]["value"],
        2
    );
}

#[test]
fn malformed_delayed_step_remains_committed_with_structured_failure() {
    let text = EXACT.replace("next end step", "next combat");
    let parsed = parse(
        "Phelia, Exuberant Shepherd",
        "Legendary Creature — Dog",
        &text,
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.status, "unsupported", "{parsed:#?}");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let diagnostic = ability.diagnostics.first().expect("structured failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("attack-delayed-return-counter")
    );
    assert_eq!(diagnostic.state.as_deref(), Some("duration.next-step.step"));
    assert_eq!(diagnostic.found.as_deref(), Some("combat"));
}
