use super::super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

/// The shared payment action projected onto the activation schema as the
/// third cost of its sequence.
fn parse(text: &str) -> MachineResult<SelectedSacrificeCostNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selected-sacrifice-cost-test", &tokens);
    let payment = recognize_payment_action(&mut cursor)?;
    cursor.finish()?;
    match activation_cost_from_payment(&mut cursor, payment, 2)? {
        LocalActivationCostNode::SacrificeObject(node) => Ok(node),
        other => panic!("{text}: a selected sacrifice, not {other:?}"),
    }
}

#[test]
fn selected_cost_preserves_generic_type_subtype_union_and_numeric_predicates() {
    for text in [
        "Sacrifice a creature",
        "Sacrifice an artifact or creature",
        "Sacrifice a Goblin",
        "Sacrifice a noncreature artifact",
        "Sacrifice a creature with power 2 or less",
        "Sacrifice a creature you control",
    ] {
        let node = parse(text).expect("the shared permanent query is valid");
        assert_eq!(node.cost_index, 2);
        assert!(node.object.predicate.supports_domain(ObjectKind::Permanent));
        assert!(node.object.predicate.has_nominal_head());
    }
    assert!(matches!(
        parse("Sacrifice an artifact or creature")
            .unwrap()
            .object
            .predicate,
        ObjectPredicate::Union(_)
    ));
}

#[test]
fn sacrifice_domain_controller_and_complete_consumption_are_not_approximated() {
    for (text, state) in [
        (
            "Sacrifice an instant",
            "cost.payment.sacrifice.permanent-domain",
        ),
        (
            "Sacrifice a green",
            "cost.payment.sacrifice.permanent-domain",
        ),
        (
            "Sacrifice a creature an opponent controls",
            "cost.payment.sacrifice.controller",
        ),
    ] {
        assert_eq!(
            parse(text).expect_err("invalid cost must fail").state,
            state
        );
    }
    assert!(parse("Sacrifice a creature that shares a creature type with it").is_err());
}

#[test]
fn cost_sequence_owns_its_commas_and_assigns_distinct_payment_bindings() {
    let tokens = recognize_entities("{1}, Sacrifice a creature, Sacrifice an artifact, {T}");
    let mut cursor = OracleTokenCursor::new("selected-sacrifice-sequence-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor).unwrap();
    cursor.finish().unwrap();
    let [
        LocalActivationCostNode::PayMana(_),
        LocalActivationCostNode::SacrificeObject(first),
        LocalActivationCostNode::SacrificeObject(second),
        LocalActivationCostNode::TapSelf(_),
    ] = costs.costs.as_slice()
    else {
        panic!("one typed child for each independent cost");
    };
    assert_eq!((first.cost_index, second.cost_index), (1, 2));
}

#[test]
fn another_permanent_cost_does_not_force_the_source_onto_the_battlefield() {
    let tokens = recognize_entities("Discard this card, Sacrifice a creature");
    let mut cursor = OracleTokenCursor::new("selected-sacrifice-hand-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor).unwrap();
    let source = costs.source_zone_requirements();
    assert!(source.hand);
    assert!(!source.battlefield);

    let tokens = recognize_entities("{T}, Discard this card, Sacrifice a creature");
    let mut cursor = OracleTokenCursor::new("selected-sacrifice-hand-test", &tokens);
    assert_eq!(
        recognize_activation_cost_sequence(&mut cursor)
            .unwrap_err()
            .state,
        "activation.compound.source-zone-conflict"
    );
}

#[test]
fn full_and_native_parse_modes_preserve_the_selected_cost_and_its_choice() {
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};
    for text in [
        "Sacrifice a creature: Add one mana of any color.",
        "Sacrifice a land: You gain 2 life.",
    ] {
        let request = OracleCardParseRequest {
            card_name: "Generic Cost Source".into(),
            type_line: "Artifact".into(),
            mana_cost: Some("{2}".into()),
            oracle_text: Some(text.into()),
            layout: Some("normal".into()),
            faces: Vec::new(),
        };
        let full = parse_oracle_card(request.clone());
        let native = parse_oracle_card_native(request);
        assert_eq!(full.status, "canonical");
        assert_eq!(native.status, "canonical");
        assert_eq!(full.abilities[0].rule, native.abilities[0].rule);
        let rule = full.abilities[0].rule.as_ref().unwrap();
        assert_eq!(rule["costs"][0]["permanent"]["kind"], "chosenObject");
        assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseObjects");
    }
    let impossible = OracleCardParseRequest {
        card_name: "Incoherent Source Zones".into(),
        type_line: "Artifact".into(),
        mana_cost: Some("{2}".into()),
        oracle_text: Some("Discard this card, Sacrifice a creature, {T}: You gain 1 life.".into()),
        layout: Some("normal".into()),
        faces: Vec::new(),
    };
    let full = parse_oracle_card(impossible.clone());
    assert_eq!(full.status, "unsupported");
    assert_eq!(
        full.abilities[0].recognizer,
        crate::oracle::model::RecognizerProvenance::Native
    );
    assert!(full.abilities[0].diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "oracle_machine_failure"
            && diagnostic.state.as_deref() == Some("activation.compound.source-zone-conflict")
    }));
    assert_eq!(parse_oracle_card_native(impossible).status, "unsupported");
}

#[test]
fn selected_query_x_requires_a_producer_in_its_own_typed_activation_costs() {
    for mana in ["{X}", "{X}{X}{G}"] {
        let text = format!("{mana}, Sacrifice a creature with power X or less");
        let tokens = recognize_entities(&text);
        let mut cursor = OracleTokenCursor::new("selected-cost-x-scope-test", &tokens);
        let costs = recognize_activation_cost_sequence(&mut cursor).expect("local typed producer");
        cursor.finish().unwrap();
        assert!(
            costs
                .costs
                .iter()
                .any(|cost| matches!(cost, LocalActivationCostNode::SacrificeObject(_)))
        );
    }
    for text in [
        "Sacrifice a creature with power X or less",
        "{2}, Sacrifice a creature with power X or less",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("selected-cost-x-scope-test", &tokens);
        assert_eq!(
            recognize_activation_cost_sequence(&mut cursor)
                .unwrap_err()
                .state,
            "activation.compound.selected-cost-x-producer"
        );
    }
}

#[test]
fn nested_selected_cost_cannot_borrow_modal_or_printed_mana_x() {
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};
    for (type_line, text) in [
        (
            "Instant",
            "Choose two — • Target player creates X 0/1 colorless Eldrazi Spawn creature tokens with \"Sacrifice a creature with power X or less: Add {C}.\" • Target player scries X, then draws a card. • Exile target creature with mana value X or less. • Exile up to X target cards from graveyards.",
        ),
        (
            "Enchantment — Saga",
            "I — This Saga gains \"{T}, Sacrifice a creature with power X or less: Add {C}.\"",
        ),
    ] {
        let request = OracleCardParseRequest {
            card_name: "Synthetic Nested Selected Cost X Scope".into(),
            type_line: type_line.into(),
            mana_cost: Some("{X}{G}".into()),
            oracle_text: Some(text.into()),
            layout: Some("normal".into()),
            faces: vec![],
        };
        for parsed in [
            parse_oracle_card_native(request.clone()),
            parse_oracle_card(request),
        ] {
            assert_eq!(parsed.status, "unsupported");
            // The quote is compiled as its own ability: the grant fails at
            // the quote and carries the inner X-producer failure.
            assert!(
                parsed.abilities[0].diagnostics.iter().any(|diagnostic| {
                    diagnostic.state.as_deref() == Some("quoted-ability.body")
                        && diagnostic
                            .message
                            .contains("activation.compound.selected-cost-x-producer")
                }),
                "{:#?}",
                parsed.abilities[0].diagnostics
            );
        }
    }
}
