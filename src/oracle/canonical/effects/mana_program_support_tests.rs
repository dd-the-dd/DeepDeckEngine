use super::*;
use crate::oracle::model::{AbilitySource, OracleCardParseRequest, RecognizerProvenance};
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};
use crate::oracle::{parse_oracle_card, parse_oracle_card_native};

fn with_input<T>(text: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    operation(&AbilityInput {
        face_name: "Generic Mana Source",
        face_type_line: "Artifact",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn old_and_generic_parents_share_typed_backend_capability_limits() {
    for (text, state) in [
        (
            "{T}: Add {G}. Add {R}.",
            "program.mana.backend-effect-order",
        ),
        ("{X}, {T}: Add {G}.", "program.mana.backend-cost-context"),
    ] {
        with_input(text, |input| {
            let old = parse_promoted_activated_mana(input)
                .err()
                .expect("unsupported execution");
            assert_eq!(old.state, state);
            let EffectProgramParseAttempt::FailedSupport(generic) =
                try_parse_contextual_effect_program(input)
            else {
                panic!("unsafe execution is not a safe legacy deferral");
            };
            assert_eq!(generic.state, old.state);
        });
    }
}

#[test]
fn one_production_keeps_fixed_vectors_choices_and_followups_distinct() {
    for text in [
        "{T}: Add {G}{R}.",
        "{T}: Add {G} or {R}.",
        "{T}: Add {G}. You gain 1 life.",
    ] {
        with_input(text, |input| {
            let established = parse_promoted_activated_mana(input).expect("supported compact mana");
            let EffectProgramParseAttempt::Parsed(generic) =
                try_parse_contextual_effect_program(input)
            else {
                panic!("one production and supported followups remain executable");
            };
            assert_eq!(generic.rule, established.rule);
            assert_eq!(generic.rule["kind"], "manaAbility");
        });
    }
}

#[test]
fn unsafe_execution_never_returns_full_legacy_canonical_or_native_coverage() {
    for native_only in [false, true] {
        for (text, state) in [
            (
                "{T}: Add {G}. Add {R}.",
                "program.mana.backend-effect-order",
            ),
            (
                "({T}: Add {G}. Add {R}.)",
                "program.mana.backend-effect-order",
            ),
            (
                "Sacrifice this creature: Add {G}. Add {R}.",
                "program.mana.backend-effect-order",
            ),
            ("{X}, {T}: Add {G}.", "program.mana.backend-cost-context"),
            (
                "{T}, Discard this card: Add {G}.",
                "program.activation.source-zone-conflict",
            ),
        ] {
            let request = OracleCardParseRequest {
                card_name: "Generic Mana Source".to_string(),
                type_line: "Artifact".to_string(),
                mana_cost: Some("{2}".to_string()),
                oracle_text: Some(text.to_string()),
                layout: Some("normal".to_string()),
                faces: Vec::new(),
            };
            let parsed = if native_only {
                parse_oracle_card_native(request)
            } else {
                parse_oracle_card(request)
            };
            assert_eq!(parsed.status, "unsupported", "{text}");
            let ability = &parsed.abilities[0];
            assert_eq!(ability.recognizer, RecognizerProvenance::Native);
            assert!(ability.rule.is_none());
            assert!(
                ability.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == "oracle_machine_failure"
                        && diagnostic.state.as_deref() == Some(state)
                }),
                "{text}"
            );
            assert!(
                !ability
                    .diagnostics
                    .iter()
                    .any(|diagnostic| { diagnostic.code == "native_candidate_unpromoted" })
            );
        }
    }
}

#[test]
fn compact_and_generic_parents_share_library_classification_and_cost_x_binding() {
    for text in [
        "{T}: Add {G}. Draw a card.",
        "{T}: Add {G}. Surveil 1.",
        "{X}, {T}: Add {G}. Draw X cards.",
        "{T}: Add {G}. Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
    ] {
        with_input(text, |input| {
            let compact =
                parse_promoted_activated_mana(input).expect("library-moving ordinary activation");
            let EffectProgramParseAttempt::Parsed(generic) =
                try_parse_contextual_effect_program(input)
            else {
                panic!("generic parent must share classification and lowering");
            };
            assert_eq!(compact.rule, generic.rule, "{text}");
            assert_eq!(compact.rule["kind"], "activatedAbility");
            if text.starts_with("{X}") {
                assert_eq!(compact.rule["declaration"]["decisions"][0], x_value());
                assert_eq!(
                    compact.rule["effects"][1]["count"],
                    decision_result("xValue")
                );
            }
        });
    }
}

#[test]
fn reading_library_size_does_not_count_as_moving_a_card() {
    let gain = LocalEffectNode::GainLife(GainLifeEffectNode {
        player: LocalPlayerReference::Controller,
        amount: NumericExpression::CardsInZone {
            player: LocalPlayerReference::Controller,
            zone: LocalZone::Library,
        },
    });
    assert!(!moves_cards_to_or_from_library(&gain));

    // Self-mill is not yet in the shared action sequence. Reuse the real
    // targeted-mill child to prove the typed execution fact without claiming
    // that missing grammar or lowering as supported.
    let tokens = recognize_oracle_entities("Target player mills two cards.");
    let mut cursor = OracleTokenCursor::new("test.mill-library-movement-fact", &tokens);
    let mill = recognize_mill_cards_effect(&mut cursor).expect("the existing targeted-mill child");
    assert!(moves_cards_to_or_from_library(&LocalEffectNode::MillCards(
        mill
    )));
}

#[test]
fn same_library_ordering_is_distinct_from_entering_or_leaving_the_zone() {
    let tokens = recognize_oracle_entities(
        "Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
    );
    let mut cursor = OracleTokenCursor::new("test.library-boundary-facts", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    let [LocalEffectNode::ControllerLibrarySearch(search)] = sequence.effects.as_slice() else {
        panic!("the shared search grammar owns this node");
    };
    for (destination, crosses_boundary) in [
        (SearchDestination::LibraryTop, false),
        (SearchDestination::Hand, true),
        (SearchDestination::Battlefield { tapped: true }, true),
    ] {
        let mut variant = search.clone();
        variant.destination = destination;
        assert_eq!(
            moves_cards_to_or_from_library(&LocalEffectNode::ControllerLibrarySearch(variant)),
            crosses_boundary,
        );
    }
}
