use super::*;
use crate::oracle::model::{AbilitySource, RecognizedEntity};
use crate::oracle::syntax::{AbilityInput, recognize_entities};

fn input<'a>(source: &'a AbilitySource, tokens: &'a [RecognizedEntity]) -> AbilityInput<'a> {
    AbilityInput {
        face_name: "Generic Additional Cost",
        face_type_line: "Instant",
        source,
        tokens,
        name_links: &[],
        name_resolution_failure: None,
    }
}

fn attempt(text: &str) -> MandatoryAdditionalSacrificeParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: text.len(),
    };
    let tokens = recognize_entities(text);
    try_parse_promoted_mandatory_additional_sacrifice(&input(&source, &tokens))
}

fn parse(text: &str) -> CanonicalRuleDraft {
    match attempt(text) {
        MandatoryAdditionalSacrificeParseAttempt::Parsed(draft) => draft,
        result => panic!("mandatory sacrifice cost must lower: {result:?}"),
    }
}

#[test]
fn crop_rotation_cost_is_a_non_target_object_choice_and_atomic_sacrifice() {
    let draft = parse("As an additional cost to cast this spell, sacrifice a land.");
    let decision = &draft.rule["declaration"]["decisions"][0];
    let cost = &draft.rule["declaration"]["additionalCosts"][0];

    assert_eq!(decision["kind"], "chooseObjects");
    assert_eq!(decision["quantity"]["kind"], "exactly");
    assert_eq!(decision["candidates"]["controller"]["kind"], "controllerOf");
    assert_eq!(decision["candidates"]["where"]["value"], "Land");
    assert_eq!(cost["kind"], "sacrificePermanent");
    assert_eq!(cost["permanent"]["kind"], "chosenObject");
    assert_eq!(draft.rule["effects"].as_array().map(Vec::len), Some(0));
}

#[test]
fn casting_cost_kind_is_independent_of_printed_card_type_in_both_modes() {
    use crate::oracle::model::RecognizerProvenance;
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};

    for type_line in ["Creature", "Artifact", "Enchantment", "Instant"] {
        let request = || OracleCardParseRequest {
            card_name: "Generic Additional Cost Source".to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{1}{B}".to_string()),
            oracle_text: Some(
                "As an additional cost to cast this spell, sacrifice a creature.".to_string(),
            ),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        };
        let full = parse_oracle_card(request());
        let native = parse_oracle_card_native(request());
        assert_eq!(full.status, "canonical", "Full mode for {type_line}");
        assert_eq!(
            native.status, "canonical",
            "NativeOnly mode for {type_line}"
        );
        assert_eq!(full.abilities.len(), 1);
        assert_eq!(native.abilities.len(), 1);
        assert_eq!(full.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(full.abilities[0].rule, native.abilities[0].rule);
        let rule = full.abilities[0]
            .rule
            .as_ref()
            .expect("native casting cost");
        assert_eq!(rule["kind"], "spellAbility");
        assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseObjects");
        assert_eq!(
            rule["declaration"]["additionalCosts"][0]["kind"],
            "sacrificePermanent"
        );
    }
}

#[test]
fn sibling_union_lowers_to_one_choice_without_a_combined_cost_operation() {
    let draft =
        parse("As an additional cost to cast this spell, sacrifice an artifact or creature.");
    let predicate = &draft.rule["declaration"]["decisions"][0]["candidates"]["where"];

    assert_eq!(predicate["kind"], "or");
    assert_eq!(predicate["operands"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        draft.rule["declaration"]["additionalCosts"][0]["kind"],
        "sacrificePermanent"
    );
}

#[test]
fn claim_requires_the_mandatory_single_sentence_shape() {
    for text in [
        "As an additional cost to cast this spell, you may sacrifice a land.",
        "As an additional cost to cast this spell, sacrifice this land.",
        "As an additional cost to cast this spell, sacrifice a creature or pay {3}{B}.",
        "As an additional cost to cast this spell, sacrifice a creature and pay 2 life.",
        "As an additional cost to cast this spell, sacrifice a creature and discard a card.",
        "As an additional cost to cast this spell, sacrifice a creature and an artifact.",
        "As an additional cost to cast this spell, sacrifice a creature or a planeswalker.",
        "Sacrifice a land: Add {G}.",
    ] {
        let source = AbilitySource {
            face_id: None,
            text: text.to_string(),
            line_start: 0,
            line_end: text.len(),
        };
        let tokens = recognize_entities(text);
        assert!(!claims_promoted_mandatory_additional_sacrifice_family(
            &input(&source, &tokens)
        ));
    }
}

#[test]
fn claim_preserves_predicate_connectors_without_a_second_action_or_article() {
    for text in [
        "As an additional cost to cast this spell, sacrifice an artifact or creature.",
        "As an additional cost to cast this spell, sacrifice an artifact and creature.",
    ] {
        let source = AbilitySource {
            face_id: None,
            text: text.to_string(),
            line_start: 0,
            line_end: text.len(),
        };
        let tokens = recognize_entities(text);
        assert!(claims_promoted_mandatory_additional_sacrifice_family(
            &input(&source, &tokens)
        ));
    }
}

#[test]
fn subtype_and_literal_property_costs_use_shared_object_predicates() {
    let subtype = parse("As an additional cost to cast this spell, sacrifice a Goblin.");
    assert_eq!(
        subtype.rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "subtypeContains", "value": "Goblin" })
    );
    let numeric = parse(
        "As an additional cost to cast this spell, sacrifice a creature with power 4 or greater.",
    );
    let predicate = &numeric.rule["declaration"]["decisions"][0]["candidates"]["where"];
    assert_eq!(predicate["kind"], "and");
    assert_eq!(predicate["operands"][1]["left"]["kind"], "powerOf");
    assert_eq!(predicate["operands"][1]["operator"], ">=");
}

#[test]
fn unsupported_context_is_unpromoted_but_impossible_domain_is_failed() {
    for text in [
        "As an additional cost to cast this spell, sacrifice a creature that shares a creature type with it.",
        "As an additional cost to cast this spell, sacrifice a creature with mana value X.",
    ] {
        assert!(matches!(
            attempt(text),
            MandatoryAdditionalSacrificeParseAttempt::Unpromoted(_)
        ));
    }
    assert!(matches!(
        attempt("As an additional cost to cast this spell, sacrifice an instant."),
        MandatoryAdditionalSacrificeParseAttempt::Failed(_)
    ));
}

#[test]
fn default_preserves_unsupported_cost_context_with_a_diagnostic() {
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};
    let request = OracleCardParseRequest {
        card_name: "Generic Additional Cost".into(),
        type_line: "Sorcery".into(),
        mana_cost: Some("{1}{R}".into()),
        oracle_text: Some(
            "As an additional cost to cast this spell, sacrifice a creature that shares a creature type with it.".into(),
        ),
        layout: Some("normal".into()),
        faces: vec![],
    };
    let strict = parse_oracle_card_native(request.clone());
    assert_eq!(strict.status, "unsupported");
    assert!(strict.abilities[0].diagnostics.iter().any(|diagnostic| {
        diagnostic.state.as_deref() == Some("additional-cost.mandatory-sacrifice.terminator")
    }));
    let full = parse_oracle_card(request);
    assert_eq!(full.status, "unsupported");
    assert_eq!(
        full.abilities[0].recognizer,
        crate::oracle::model::RecognizerProvenance::Native
    );
    assert!(
        full.abilities[0]
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "oracle_machine_failure" })
    );
}
