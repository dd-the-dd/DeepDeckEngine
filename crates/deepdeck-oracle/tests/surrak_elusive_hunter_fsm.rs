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
    // Trigger frames claim every triggered ability: the generic trigger
    // program owns the committed diagnostic and the specialized family keeps
    // its precise state as explicit candidate evidence.
    let diagnostics = &parsed.abilities[0].diagnostics;
    assert_eq!(
        diagnostics[0].automaton.as_deref(),
        Some("generic-trigger-program")
    );
    assert_eq!(
        diagnostics[0].state.as_deref(),
        Some("selector.predicate.start")
    );
    assert_eq!(diagnostics[0].found.as_deref(), Some("becomes"));
    assert_eq!(diagnostics[1].code, "oracle_candidate_machine_failure");
    assert_eq!(
        diagnostics[1].automaton.as_deref(),
        Some("became-target-trigger")
    );
    assert_eq!(
        diagnostics[1].state.as_deref(),
        Some("event.became-target.subject.article")
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
fn named_and_reflexive_subjects_are_not_stolen_by_the_article_selector_family() {
    for text in [
        "Whenever Ruric Thar becomes the target of a spell an opponent controls, draw a card.",
        "Whenever this creature becomes the target of a spell an opponent controls, draw a card.",
    ] {
        let parsed = parse("Claim Boundary", "Creature — Warrior", text);
        let ability = &parsed.abilities[0];
        assert_eq!(parsed.status, "unsupported", "{text}");
        assert!(ability.rule.is_none(), "{text}");
        // A source-subject trigger may be rejected by the shared source
        // program, but never claimed by the article-selector family.
        assert!(
            ability.diagnostics.iter().all(|diagnostic| {
                diagnostic.automaton.as_deref() != Some("became-target-trigger")
            }),
            "the article-selector family must not claim `{text}`"
        );
    }
}
