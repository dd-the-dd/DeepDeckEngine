use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const RESTLESS_REEF_ORACLE: &str = "This land enters tapped.\n{T}: Add {U} or {B}.\n{2}{U}{B}: Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a land.\nWhenever this land attacks, target player mills four cards.";

fn parse_card(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn entry_tapped_rule() -> serde_json::Value {
    json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
        "replacement": [{
            "kind": "setEnteringState",
            "object": { "kind": "self" },
            "tapped": true,
        }],
    })
}

fn attack_mill_rule(count: i64) -> serde_json::Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "declaredAttacker", "object": { "kind": "self" } },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetPlayer",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": { "kind": "players" },
            }],
        },
        "effects": [{
            "kind": "mill",
            "player": { "kind": "chosenTarget", "id": "targetPlayer" },
            "count": { "kind": "integer", "value": count },
        }],
    })
}

#[test]
fn restless_reef_targeted_abilities_are_native_and_match_ground_truth() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/restless-reef.json"
    ))
    .expect("Restless Reef ground truth is valid JSON");
    let parsed = parse_card("Restless Reef", "Land", RESTLESS_REEF_ORACLE);

    assert_eq!(parsed.abilities.len(), 4);
    for (actual_index, truth_index) in [(0, 0), (3, 1)] {
        let ability = &parsed.abilities[actual_index];
        assert_eq!(ability.status, "canonical");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert!(
            ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
        );
        assert_eq!(
            ability.rule,
            Some(truth["abilities"][truth_index]["expectedRule"].clone())
        );
    }
}

#[test]
fn source_type_and_literal_count_siblings_reuse_the_same_children() {
    let entry = parse_card("Tapped Relic", "Artifact", "This artifact enters tapped.");
    assert_eq!(entry.status, "canonical");
    assert_eq!(entry.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(entry.abilities[0].rule, Some(entry_tapped_rule()));

    let mill = parse_card(
        "Attacking Mill Sibling",
        "Creature",
        "Whenever this creature attacks, target player mills three cards.",
    );
    assert_eq!(mill.status, "canonical");
    assert_eq!(mill.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(mill.abilities[0].rule, Some(attack_mill_rule(3)));
}

#[test]
fn nonpermanent_source_type_is_a_committed_entry_failure() {
    let parsed = parse_card("Malformed Entry", "Instant", "This instant enters tapped.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("source-entry-tapped")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("replacement.source-entry-tapped.source")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("instant"));
}

#[test]
fn unknown_mill_count_is_a_committed_numeric_failure() {
    let parsed = parse_card(
        "Malformed Attack Mill",
        "Creature",
        "Whenever this creature attacks, target player mills many cards.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert!(ability.entities.iter().any(|entity| {
        entity.raw.eq_ignore_ascii_case("many") && entity.token_type == "unknown"
    }));
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("source-attacks-mill-trigger")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("numeric.atom.start")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("many"));
}
