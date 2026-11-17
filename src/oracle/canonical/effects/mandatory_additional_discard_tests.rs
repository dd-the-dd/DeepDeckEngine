use super::*;
use crate::oracle::model::{AbilitySource, RecognizedEntity};
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn input<'a>(source: &'a AbilitySource, tokens: &'a [RecognizedEntity]) -> AbilityInput<'a> {
    AbilityInput {
        face_name: "Generic Additional Discard Cost",
        face_type_line: "Instant",
        source,
        tokens,
        name_links: &[],
        name_resolution_failure: None,
    }
}

fn attempt(text: &str) -> MandatoryAdditionalDiscardParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    try_parse_promoted_mandatory_additional_discard(&input(&source, &tokens))
}

fn parsed(text: &str) -> CanonicalRuleDraft {
    match attempt(text) {
        MandatoryAdditionalDiscardParseAttempt::Parsed(draft) => draft,
        other => panic!("complete discard cost must lower: {other:?}"),
    }
}

#[test]
fn exact_singleton_is_a_non_target_hand_choice_and_scalar_payment() {
    let result = parsed("As an additional cost to cast this spell, discard a card.");
    assert_eq!(
        result.rule,
        json!({
            "kind": "spellAbility", "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "kind": "chooseObjects", "id": "additionalCostCard",
                    "candidates": {
                        "kind": "cards", "zone": {
                            "kind": "hand", "player": { "kind": "controllerOf", "object": { "kind": "self" } }
                        }, "where": null
                    },
                    "quantity": { "kind": "exactly", "value": { "kind": "integer", "value": 1 } }
                }],
                "additionalCosts": [{ "kind": "discardCard", "card": { "kind": "chosenObject", "id": "additionalCostCard" } }]
            },
            "effects": []
        })
    );
}

#[test]
fn plural_payment_uses_one_distinct_collection_choice_and_indexed_scalar_consumers() {
    let result = parsed("As an additional cost to cast this spell, discard two cards.");
    let declaration = &result.rule["declaration"];
    assert_eq!(declaration["decisions"].as_array().map(Vec::len), Some(1));
    assert_eq!(declaration["decisions"][0]["quantity"]["value"]["value"], 2);
    assert_eq!(
        declaration["additionalCosts"],
        json!([
            { "kind": "discardCard", "card": { "kind": "chosenObject", "id": "additionalCostCard:0" } },
            { "kind": "discardCard", "card": { "kind": "chosenObject", "id": "additionalCostCard:1" } }
        ])
    );
    let filtered = parsed("As an additional cost to cast this spell, discard a land card.");
    assert_eq!(
        filtered.rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "cardTypeContains", "value": "Land"
        })
    );
}

#[test]
fn the_atomic_claim_preserves_sacrifice_optional_alternative_and_compound_families() {
    for text in [
        "As an additional cost to cast this spell, sacrifice a creature.",
        "As an additional cost to cast this spell, you may discard a card.",
        "As an additional cost to cast this spell, discard a card or pay {5}.",
        "As an additional cost to cast this spell, discard a card and sacrifice a creature.",
        "As an additional cost to cast this spell, discard a card. Draw a card.",
        "Discard a card: Draw a card.",
    ] {
        assert!(
            matches!(
                attempt(text),
                MandatoryAdditionalDiscardParseAttempt::NotClaimed
            ),
            "{text}"
        );
    }
    assert!(matches!(
        attempt("As an additional cost to cast this spell, discard a card at random."),
        MandatoryAdditionalDiscardParseAttempt::Unpromoted(_)
    ));
}

#[test]
fn casting_parent_is_independent_of_printed_card_type_and_classifier_hint() {
    use crate::oracle::model::RecognizerProvenance;
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};
    for type_line in ["Instant", "Sorcery", "Artifact Creature", "Creature"] {
        let request = OracleCardParseRequest {
            card_name: "Generic Discard Payment Source".into(),
            type_line: type_line.into(),
            mana_cost: Some("{1}{R}".into()),
            oracle_text: Some("As an additional cost to cast this spell, discard a card.".into()),
            layout: Some("normal".into()),
            faces: vec![],
        };
        let native = parse_oracle_card_native(request.clone());
        let full = parse_oracle_card(request);
        assert_eq!(native.status, "canonical", "NativeOnly {type_line}");
        assert_eq!(full.status, "canonical", "Full {type_line}");
        assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(full.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(native.abilities[0].rule, full.abilities[0].rule);
    }
}
