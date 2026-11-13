use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const MURKTIDE_ORACLE: &str = "Delve (Each card you exile from your graveyard while casting this spell pays for {1}.)\nFlying\nThis creature enters with a +1/+1 counter on it for each instant and sorcery card exiled with it.\nWhenever an instant or sorcery card leaves your graveyard, put a +1/+1 counter on this creature.";

fn parse(name: &str, type_line: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn self_ref() -> serde_json::Value {
    json!({ "kind": "self" })
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

fn instant_or_sorcery() -> serde_json::Value {
    json!({
        "kind": "or",
        "operands": [
            { "kind": "cardTypeContains", "value": "Instant" },
            { "kind": "cardTypeContains", "value": "Sorcery" },
        ],
    })
}

#[test]
fn murktide_regent_is_fully_native_and_primitive() {
    let parsed = parse(
        "Murktide Regent",
        "Creature - Dragon",
        "{5}{U}{U}",
        MURKTIDE_ORACLE,
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 4);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert!(parsed.abilities.iter().all(|ability| {
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed
            .abilities
            .iter()
            .map(|ability| ability.rule.clone().expect("native rule"))
            .collect::<Vec<_>>(),
        vec![
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "delve" },
            }),
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "flying" },
            }),
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": "+1/+1",
                    "count": {
                        "kind": "countDecisionCardsMatching",
                        "decisionId": "delveCards",
                        "where": instant_or_sorcery(),
                    },
                }],
            }),
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "cardsLeftGraveyard",
                    "player": controller(),
                    "where": instant_or_sorcery(),
                    "aggregation": "perCard",
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": { "kind": "integer", "value": 1 },
                }],
            }),
        ]
    );
}

#[test]
fn delve_without_reminder_is_a_generic_native_sibling() {
    let parsed = parse("Test Delver", "Creature", "{4}{U}", "Delve");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": { "kind": "delve" },
        }))
    );
}

#[test]
fn malformed_delve_reminder_is_a_committed_native_failure() {
    let parsed = parse(
        "Test Delver",
        "Creature",
        "{4}{U}",
        "Delve (Each card you exile from your graveyard while casting this spell pays for {2}.)",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("keyword-delve")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("keyword.delve.reminder.payment")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("{2}"));
}

#[test]
fn delve_rejects_trailing_unconsumed_tokens_inside_the_native_family() {
    let parsed = parse("Test Delver", "Creature", "{4}{U}", "Delve flying");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("keyword-delve")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("keyword.delve.reminder-or-complete")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("flying"));
}

#[test]
fn entering_counter_decision_rejects_an_unbound_exile_anaphor() {
    let parsed = parse(
        "Test Delver",
        "Creature",
        "{4}{U}",
        "This creature enters with a +1/+1 counter on it for each instant or sorcery card exiled with them.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("decision-entry-counters")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("replacement.decision-entry-counters.exile-reference")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("them"));
}

#[test]
fn graveyard_leave_event_preserves_per_card_vs_one_or_more_aggregation() {
    let singular = parse(
        "Test Watcher",
        "Creature",
        "{2}{U}",
        "Whenever an instant or sorcery card leaves your graveyard, put a +1/+1 counter on this creature.",
    );
    let aggregate = parse(
        "Test Watcher",
        "Creature",
        "{2}{U}",
        "Whenever one or more instant or sorcery cards leave your graveyard, put a +1/+1 counter on this creature.",
    );

    for parsed in [&singular, &aggregate] {
        assert_eq!(parsed.status, "canonical");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(
            parsed.abilities[0].rule.as_ref().expect("native rule")["event"]["where"],
            instant_or_sorcery()
        );
    }
    assert_eq!(
        singular.abilities[0].rule.as_ref().unwrap()["event"]["aggregation"],
        "perCard"
    );
    assert_eq!(
        aggregate.abilities[0].rule.as_ref().unwrap()["event"]["aggregation"],
        "oneOrMore"
    );
}

#[test]
fn graveyard_leave_event_rejects_a_non_controller_zone() {
    let parsed = parse(
        "Test Watcher",
        "Creature",
        "{2}{U}",
        "Whenever an instant or sorcery card leaves a graveyard, put a +1/+1 counter on this creature.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("graveyard-leave-counter")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("trigger.graveyard-leave.zone")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("a"));
}
