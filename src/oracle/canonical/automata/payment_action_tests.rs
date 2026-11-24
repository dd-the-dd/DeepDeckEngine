use super::super::*;
use super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

fn payment(text: &str) -> MachineResult<PaymentActionNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("payment-action-test", &tokens);
    let node = recognize_payment_action(&mut cursor)?;
    cursor.finish()?;
    Ok(node)
}

/// The payment action read at `start` inside a complete ability text.
fn payment_in_context(text: &str, start: usize) -> PaymentActionNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("payment-action-context-test", &tokens);
    cursor.advance_to(start).unwrap();
    recognize_payment_action(&mut cursor).unwrap_or_else(|failure| panic!("{text}: {failure:?}"))
}

fn activation_costs(text: &str) -> MachineResult<ActivationCostSequenceNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("payment-action-activation-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor)?;
    cursor.finish()?;
    Ok(costs)
}

fn resolution_program(text: &str) -> MachineResult<EffectProgramNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("payment-action-resolution-test", &tokens);
    let program = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )?;
    cursor.finish()?;
    Ok(program)
}

/// The resolution-cost child's own failure after `you may`. A sentence the
/// cost child rejects may still be an ordinary optional instruction.
fn resolution_cost_failure(text: &str) -> crate::oracle::canonical::MachineFailure {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("payment-action-resolution-cost-test", &tokens);
    cursor.consume_phrase(&["you", "may"]).unwrap();
    recognize_resolution_cost(&mut cursor).unwrap_err()
}

fn chosen(node: &PaymentActionNode) -> &ChosenPaymentObjectsNode {
    match node {
        PaymentActionNode::Tap(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Sacrifice(PaymentObjectsNode::Chosen(chosen))
        | PaymentActionNode::Discard(PaymentObjectsNode::Chosen(chosen)) => chosen,
        other => panic!("chosen payment objects, not {other:?}"),
    }
}

#[test]
fn the_same_payment_action_reads_the_same_node_in_activation_and_resolution() {
    for (cost, activation, resolution) in [
        (
            "tap two untapped creatures you control",
            "{1}, tap two untapped creatures you control: Draw a card.",
            "you may tap two untapped creatures you control. If you do, draw a card.",
        ),
        (
            "sacrifice a creature",
            "{1}, sacrifice a creature: Draw a card.",
            "you may sacrifice a creature. If you do, draw a card.",
        ),
        (
            "discard a card",
            "{1}, discard a card: Draw a card.",
            "you may discard a card. If you do, draw a card.",
        ),
        (
            "pay 2 life",
            "{1}, pay 2 life: Draw a card.",
            "you may pay 2 life. If you do, draw a card.",
        ),
    ] {
        let standalone = payment(cost).unwrap_or_else(|failure| panic!("{cost}: {failure:?}"));
        assert_eq!(
            payment_in_context(activation, 2),
            standalone,
            "{activation}"
        );
        assert_eq!(
            payment_in_context(resolution, 2),
            standalone,
            "{resolution}"
        );
    }
}

#[test]
fn each_context_projects_the_shared_tap_node_without_reinterpreting_it() {
    let node = payment("tap two untapped creatures you control").unwrap();
    let tokens = recognize_entities("");
    let mut cursor = OracleTokenCursor::new("payment-action-projection-test", &tokens);
    let LocalActivationCostNode::TapObjects(activation) =
        activation_cost_from_payment(&mut cursor, node.clone(), 1).unwrap()
    else {
        panic!("activation tap-objects cost");
    };
    let ResolutionCostNode::TapObjects(resolution) =
        resolution_cost_from_payment(&mut cursor, node).unwrap()
    else {
        panic!("resolution tap-objects cost");
    };
    assert_eq!(activation.tap, resolution);
    assert_eq!(activation.cost_index, 1);
}

#[test]
fn quantity_status_exclusion_and_query_compose_for_every_object_verb() {
    for (text, count, exclude_source) in [
        ("tap an untapped creature you control", 1, false),
        ("tap three untapped creatures you control", 3, false),
        ("tap two creatures you control", 2, false),
        ("tap another untapped creature you control", 1, true),
        ("tap two other untapped Goblins you control", 2, true),
        (
            "tap two untapped artifacts or creatures you control",
            2,
            false,
        ),
        (
            "tap two untapped creatures and/or artifacts you control",
            2,
            false,
        ),
        ("sacrifice a creature", 1, false),
        ("sacrifice another creature or artifact", 1, true),
        // A previously unused combination: a counted sacrifice.
        ("sacrifice two Goblins", 2, false),
        ("discard a card", 1, false),
        ("discard two creature cards", 2, false),
    ] {
        let node = payment(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        let chosen = chosen(&node);
        assert_eq!(
            (chosen.count, chosen.exclude_source),
            (NumericAtom::Integer(count), exclude_source),
            "{text}"
        );
    }
    // The quantity slot is the shared numeric atom: X is read, not bound.
    for text in [
        "sacrifice X Goats",
        "discard X cards",
        "tap X untapped artifacts you control",
    ] {
        let node = payment(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert_eq!(chosen(&node).count, NumericAtom::VariableX, "{text}");
    }
    assert!(
        chosen(&payment("discard a card").unwrap())
            .filter
            .is_any_card()
    );
    assert_eq!(
        chosen(&payment("discard a card at random").unwrap()).method,
        CollectionSelectionMethod::Random
    );
}

#[test]
fn source_references_keep_their_typed_noun_per_object_domain() {
    for (text, expected) in [
        (
            "{T}",
            PaymentActionNode::Tap(PaymentObjectsNode::Source(PaymentSourceNoun::TapSymbol)),
        ),
        (
            "sacrifice this token",
            PaymentActionNode::Sacrifice(PaymentObjectsNode::Source(PaymentSourceNoun::Permanent)),
        ),
        (
            "sacrifice this creature",
            PaymentActionNode::Sacrifice(PaymentObjectsNode::Source(PaymentSourceNoun::Permanent)),
        ),
        // The shared source reader names the source by any subtype noun.
        (
            "sacrifice this Aura",
            PaymentActionNode::Sacrifice(PaymentObjectsNode::Source(PaymentSourceNoun::Permanent)),
        ),
        // Exile takes the source permanent from the battlefield.
        (
            "exile this artifact",
            PaymentActionNode::Exile {
                objects: PaymentObjectsNode::Source(PaymentSourceNoun::Permanent),
                origin: None,
            },
        ),
        (
            "discard this card",
            PaymentActionNode::Discard(PaymentObjectsNode::Source(PaymentSourceNoun::Card)),
        ),
        (
            "exile this card from your graveyard",
            PaymentActionNode::Exile {
                objects: PaymentObjectsNode::Source(PaymentSourceNoun::Card),
                origin: Some(PaymentOriginZone::Graveyard),
            },
        ),
        ("blight 2", PaymentActionNode::Blight { amount: 2 }),
    ] {
        assert_eq!(payment(text).unwrap(), expected, "{text}");
    }
}

#[test]
fn shared_reader_rejects_targets_unknown_nouns_and_unscoped_exclusions() {
    for (text, state) in [
        // A cost selection is never a target (CR 601.2h).
        ("tap target creature you control", "cost.payment.quantity"),
        ("sacrifice target creature", "cost.payment.quantity"),
        ("sacrifice this widget", "reference.source.this-object"),
        ("sacrifice this instant", "cost.payment.source.noun"),
        ("discard this creature", "cost.payment.source.domain"),
        ("sacrifice this card", "cost.payment.source.domain"),
        (
            "exile this artifact from your graveyard",
            "cost.payment.exile.source-domain",
        ),
        // `it` resumes a source an earlier action of the same list named.
        ("sacrifice it", "numeric.atom.start"),
        // Without `you control` the payer-controlled scope is not stated.
        ("tap two untapped creatures", "cost.payment.tap.domain"),
        (
            "tap another untapped creature or an artifact you control",
            "cost.payment.branch-article",
        ),
        (
            "sacrifice a creature an opponent controls",
            "cost.payment.sacrifice.controller",
        ),
        (
            "sacrifice an instant",
            "cost.payment.sacrifice.permanent-domain",
        ),
        // Chosen cards to exile name the zone they come from.
        ("exile a creature card", "cost.payment.exile.origin"),
        (
            "exile this card from your library",
            "cost.payment.exile.origin-zone",
        ),
    ] {
        let failure = payment(text).unwrap_err();
        assert_eq!(failure.state, state, "{text}");
    }
}

#[test]
fn a_cost_sequence_reads_only_the_connectors_its_parent_admits() {
    let tokens = recognize_entities("pay {1} and sacrifice a creature");
    let mut cursor = OracleTokenCursor::new("payment-sequence-test", &tokens);
    let sequence = recognize_cost_sequence(
        &mut cursor,
        CostListGrammar {
            comma: true,
            and: true,
        },
    )
    .unwrap();
    cursor.finish().unwrap();
    assert_eq!(sequence.costs.len(), 2);
    assert_eq!(sequence.connectors, [CostConnector::And]);

    let mut cursor = OracleTokenCursor::new("payment-sequence-test", &tokens);
    let sequence = recognize_cost_sequence(
        &mut cursor,
        CostListGrammar {
            comma: true,
            and: false,
        },
    )
    .unwrap();
    assert_eq!(sequence.costs.len(), 1);
    assert_eq!(cursor.peek(), Some("and"));
}

#[test]
fn resolution_gains_atoms_first_read_in_other_cost_contexts() {
    // `sacrifice this PERMANENT` was an activation-only reader.
    let EffectProgramNode::Outcome {
        producer: OutcomeProducer::Cost(cost),
        ..
    } = resolution_program("you may sacrifice this artifact. If you do, draw two cards.").unwrap()
    else {
        panic!("optional cost program");
    };
    assert_eq!(cost, ResolutionCostNode::SacrificeSource);
    // The shared card selector replaces the `a card`-only resolution reader.
    let EffectProgramNode::Outcome {
        producer: OutcomeProducer::Cost(ResolutionCostNode::DiscardCard { filter }),
        ..
    } = resolution_program("you may discard a creature card. If you do, draw two cards.").unwrap()
    else {
        panic!("optional discard cost");
    };
    assert_eq!(filter, ObjectPredicate::CardType(ObjectCardType::Creature));
}

#[test]
fn resolution_gains_lower_to_the_engine_cost_program_schema() {
    for (text, cost) in [
        (
            "At the beginning of your upkeep, you may sacrifice this artifact. If you do, draw two cards.",
            serde_json::json!({ "kind": "sacrificePermanent", "permanent": { "kind": "self" } }),
        ),
        (
            "At the beginning of your upkeep, you may discard a creature card. If you do, draw two cards.",
            serde_json::json!({
                "kind": "discardCard",
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            }),
        ),
        (
            "At the beginning of your upkeep, you may discard a card. If you do, draw two cards.",
            serde_json::json!({ "kind": "discardCard", "where": null }),
        ),
    ] {
        let parsed = parse_oracle_card(OracleCardParseRequest {
            card_name: "Generic Payment Source".into(),
            type_line: "Artifact".into(),
            mana_cost: Some("{2}".into()),
            oracle_text: Some(text.into()),
            layout: Some("normal".into()),
            faces: Vec::new(),
        });
        assert_eq!(parsed.status, "canonical", "{text}: {:?}", parsed.abilities);
        let rule = serde_json::to_string(parsed.abilities[0].rule.as_ref().unwrap()).unwrap();
        let cost = serde_json::to_string(&cost).unwrap();
        assert!(rule.contains(&cost), "{text}: {rule}");
    }
}

#[test]
fn each_context_fails_explicitly_for_an_atom_without_its_engine_schema() {
    for (text, state) in [
        (
            "{1}, Untap a tapped creature you control",
            "engine.gap.activation-cost-action",
        ),
        // The activation announces X; X cards chosen at random still have
        // no engine schema (X chosen objects are a declared choice).
        (
            "{1}, Discard X cards at random",
            "engine.gap.activation-cost-action",
        ),
        (
            "{1}, Exile a card at random from your graveyard",
            "activation.exile-cards.selection-support",
        ),
        ("Tap this creature", "activation.tap-self.written"),
    ] {
        assert_eq!(activation_costs(text).unwrap_err().state, state, "{text}");
    }
    for (text, state) in [
        (
            "you may exile this card from your graveyard. If you do, draw a card.",
            "engine.gap.resolution-cost-action",
        ),
        (
            "you may sacrifice two creatures. If you do, draw a card.",
            "cost.resolution.sacrifice.quantity",
        ),
        // No announcement binds X in a resolution cost.
        (
            "you may sacrifice X creatures. If you do, draw a card.",
            "cost.resolution.sacrifice.quantity",
        ),
        (
            "you may pay {1} and pay 2 life. If you do, draw a card.",
            "cost.resolution.sequence.resources",
        ),
        (
            "you may discard a card at random. If you do, draw a card.",
            "cost.resolution.discard.selection-method",
        ),
        (
            "you may tap this creature. If you do, draw a card.",
            "cost.resolution.tap.source",
        ),
    ] {
        assert_eq!(resolution_cost_failure(text).state, state, "{text}");
    }
}
