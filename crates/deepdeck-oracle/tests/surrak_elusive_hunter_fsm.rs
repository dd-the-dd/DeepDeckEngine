use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "This spell can't be countered.\nTrample\nWhenever a creature you control or a creature spell you control becomes the target of a spell or ability an opponent controls, draw a card.";

fn parse(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn ability<'a>(
    parsed: &'a OracleCardParseResult,
    needle: &str,
) -> &'a deepdeck_oracle::oracle::ParsedOracleAbility {
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.contains(needle))
        .expect("requested ability exists")
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn expected_targeting_rule(card_type: &str, count: i64) -> serde_json::Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "becameTarget",
            "object": {
                "kind": "union",
                "sets": [
                    {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": { "kind": "cardTypeContains", "value": card_type },
                    },
                    {
                        "kind": "spells",
                        "controller": controller(),
                        "where": { "kind": "cardTypeContains", "value": card_type },
                    },
                ],
            },
            "targetingStackObject": {
                "kind": "stackItems",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": {
                    "kind": "or",
                    "operands": [
                        { "kind": "isSpell" },
                        { "kind": "isActivatedAbility" },
                        { "kind": "isTriggeredAbility" },
                    ],
                },
            },
        },
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": { "kind": "integer", "value": count },
        }],
    })
}

#[test]
fn exact_card_is_all_native_and_primitive() {
    let parsed = parse(
        "Surrak, Elusive Hunter",
        "Legendary Creature — Human Warrior",
        EXACT_ORACLE,
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));
    assert_eq!(
        ability(&parsed, "can't be countered").rule,
        Some(json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "stack" },
            },
            "modifiers": [{
                "kind": "cantBeCountered",
                "object": { "kind": "self" },
            }],
        }))
    );
    assert_eq!(
        ability(&parsed, "becomes the target").rule,
        Some(expected_targeting_rule("Creature", 1))
    );
}

#[test]
fn artifact_sibling_reuses_both_object_domains_and_numeric_effect() {
    let parsed = parse(
        "Targeting Observatory",
        "Artifact Creature — Construct",
        "Whenever an artifact you control or an artifact spell you control becomes the target of a spell or ability an opponent controls, draw two cards.",
    );

    assert_eq!(parsed.status, "canonical");
    let targeting = ability(&parsed, "becomes the target");
    assert_eq!(targeting.recognizer, RecognizerProvenance::Native);
    assert_eq!(targeting.rule, Some(expected_targeting_rule("Artifact", 2)));
}

#[test]
fn permanent_only_and_spell_only_subjects_remain_distinct() {
    let permanent = parse(
        "Permanent Watcher",
        "Creature — Wizard",
        "Whenever a creature you control becomes the target of a spell an opponent controls, draw a card.",
    );
    let spell = parse(
        "Spell Watcher",
        "Creature — Wizard",
        "Whenever a creature spell you control becomes the target of an ability an opponent controls, draw a card.",
    );

    for parsed in [&permanent, &spell] {
        assert_eq!(parsed.status, "canonical");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    }
    assert_eq!(
        permanent.abilities[0].rule.as_ref().unwrap()["event"]["object"]["kind"],
        "permanents"
    );
    assert_eq!(
        spell.abilities[0].rule.as_ref().unwrap()["event"]["object"]["kind"],
        "spells"
    );
}

#[test]
fn malformed_family_is_native_failure_without_legacy_fallback() {
    let parsed = parse(
        "Broken Watcher",
        "Creature — Wizard",
        "Whenever a creature you control or becomes the target of a spell an opponent controls, draw a card.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(parsed.abilities[0].rule.is_none());
    // Trigger frames claim every triggered ability: the one event clause of
    // the generic trigger program owns the committed diagnostic. The
    // targeting frame is one of its alternatives, not a separate family.
    let diagnostics = &parsed.abilities[0].diagnostics;
    assert_eq!(
        diagnostics[0].automaton.as_deref(),
        Some("generic-trigger-program")
    );
    assert!(matches!(
        diagnostics[0].state.as_deref(),
        Some("event.became-target.subject.article" | "selector.predicate.start")
    ));
    assert_eq!(diagnostics[0].found.as_deref(), Some("becomes"));
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.automaton.as_deref() != Some("became-target-trigger"))
    );
}

#[test]
fn malformed_counter_prohibition_is_native_failure() {
    let parsed = parse(
        "Overqualified Counter Test",
        "Instant",
        "This spell can't be countered twice.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].diagnostics[0].state.as_deref(),
        Some("counter-prohibition.terminator")
    );
}

#[test]
fn an_unresolved_name_is_not_stolen_by_the_article_selector() {
    let text =
        "Whenever Ruric Thar becomes the target of a spell an opponent controls, draw a card.";
    let parsed = parse("Claim Boundary", "Creature — Warrior", text);
    let ability = &parsed.abilities[0];
    assert_eq!(parsed.status, "unsupported", "{text}");
    assert!(ability.rule.is_none(), "{text}");
}

/// The source subject composes the shared source reference with the
/// targeting frame of `become` and the same stack-object selector; a
/// stack object of any controller is the unqualified form (CR 115.1).
#[test]
fn the_source_subject_composes_with_the_targeting_frame() {
    for (text, controller) in [
        (
            "Whenever this creature becomes the target of a spell an opponent controls, draw a card.",
            Some(json!({ "kind": "opponentsOf", "player": controller() })),
        ),
        (
            "When this creature becomes the target of a spell or ability, sacrifice it.",
            None,
        ),
    ] {
        let parsed = parse("Claim Boundary", "Creature — Illusion", text);
        assert_eq!(parsed.status, "canonical", "{text}");
        let event = &parsed.abilities[0].rule.as_ref().expect("rule")["event"];
        assert_eq!(event["kind"], "becameTarget", "{text}");
        assert_eq!(event["object"], json!({ "kind": "self" }), "{text}");
        assert_eq!(event["targetingStackObject"]["kind"], "stackItems");
        assert_eq!(
            event["targetingStackObject"].get("controller").cloned(),
            controller,
            "{text}"
        );
    }
}

/// Valiant: the shared turn-ordinal modifier on the source targeting frame.
#[test]
fn a_targeting_turn_ordinal_is_typed_and_a_turn_owner_is_an_event_time_condition() {
    let parsed = parse(
        "Valiant Probe",
        "Creature — Soldier",
        "Whenever this creature becomes the target of a spell or ability you control for the first time each turn, draw a card.",
    );
    assert_eq!(parsed.status, "canonical");
    let event = &parsed.abilities[0].rule.as_ref().expect("rule")["event"];
    assert_eq!(event["turnOrdinal"], 1);
    assert_eq!(event["targetingStackObject"]["controller"], controller());

    let parsed = parse(
        "Turn Owner Probe",
        "Creature — Soldier",
        "Whenever this creature becomes the target of a spell during your turn, draw a card.",
    );
    // The targeting matcher has no turn field, so the turn owner is the
    // ability's event-time condition, checked when the event happens.
    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0].rule.as_ref().expect("rule");
    assert_eq!(rule["event"]["kind"], "becameTarget");
    assert!(rule["event"].get("duringControllerTurn").is_none());
    assert_eq!(rule["eventCondition"]["kind"], "duringControllerTurn");
}
