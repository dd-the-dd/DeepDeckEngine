use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Flying, vigilance, deathtouch, lifelink\nWhen Atraxa enters, reveal the top ten cards of your library. For each card type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order. (Artifact, battle, creature, enchantment, instant, land, planeswalker, and sorcery are card types.)";

fn parse(oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Atraxa, Grand Unifier".to_string(),
        type_line: "Legendary Creature — Phyrexian Angel".to_string(),
        mana_cost: Some("{3}{G}{W}{U}{B}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn exact_card_is_fully_native_and_uses_only_collection_choice_and_zone_primitives() {
    let parsed = parse(EXACT_ORACLE);
    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));

    let rule = parsed.abilities[1].rule.as_ref().expect("entry rule");
    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(
        rule["event"],
        json!({ "kind": "enterBattlefield", "object": { "kind": "self" } })
    );
    let effects = rule["effects"].as_array().expect("primitive effects");
    assert_eq!(effects.len(), 21);
    assert_eq!(effects[0]["kind"], "lookAtTopCards");
    assert_eq!(effects[0]["count"]["value"], 10);
    assert_eq!(effects[1]["kind"], "revealCards");

    let choices = effects
        .iter()
        .filter(|effect| effect["kind"] == "chooseCards")
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 9);
    assert!(choices.iter().any(|effect| {
        effect["id"] == "cardTypeKindred"
            && effect["where"] == json!({ "kind": "cardTypeContains", "value": "Kindred" })
    }));
    assert!(choices.iter().all(|effect| {
        effect["minimum"] == json!({ "kind": "integer", "value": 0 })
            && effect["maximum"] == json!({ "kind": "integer", "value": 1 })
    }));
    assert_eq!(effects.last().unwrap()["to"]["position"], "bottom");
    assert_eq!(effects.last().unwrap()["order"]["kind"], "random");

    let ir = rule.to_string();
    assert!(!ir.contains("revealTopAndChooseOnePerCardType"));
    assert!(!ir.contains("resolveTriggeredInstruction"));
}

#[test]
fn other_quantity_and_reordered_documentary_types_share_the_same_children() {
    let parsed = parse(
        "When this creature enters, reveal the top three cards of your library. For each card type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order. (Kindred, sorcery, planeswalker, land, instant, enchantment, creature, battle, and artifact are card types.)",
    );
    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap();
    assert_eq!(effects[0]["count"]["value"], 3);
    assert_eq!(
        effects
            .iter()
            .filter(|effect| effect["kind"] == "chooseCards")
            .count(),
        9
    );
}

#[test]
fn each_choice_uses_the_previous_remaining_set() {
    let parsed = parse(
        "When this creature enters, reveal the top two cards of your library. For each card type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order.",
    );
    let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap();
    let choices = effects
        .iter()
        .filter(|effect| effect["kind"] == "chooseCards")
        .collect::<Vec<_>>();
    assert_eq!(choices[0]["from"]["kind"], "boundObjects");
    assert_eq!(choices[1]["from"]["kind"], "setDifference");
    assert_eq!(
        choices[1]["from"]["right"],
        json!({ "kind": "decisionResult", "decisionId": "cardTypeArtifact" })
    );
    assert_eq!(effects.last().unwrap()["cards"]["kind"], "setDifference");
}

#[test]
fn claimed_type_and_zone_near_misses_report_precise_native_states() {
    for (oracle, state) in [
        (
            "When this creature enters, reveal the top three cards of your library. For each permanent type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order.",
            "selection.per-card-type.category",
        ),
        (
            "When this creature enters, reveal the top three cards of your library. For each card type, you may put a card of that type from among the revealed cards into your graveyard. Put the rest on the bottom of your library in a random order.",
            "selection.per-card-type.destination",
        ),
    ] {
        let parsed = parse(oracle);
        assert_eq!(parsed.status, "unsupported", "{oracle}: {parsed:#?}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        // The shared trigger program reports its own committed failure first;
        // the partition family's precise near-miss state stays visible.
        let diagnostics = &parsed.abilities[0].diagnostics;
        assert_eq!(
            diagnostics[0].automaton.as_deref(),
            Some("generic-trigger-program")
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.automaton.as_deref() == Some("card-type-partition")
                    && diagnostic.state.as_deref() == Some(state)
            }),
            "{oracle}: {diagnostics:#?}"
        );
    }
}
