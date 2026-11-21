//! Whole-hand player verbs, the revealed hand as an observed collection, the
//! controller's choice from it, and player verbs on the chosen card.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn effects(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse(name, type_line, text);
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    parsed.abilities[0].rule.clone().expect("canonical rule")["effects"].clone()
}

fn kinds(effects: &Value) -> Vec<&str> {
    effects
        .as_array()
        .expect("effects")
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap_or_default())
        .collect()
}

fn target_player() -> Value {
    json!({"kind":"chosenTarget","id":"targetPlayer"})
}

#[test]
fn castigate_reveals_chooses_from_the_hand_and_exiles_that_card() {
    let program = effects(
        "Castigate",
        "Sorcery",
        "Target opponent reveals their hand. You choose a nonland card from it and exile that card.",
    );
    assert_eq!(
        kinds(&program),
        ["revealHand", "bind", "chooseCards", "moveCards"]
    );
    assert_eq!(program[0]["player"], target_player());
    // The hand's identities are fixed when revealed (CR 608.2c).
    assert_eq!(program[1]["value"]["zone"]["player"], target_player());
    let hand = json!({"kind":"boundObjects","binding":program[1]["id"]});
    assert_eq!(program[2]["from"], hand);
    assert_eq!(
        program[2]["player"],
        json!({"kind":"controllerOf","object":{"kind":"self"}})
    );
    assert_eq!(
        program[2]["where"],
        json!({"kind":"not","operand":{"kind":"cardTypeContains","value":"Land"}})
    );
    assert_eq!(
        program[3]["cards"],
        json!({"kind":"decisionResult","decisionId":program[2]["id"]})
    );
    assert_eq!(program[3]["to"]["kind"], "exile");
}

#[test]
fn activated_and_triggered_hand_disruption_reuse_the_same_children() {
    // Corpse Traders: an activated ability, so no spell-only recipe applies.
    let parsed = parse(
        "Corpse Traders",
        "Creature — Human Rogue",
        "{2}{B}, Sacrifice a creature: Target opponent reveals their hand. You choose a card from it. That player discards that card. Activate only as a sorcery.",
    );
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let program = parsed.abilities[0].rule.clone().unwrap()["effects"].clone();
    assert_eq!(
        kinds(&program),
        ["revealHand", "bind", "chooseCards", "discardCards"]
    );
    assert_eq!(program[3]["player"], target_player());
    assert_eq!(
        program[3]["cards"],
        json!({"kind":"decisionResult","decisionId":program[2]["id"]})
    );
    // Nimble Larcenist: a coordinated type list inside the chosen card query.
    let program = effects(
        "Nimble Larcenist",
        "Creature — Cat Rogue",
        "When this creature enters, target opponent reveals their hand. You choose an artifact, instant, or sorcery card from it and exile that card.",
    );
    assert_eq!(
        kinds(&program),
        ["revealHand", "bind", "chooseCards", "moveCards"]
    );
    assert_eq!(program[2]["where"]["kind"], "or");
}

#[test]
fn unused_player_exile_and_whole_hand_combinations() {
    // No measured card pairs a targeted player with `exiles that card`.
    let program = effects(
        "Probe Inquisitor",
        "Sorcery",
        "Target player reveals their hand. You choose a creature card from it. That player exiles that card.",
    );
    assert_eq!(
        kinds(&program),
        ["revealHand", "bind", "chooseCards", "moveCards"]
    );
    let program = effects(
        "Probe Purge",
        "Sorcery",
        "Target player discards their hand.",
    );
    assert_eq!(kinds(&program), ["discardHand"]);
    assert_eq!(program[0]["player"], target_player());
    let program = effects(
        "Probe Census",
        "Sorcery",
        "Each opponent reveals their hand.",
    );
    assert_eq!(kinds(&program), ["forEachPlayer"]);
    assert_eq!(program[0]["effects"][0]["kind"], "revealHand");
}

#[test]
fn quantified_whole_hand_verb_and_then_continuation() {
    let program = effects(
        "Wheel of Fortune",
        "Sorcery",
        "Each player discards their hand, then draws seven cards.",
    );
    assert_eq!(kinds(&program), ["forEachPlayer", "forEachPlayer"]);
    assert_eq!(program[0]["effects"][0]["kind"], "discardHand");
    assert_eq!(program[1]["effects"][0]["kind"], "drawCards");
    let program = effects(
        "Smallpox",
        "Sorcery",
        "Each player loses 1 life, discards a card, sacrifices a creature of their choice, then sacrifices a land of their choice.",
    );
    assert_eq!(kinds(&program)[0], "loseLife");
    assert_eq!(kinds(&program).last(), Some(&"sacrificePermanents"));
}

#[test]
fn near_misses_without_a_revealed_hand_or_owned_object_fail_explicitly() {
    for text in [
        // `from it` names a collection only through a revealed hand.
        "You choose a card from it. Exile that card.",
        // Another revealed object is not the whole-hand verb.
        "Target opponent reveals the top card of their library.",
        // The acting player's own hand only.
        "Target opponent reveals your hand.",
        // A counted exile by a player has no reader yet.
        "Target opponent exiles a card from their hand.",
    ] {
        assert_ne!(
            parse("Probe", "Sorcery", text).status,
            "canonical",
            "{text}"
        );
    }
}
