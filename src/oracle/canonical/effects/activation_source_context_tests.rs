use super::*;
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card_native};

fn parse(text: &str, face_cost: &str, face_type: &str) -> OracleCardParseResult {
    parse_oracle_card_native(OracleCardParseRequest {
        card_name: "Activation Context Witness".into(),
        type_line: face_type.into(),
        mana_cost: Some(face_cost.into()),
        oracle_text: Some(text.into()),
        layout: Some("normal".into()),
        faces: vec![],
    })
}

#[test]
fn source_availability_requires_retaining_costs_and_a_permanent_face() {
    use ExplicitActivationSourceAvailability::{
        RetainedBattlefieldPermanent, RetainedOrDeparted, Unproved,
    };
    for (text, expected) in [
        ("{0}", RetainedBattlefieldPermanent),
        ("{T}", RetainedBattlefieldPermanent),
        ("Pay 2 life", RetainedBattlefieldPermanent),
        ("Pay {E}{E}", RetainedBattlefieldPermanent),
        ("Sacrifice another creature", RetainedBattlefieldPermanent),
        ("Sacrifice this creature", Unproved),
        // The payer may sacrifice the source itself: a self-reference still
        // names it (CR 113.7a) and does nothing once it left (CR 400.7).
        ("Sacrifice a creature", RetainedOrDeparted),
        ("Sacrifice two creatures", RetainedOrDeparted),
        ("Discard this card", Unproved),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("activation-source-cost-unit", &tokens);
        let costs = recognize_activation_cost_sequence(&mut cursor).unwrap();
        let context =
            activation_body_context("Creature", &costs, &EffectRecognitionContext::default());
        assert_eq!(context.activation_source_availability, expected, "{text}");
        assert!(context.source.is_none());
        assert!(!context.source_cast_x);
        // A card discarded or sacrificed to pay for its own ability is the
        // only object a leading `it` can name; retained-source costs and
        // another sacrificed object prove no anaphor.
        assert_eq!(
            context.source_anaphor_binding,
            if text == "Discard this card" || text == "Sacrifice this creature" {
                SourceAnaphorBinding::EventSubject(LocalObjectReference::SourceSelf)
            } else {
                SourceAnaphorBinding::Unproved
            },
            "{text}"
        );
        let spell =
            activation_body_context("Instant", &costs, &EffectRecognitionContext::default());
        assert_eq!(
            spell.activation_source_availability,
            ExplicitActivationSourceAvailability::Unproved
        );
        let subtype = activation_body_context(
            "Emblem — Creature",
            &costs,
            &EffectRecognitionContext::default(),
        );
        assert_eq!(
            subtype.activation_source_availability,
            ExplicitActivationSourceAvailability::Unproved
        );
    }
}

#[test]
fn cost_context_is_derived_once_and_identity_preserves_old_caller_trace() {
    let tokens = recognize_entities("{0}: Draw a card.");
    let mut original_cursor = OracleTokenCursor::new("activation-source-parity", &tokens);
    let old = try_recognize_activated_effect_sequence_with_annotation(
        &mut original_cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    );
    let calls = std::cell::Cell::new(0);
    let mut cursor = OracleTokenCursor::new("activation-source-parity", &tokens);
    let new = try_recognize_activated_effect_sequence_with_cost_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
        |costs, base| {
            calls.set(calls.get() + 1);
            assert_eq!(costs.costs.len(), 1);
            base.clone()
        },
    );
    let ActivatedEffectSequenceParseAttempt::Parsed(old) = old else {
        panic!("old caller")
    };
    let ActivatedEffectSequenceParseAttempt::Parsed(new) = new else {
        panic!("new callback")
    };
    assert_eq!(calls.get(), 1);
    assert_eq!(old, new);
    assert_eq!(original_cursor.transitions(), cursor.transitions());
}

#[test]
fn explicit_availability_does_not_create_pronouns_or_borrow_casting_x() {
    for text in [
        "{0}: It gets +1/+0 until end of turn.",
        "{0}: Sacrifice it.",
        "{0}: Return it to the battlefield transformed under its owner's control.",
    ] {
        assert_eq!(
            parse(text, "{0}", "Creature").status,
            "unsupported",
            "{text}"
        );
    }
    let borrowed = parse(
        "{0}: This creature gets +X/+0 until end of turn.",
        "{X}{G}",
        "Creature",
    );
    assert_eq!(borrowed.status, "unsupported");
    assert!(
        borrowed.abilities[0]
            .diagnostics
            .iter()
            .any(|d| d.code == "oracle_context_binding_failure")
    );
    let declared = parse(
        "{X}: This creature gets +X/+0 until end of turn.",
        "{0}",
        "Creature",
    );
    assert_eq!(declared.status, "canonical");
    let rule = declared.abilities[0].rule.as_ref().unwrap();
    assert_eq!(
        rule["declaration"]["decisions"],
        json!([{"kind":"chooseNumber","id":"xValue","minimum":0}])
    );
    assert_eq!(rule["effects"][0]["power"], decision_result("xValue"));

    let local = parse(
        "{0}: This creature gets +1/+0 until end of turn and it gains flying until end of turn.",
        "{0}",
        "Creature",
    );
    assert_eq!(local.status, "canonical");
    let rule = local.abilities[0].rule.as_ref().unwrap();
    assert_eq!(rule["effects"][1]["object"], self_ref());
}

#[test]
fn signed_pair_boundary_is_independent_of_left_expression_words() {
    let parenthesized = parse(
        "Target creature gets +(X plus two)/+0 until end of turn.",
        "{X}{R}",
        "Instant",
    );
    let ordinary = parse(
        "Target creature gets +X plus two/+0 until end of turn.",
        "{X}{R}",
        "Instant",
    );
    assert_eq!(ordinary.status, "canonical");
    assert_eq!(ordinary.abilities[0].rule, parenthesized.abilities[0].rule);
    let property = parse(
        "{0}: This creature gets +this creature's power/+0 until end of turn.",
        "{0}",
        "Creature",
    );
    assert_eq!(property.status, "canonical");
    for ending in ["power", "control", "frobnicate"] {
        let text = format!("{ending}/+0");
        let tokens = recognize_entities(&text);
        assert_eq!(tokens[0].raw, ending);
        assert_eq!(tokens[1].token_type, "statPairSeparator");
        for token in tokens {
            assert_eq!(&text[token.span_start..token.span_end], token.raw);
        }
    }
    assert_eq!(recognize_entities("+2/+0")[0].token_type, "counterValue");
    assert_eq!(
        parse(
            "Target creature gets +frobnicate/+0 until end of turn.",
            "{0}",
            "Instant"
        )
        .status,
        "unsupported"
    );
}
