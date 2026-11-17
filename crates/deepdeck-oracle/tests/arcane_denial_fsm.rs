use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const ARCANE_DENIAL_ORACLE: &str = "Counter target spell. Its controller may draw up to two cards at the beginning of the next turn's upkeep.\nYou draw a card at the beginning of the next turn's upkeep.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Arcane Denial".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn counter_and_both_next_upkeep_draws_are_native_primitives() {
    let parsed = parse(ARCANE_DENIAL_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetSpell",
                    "kind": "chooseTargets",
                    "candidates": { "kind": "spells" },
                    "minimum": 1,
                    "maximum": 1,
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "counteredSpellController",
                    "value": {
                        "kind": "controllerOf",
                        "object": { "kind": "chosenTarget", "id": "targetSpell" },
                    },
                },
                {
                    "kind": "counterSpell",
                    "spell": { "kind": "chosenTarget", "id": "targetSpell" },
                },
                {
                    "kind": "installDelayedStepTrigger",
                    "step": "upkeep",
                    "controller": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "effects": [
                        {
                            "kind": "chooseNumber",
                            "id": "delayedDrawCount",
                            "player": {
                                "kind": "boundValue",
                                "id": "counteredSpellController",
                            },
                            "minimum": { "kind": "integer", "value": 0 },
                            "maximum": { "kind": "integer", "value": 2 },
                        },
                        {
                            "kind": "drawCards",
                            "player": {
                                "kind": "boundValue",
                                "id": "counteredSpellController",
                            },
                            "count": {
                                "kind": "decisionResult",
                                "decisionId": "delayedDrawCount",
                            },
                        },
                    ],
                    "captureBindings": ["counteredSpellController"],
                },
            ],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "installDelayedStepTrigger",
                "step": "upkeep",
                "controller": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "count": { "kind": "integer", "value": 1 },
                }],
            }],
        }))
    );
}

#[test]
fn selector_counts_and_delayed_steps_generalize_without_card_identity() {
    let parsed = parse(
        "Counter target noncreature spell. Its controller may draw up to three cards at the beginning of the next turn's end step.\nYou draw two cards at the beginning of the next turn's end step.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let first = parsed.abilities[0].rule.as_ref().expect("sibling rule");
    assert_eq!(
        first["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "not"
    );
    assert_eq!(first["effects"][2]["step"], "endStep");
    assert_eq!(first["effects"][2]["effects"][0]["maximum"]["value"], 3);
    let second = parsed.abilities[1].rule.as_ref().expect("sibling draw");
    assert_eq!(second["effects"][0]["step"], "endStep");
    assert_eq!(second["effects"][0]["effects"][0]["count"]["value"], 2);
}

#[test]
fn missing_optional_draw_count_is_a_committed_native_failure() {
    let parsed = parse(
        "Counter target spell. Its controller may draw up to cards at the beginning of the next turn's upkeep.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("counter-delayed-draw")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("numeric.atom.start")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("cards"));
}

#[test]
fn unsupported_delayed_step_is_reported_by_the_shared_temporal_child() {
    let parsed = parse(
        "Counter target spell. Its controller may draw up to two cards at the beginning of the next turn's draw step.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.delayed-step.step")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("draw"));
}

#[test]
fn trailing_sentence_fails_full_consumption_in_the_sequence_parent() {
    let parsed = parse(
        "Counter target spell. Its controller may draw up to two cards at the beginning of the next turn's upkeep. Draw a card.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("sequence.counter-delayed-draw.complete")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("Draw"));
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(ARCANE_DENIAL_ORACLE);
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.entities.iter())
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
