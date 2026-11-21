use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const REPLICATE_REMINDER: &str = "(When you cast this spell, copy it for each time you paid its replicate cost. You may choose new targets for the copies.)";

fn parse_instant(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Consign to Memory".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn replicate_and_heterogeneous_stack_counter_are_native_and_primitive() {
    let parsed = parse_instant(&format!(
        "Replicate {{1}} {REPLICATE_REMINDER}\nCounter target triggered ability or colorless spell."
    ));

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );

    let replicate = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("Replicate emits a rule");
    assert_eq!(replicate["ability"]["kind"], "replicate");
    assert_eq!(replicate["ability"]["cost"]["manaCost"], "{1}");
    assert_eq!(replicate["ability"]["repeatable"], true);

    let counter = parsed.abilities[1]
        .rule
        .as_ref()
        .expect("counter emits a rule");
    assert_eq!(
        counter["declaration"]["decisions"][0]["candidates"],
        json!({
            "kind": "stackItems",
            "where": {
                "kind": "or",
                "operands": [
                    { "kind": "isTriggeredAbility" },
                    {
                        "kind": "and",
                        "operands": [
                            { "kind": "isColorless" },
                            { "kind": "isSpell" },
                        ],
                    },
                ],
            },
        })
    );
    assert_eq!(counter["effects"][0]["kind"], "counterStackObject");
    assert_eq!(
        counter["effects"][0]["object"],
        json!({ "kind": "chosenTarget", "id": "targetStackObject" })
    );
}

#[test]
fn sibling_cost_and_stack_ability_kind_reuse_the_same_automata() {
    for (ability_kind, expected_filter) in [
        ("triggered", "isTriggeredAbility"),
        ("activated", "isActivatedAbility"),
    ] {
        let parsed = parse_instant(&format!(
            "Replicate {{2}}{{U}} (When you cast this spell, copy it for each time you paid its replicate cost.)\nCounter target {ability_kind} ability or colorless spell."
        ));
        assert_eq!(parsed.status, "canonical", "{ability_kind}");
        assert!(
            parsed
                .abilities
                .iter()
                .all(|ability| ability.recognizer == RecognizerProvenance::Native),
            "{ability_kind}"
        );
        assert_eq!(
            parsed.abilities[0].rule.as_ref().unwrap()["ability"]["cost"]["manaCost"],
            "{2}{U}"
        );
        assert_eq!(
            parsed.abilities[1].rule.as_ref().unwrap()["declaration"]["decisions"][0]["candidates"]
                ["where"]["operands"][0]["kind"],
            expected_filter
        );
    }
}

#[test]
fn promoted_families_report_precise_near_miss_states_without_fallback() {
    for (oracle_text, automaton, state, found) in [
        (
            "Replicate {1} (When you cast this spell, copy it for each time you paid their replicate cost.)",
            "keyword-replicate",
            "keyword.replicate.reminder.paid-cost-reference",
            "their",
        ),
        (
            "Counter target triggered permanent or colorless spell.",
            "stack-item-counter",
            "selector.stack-item.ability-noun",
            "permanent",
        ),
        (
            "Counter target triggered ability or colorless permanent.",
            "stack-item-counter",
            "selector.stack-item.spell-noun",
            "permanent",
        ),
    ] {
        let parsed = parse_instant(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_ne!(ability.status, "canonical", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse_instant(&format!(
        "Replicate {{1}} {REPLICATE_REMINDER}\nCounter target triggered ability or colorless spell."
    ));
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.entities.iter())
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
