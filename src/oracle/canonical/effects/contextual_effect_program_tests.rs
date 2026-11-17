use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};

fn with_input<T>(type_line: &str, text: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_oracle_entities(text);
    let named_tokens = ExactNameTokenIndex::from_names(
        crate::card_catalog::named_token_names().expect("local named-token catalog"),
    );
    resolve_named_token_spans(&mut tokens, &named_tokens);
    operation(&AbilityInput {
        face_name: "Generic Program Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parse(type_line: &str, text: &str) -> EffectProgramParseAttempt {
    with_input(type_line, text, try_parse_contextual_effect_program)
}

fn parsed_rule(type_line: &str, text: &str) -> Value {
    let EffectProgramParseAttempt::Parsed(parsed) = parse(type_line, text) else {
        panic!("expected a complete generic program: {text}");
    };
    parsed.rule
}

fn contains_rule_kind(value: &Value, expected: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("kind").and_then(Value::as_str) == Some(expected)
                || fields
                    .values()
                    .any(|value| contains_rule_kind(value, expected))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| contains_rule_kind(value, expected)),
        _ => false,
    }
}

#[test]
fn spell_program_uses_shared_sequence_without_a_first_verb_allowlist() {
    for text in [
        "Draw two cards and you gain 3 life.",
        "You gain 3 life, then draw two cards.",
    ] {
        let rule = parsed_rule("Instant", text);
        assert_eq!(rule["kind"], "spellAbility");
        assert!(rule.get("declaration").is_none());
        let effects = rule["effects"].as_array().expect("independent effects");
        assert_eq!(effects.len(), 2);
        assert!(effects.iter().any(|effect| effect["kind"] == "drawCards"));
        assert!(effects.iter().any(|effect| effect["kind"] == "gainLife"));
    }
}

#[test]
fn activated_program_reuses_arbitrary_supported_costs_and_effects() {
    let rule = parsed_rule("Creature", "Pay 2 life: You gain 1 life.");
    assert_eq!(rule["kind"], "activatedAbility");
    assert_eq!(rule["costs"][0]["kind"], "payLife");
    assert_eq!(rule["costs"][0]["amount"]["value"], 2);
    assert_eq!(rule["effects"][0]["kind"], "gainLife");
    assert_eq!(rule["declaration"]["decisions"], json!([]));
}

#[test]
fn activation_x_is_declared_before_effect_choices_and_is_not_source_cast_x() {
    let rule = parsed_rule("Creature", "{X}, {T}: Draw X cards.");
    assert_eq!(rule["declaration"]["decisions"], json!([x_value()]));
    assert_eq!(rule["effects"][0]["count"], decision_result("xValue"));
    assert!(!contains_rule_kind(&rule["effects"], "sourceCastXValue"));
}

#[test]
fn discard_self_supplies_hand_context_and_conflicting_source_zones_fail_closed() {
    let rule = parsed_rule("Instant", "{1}, Discard this card: Draw a card.");
    assert_eq!(rule["kind"], "activatedAbility");
    assert_eq!(rule["activationZone"], "hand");
    for text in [
        "{T}, Discard this card: Draw a card.",
        "Sacrifice this creature, Discard this card: Draw a card.",
    ] {
        let EffectProgramParseAttempt::FailedSupport(failure) = parse("Creature", text) else {
            panic!("a source cannot occupy hand and battlefield during one cost payment");
        };
        assert_eq!(failure.state, "program.activation.source-zone-conflict");
    }
}

#[test]
fn outer_trigger_replacement_loyalty_and_permanent_static_contexts_are_distinct() {
    for (type_line, text) in [
        ("Creature", "Draw a card."),
        ("Creature — Sorcery", "Draw a card."),
        ("Instant", "Whenever you gain life, draw a card."),
        ("Instant", "If you would draw a card, gain 2 life instead."),
        ("Planeswalker", "+1: Draw a card."),
        (
            "Enchantment",
            "Creatures you control have \"{T}: Draw a card.\"",
        ),
    ] {
        assert!(matches!(
            parse(type_line, text),
            EffectProgramParseAttempt::NotApplicable
        ));
    }
    let context = with_input(
        "Instant",
        "Target creature gains \"{T}: Draw a card.\" until end of turn.",
        admit_context,
    );
    assert_eq!(context, Some(EffectProgramContext::Spell));
}

#[test]
fn immediate_mana_facts_do_not_read_a_created_tokens_abilities() {
    let mana = parsed_rule(
        "Creature",
        "Sacrifice this creature: Add {B}. You gain 1 life.",
    );
    assert_eq!(mana["kind"], "manaAbility");
    assert_eq!(mana["effects"][0]["kind"], "addMana");
    assert_eq!(mana["effects"][1]["kind"], "gainLife");
    assert!(mana.get("declaration").is_none());

    let treasure = parsed_rule("Artifact", "{T}: Draw a card and create a Treasure token.");
    assert_eq!(treasure["kind"], "activatedAbility");
    assert_eq!(treasure["effects"][1]["kind"], "createTokens");
    assert_eq!(treasure["effects"][1]["token"]["name"], "Treasure");
}

#[test]
fn unsupported_mana_execution_order_is_an_explicit_backend_support_limit() {
    for text in ["{T}: Add {G}. Add {R}."] {
        let EffectProgramParseAttempt::FailedSupport(failure) = parse("Artifact", text) else {
            panic!("the current mana backend cannot preserve this production program");
        };
        assert_eq!(failure.state, "program.mana.backend-effect-order");
    }
}

#[test]
fn mana_context_does_not_approximate_hand_or_variable_payment_support() {
    for text in [
        "Discard this card: Add {G}.",
        "{X}, {T}: Add {G} and you gain 1 life.",
    ] {
        let EffectProgramParseAttempt::FailedSupport(failure) = parse("Artifact", text) else {
            panic!("the current battlefield mana backend needs the missing payment context");
        };
        assert_eq!(failure.state, "program.mana.backend-cost-context");
    }
}

#[test]
fn distinct_action_or_is_not_silently_lowered_as_a_sequence() {
    let EffectProgramParseAttempt::Unpromoted(failure) =
        parse("Instant", "Draw a card or create a Treasure token.")
    else {
        panic!("alternative actions need a typed choice, not two sequential effects");
    };
    assert!(failure.expected.contains("`and`"));
}

#[test]
fn genuine_committed_numeric_or_connector_failures_remain_hard() {
    for text in ["Draw enormous cards.", "Draw a card then."] {
        assert!(matches!(
            parse("Instant", text),
            EffectProgramParseAttempt::Failed(_)
        ));
    }
}

#[test]
fn full_and_native_validate_program_x_after_composition_in_the_same_face() {
    use crate::oracle::model::{OracleCardFace, OracleCardParseRequest, RecognizerProvenance};
    use crate::oracle::{parse_oracle_card, parse_oracle_card_native};

    let spell = "Draw X cards and create a Treasure token.";
    for native_only in [false, true] {
        for (mana, text, canonical) in [
            ("{X}{U}", spell, true),
            ("{1}{U}", spell, false),
            ("{X}{U}", "{T}: Draw X cards.", false),
            ("{X}{U}", "{X}, {T}: Draw X cards.", true),
        ] {
            let request = OracleCardParseRequest {
                card_name: "Generic Binding Source".to_string(),
                type_line: "Instant".to_string(),
                mana_cost: Some(mana.to_string()),
                oracle_text: Some(text.to_string()),
                layout: Some("normal".to_string()),
                faces: Vec::new(),
            };
            let result = if native_only {
                parse_oracle_card_native(request)
            } else {
                parse_oracle_card(request)
            };
            let ability = &result.abilities[0];
            assert_eq!(ability.recognizer, RecognizerProvenance::Native);
            assert_eq!(ability.status == "canonical", canonical, "{mana}: {text}");
            if !canonical {
                assert!(ability.rule.is_none());
                assert!(ability.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == "oracle_context_binding_failure"
                        && diagnostic.state.as_deref() == Some("binding.x-value.source")
                }));
            }
        }

        let request = OracleCardParseRequest {
            card_name: "Unbound Face // Producing Face".to_string(),
            type_line: "Instant // Instant".to_string(),
            mana_cost: Some("{1}{U} // {X}{G}".to_string()),
            oracle_text: None,
            layout: Some("split".to_string()),
            faces: vec![
                OracleCardFace {
                    id: "unbound".to_string(),
                    name: "Unbound Face".to_string(),
                    type_line: "Instant".to_string(),
                    mana_cost: Some("{1}{U}".to_string()),
                    oracle_text: spell.to_string(),
                    power: None,
                    toughness: None,
                    loyalty: None,
                },
                OracleCardFace {
                    id: "producer".to_string(),
                    name: "Producing Face".to_string(),
                    type_line: "Instant".to_string(),
                    mana_cost: Some("{X}{G}".to_string()),
                    oracle_text: "You gain 1 life.".to_string(),
                    power: None,
                    toughness: None,
                    loyalty: None,
                },
            ],
        };
        let result = if native_only {
            parse_oracle_card_native(request)
        } else {
            parse_oracle_card(request)
        };
        let consumer = &result.abilities[0];
        assert_eq!(consumer.source.face_id.as_deref(), Some("unbound"));
        assert_eq!(consumer.recognizer, RecognizerProvenance::Native);
        assert_eq!(consumer.status, "unsupported");
        assert!(consumer.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "oracle_context_binding_failure"
                && diagnostic.found.as_deref() == Some("{1}{U}")
        }));
    }
}

#[test]
fn library_moves_use_ordinary_activation_context_and_keep_effect_order() {
    for (text, kinds) in [
        ("{T}: Add {G}. Draw a card.", vec!["addMana", "drawCards"]),
        ("{T}: Draw a card. Add {G}.", vec!["drawCards", "addMana"]),
        (
            "{T}: Add {G}. Surveil 1.",
            vec![
                "addMana",
                "lookAtTopCards",
                "chooseCards",
                "chooseOrder",
                "moveCards",
                "chooseOrder",
                "moveCards",
                "emitAbilitySignal",
            ],
        ),
    ] {
        let rule = parsed_rule("Artifact", text);
        assert_eq!(rule["kind"], "activatedAbility", "{text}");
        assert_eq!(rule["declaration"]["decisions"], json!([]));
        assert_eq!(
            rule["effects"]
                .as_array()
                .unwrap()
                .iter()
                .map(|effect| effect["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            kinds
        );
    }
    // A Clue's later ability is not executed by this activation.
    let clue = parsed_rule("Artifact", "{T}: Add {G}. Create a Clue token.");
    assert_eq!(clue["kind"], "manaAbility");
}
