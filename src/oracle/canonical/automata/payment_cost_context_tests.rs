//! Step 2 of the cost unification: counter removal, chosen-card exile, source
//! exclusion and resolution cost lists, each read once by the shared payment
//! reader and projected by every cost context whose engine schema pays it.
use super::super::*;
use super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};
use serde_json::{Value, json};

fn payment(text: &str) -> MachineResult<PaymentActionNode> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("payment-context-test", &tokens);
    let node = recognize_payment_action(&mut cursor)?;
    cursor.finish()?;
    Ok(node)
}

fn activation_costs(text: &str) -> MachineResult<ActivationCostSequenceNode> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("payment-context-activation-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor)?;
    cursor.finish()?;
    Ok(costs)
}

fn resolution_cost(text: &str) -> MachineResult<ResolutionCostNode> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("payment-context-resolution-test", &tokens);
    let cost = recognize_resolution_cost(&mut cursor)?;
    cursor.finish()?;
    Ok(cost)
}

fn card_rule(type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Payment Source".into(),
        type_line: type_line.into(),
        mana_cost: Some("{2}".into()),
        oracle_text: Some(text.into()),
        layout: Some("normal".into()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}: {:?}", parsed.abilities);
    parsed.abilities[0].rule.clone().expect("canonical rule")
}

fn removal(node: PaymentActionNode) -> RemoveCountersPaymentNode {
    match node {
        PaymentActionNode::RemoveCounters(removal) => removal,
        other => panic!("a counter removal, not {other:?}"),
    }
}

#[test]
fn remove_reads_quantity_optional_kind_and_three_holder_forms() {
    let typed = removal(payment("Remove two charge counters from this artifact").unwrap());
    assert_eq!(typed.count, NumericAtom::Integer(2));
    assert_eq!(typed.counter.as_ref().map(|c| c.as_str()), Some("charge"));
    assert!(matches!(typed.holder, CounterHolderNode::Source(_)));

    let untyped = removal(payment("Remove a counter from this creature").unwrap());
    assert_eq!(
        (untyped.count, untyped.counter),
        (CostQuantity::Exactly(NumericAtom::Integer(1)), None)
    );

    let one = removal(payment("Remove a +1/+1 counter from a creature you control").unwrap());
    assert_eq!(one.counter.as_ref().map(|c| c.as_str()), Some("+1/+1"));
    assert!(matches!(one.holder, CounterHolderNode::One(_)));

    let among =
        removal(payment("Remove two +1/+1 counters from among artifacts you control").unwrap());
    assert_eq!(among.count, NumericAtom::Integer(2));
    assert!(matches!(among.holder, CounterHolderNode::Among(_)));
}

#[test]
fn remove_reader_near_misses_fail_at_their_own_states() {
    for (text, state) in [
        (
            // `any number of` is a payer-chosen number (`quantity_slot.rs`);
            // a variable upper bound names two numbers and is refused.
            "Remove up to X +1/+1 counters from this creature",
            "quantity-slot.cost-bound",
        ),
        // An unscoped holder names any permanent, not one the payer controls.
        (
            "Remove a counter from a creature",
            "cost.payment.remove.holder-domain",
        ),
        // A cost never targets (CR 601.2h).
        (
            "Remove a counter from target creature",
            "cost.payment.remove.holder",
        ),
        // Number agreement belongs to the counter noun.
        (
            "Remove two counter from this creature",
            "cost.payment.remove.noun",
        ),
    ] {
        assert_eq!(payment(text).unwrap_err().state, state, "{text}");
    }
}

#[test]
fn counter_removal_projects_onto_activation_and_resolution_schemas() {
    let costs = activation_costs(
        "{2}, Remove a +1/+1 counter from a creature you control, Remove two charge counters from this artifact, Remove a counter from this artifact",
    )
    .unwrap();
    let [
        LocalActivationCostNode::PayMana(_),
        LocalActivationCostNode::RemoveControlledCounters(controlled),
        LocalActivationCostNode::RemoveSourceCounters(typed),
        LocalActivationCostNode::RemoveSourceCounters(untyped),
    ] = costs.costs.as_slice()
    else {
        panic!("one typed activation cost per removal: {costs:?}");
    };
    assert_eq!(
        (
            controlled.counter.as_ref().map(|c| c.as_str()),
            controlled.count
        ),
        (Some("+1/+1"), 1)
    );
    assert_eq!(typed.count, NumericAtom::Integer(2));
    assert_eq!((untyped.counter.clone(), untyped.cost_index), (None, 3));

    // Several unnamed counters are a distribution: each may be of any kind.
    for (text, source) in [
        ("{1}, Remove two counters from this creature", true),
        (
            "{1}, Remove three counters from among creatures you control",
            false,
        ),
    ] {
        let costs = activation_costs(text).unwrap();
        let LocalActivationCostNode::RemoveControlledCounters(distribution) = &costs.costs[1]
        else {
            panic!("{text}: a counter distribution");
        };
        assert_eq!(distribution.counter, None, "{text}");
        assert_eq!(
            matches!(distribution.holder, CounterDistributionHolder::Source),
            source,
            "{text}"
        );
    }

    assert!(matches!(
        resolution_cost("remove two charge counters from this artifact").unwrap(),
        ResolutionCostNode::RemoveSourceCounters {
            count: 2,
            counter: Some(_)
        }
    ));
    assert!(matches!(
        resolution_cost("remove a counter from this creature").unwrap(),
        ResolutionCostNode::RemoveSourceCounters {
            count: 1,
            counter: None
        }
    ));
}

#[test]
fn counter_removal_without_an_exact_schema_fails_explicitly() {
    for (text, state) in [
        // The engine declares one distribution per activation.
        (
            "{1}, Remove two counters from this creature, Remove a +1/+1 counter from a creature you control",
            "activation.remove-counters.single-distribution",
        ),
        (
            "{1}, Remove two +1/+1 counters from a creature you control",
            "activation.remove-counters.one-holder-quantity",
        ),
        // A graveyard activation cannot remove counters from a permanent source.
        (
            "{1}, Remove a counter from this creature, Exile this card from your graveyard",
            "activation.remove-counters.source-zone",
        ),
    ] {
        assert_eq!(activation_costs(text).unwrap_err().state, state, "{text}");
    }
    for (text, state) in [
        (
            "remove a +1/+1 counter from a creature you control",
            "cost.resolution.remove-counters.holder",
        ),
        (
            "remove two counters from this creature",
            "cost.resolution.remove-counters.unnamed-quantity",
        ),
    ] {
        assert_eq!(resolution_cost(text).unwrap_err().state, state, "{text}");
    }
}

#[test]
fn chosen_card_exile_and_source_exclusion_project_in_both_contexts() {
    let costs = activation_costs(
        "{4}{B}, Exile another creature card from your graveyard, Sacrifice another artifact",
    )
    .unwrap();
    let [
        LocalActivationCostNode::PayMana(_),
        LocalActivationCostNode::ExileCards(exile),
        LocalActivationCostNode::SacrificeObject(sacrifice),
    ] = costs.costs.as_slice()
    else {
        panic!("exile and sacrifice choices: {costs:?}");
    };
    assert_eq!(
        (exile.count, exile.zone, exile.exclude_source),
        (1, PaymentOriginZone::Graveyard, true)
    );
    assert!(sacrifice.exclude_source);

    assert!(matches!(
        resolution_cost("exile another creature card from your graveyard").unwrap(),
        ResolutionCostNode::ExileCard {
            zone: PaymentOriginZone::Graveyard,
            exclude_source: true,
            ..
        }
    ));
    assert_eq!(
        resolution_cost("exile two cards from your graveyard")
            .unwrap_err()
            .state,
        "cost.resolution.exile.quantity"
    );
}

#[test]
fn a_resolution_cost_list_is_one_sequence_of_distinct_resources() {
    let ResolutionCostNode::Sequence(parts) =
        resolution_cost("pay {1} and exile a card from your graveyard").unwrap()
    else {
        panic!("a cost list is a sequence");
    };
    assert!(matches!(
        parts.as_slice(),
        [
            ResolutionCostNode::Payment(ResourcePaymentNode::Mana(_)),
            ResolutionCostNode::ExileCard { .. }
        ]
    ));
    // Previously unused combination: a discard and a graveyard exile.
    assert!(matches!(
        resolution_cost("discard a card and exile a creature card from your graveyard").unwrap(),
        ResolutionCostNode::Sequence(_)
    ));
    for text in [
        "pay {1} and sacrifice a creature",
        "exile a card from your graveyard and exile a creature card from your graveyard",
    ] {
        assert_eq!(
            resolution_cost(text).unwrap_err().state,
            "cost.resolution.sequence.resources",
            "{text}"
        );
    }
}

#[test]
fn complete_activations_lower_every_new_atom_to_engine_costs() {
    let rule = card_rule(
        "Artifact Creature - Construct",
        "{1}, Sacrifice another artifact: Put a +1/+1 counter on this creature.",
    );
    let choice = &rule["declaration"]["decisions"][0];
    assert_eq!(choice["kind"], "chooseObjects");
    assert_eq!(choice["candidates"]["excludeSource"], true);
    assert_eq!(rule["costs"][1]["permanent"]["id"], choice["id"]);

    let rule = card_rule(
        "Creature - Merfolk",
        "{1}, {T}, Tap another untapped creature you control: Tap target creature an opponent controls.",
    );
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["excludeSource"],
        true
    );

    let rule = card_rule(
        "Artifact",
        "{T}, Remove two charge counters from this artifact: Draw a card.",
    );
    assert_eq!(
        rule["costs"][1],
        json!({
            "kind": "removeCounters",
            "permanent": { "kind": "self" },
            "counter": "charge",
            "count": { "kind": "integer", "value": 2 },
        })
    );

    let rule = card_rule(
        "Creature - Faerie",
        "{1}{W}, Remove a counter from this creature: Draw a card.",
    );
    assert_eq!(rule["costs"][1]["counter"]["kind"], "decisionResult");

    let rule = card_rule(
        "Creature - Turtle",
        "{2}, Remove a +1/+1 counter from a creature you control: Draw a card.",
    );
    assert_eq!(
        rule["costs"][1]["kind"],
        "removeCountersFromControlledPermanents"
    );
    assert_eq!(rule["costs"][1]["counter"], "+1/+1");

    let rule = card_rule(
        "Creature - Spirit",
        "{4}{B}, Exile another creature card from your graveyard: Draw a card.",
    );
    let choice = &rule["declaration"]["decisions"][0];
    assert_eq!(choice["candidates"]["kind"], "cards");
    assert_eq!(choice["candidates"]["excludeSource"], true);
    assert_eq!(rule["costs"][1]["kind"], "move");
    assert_eq!(rule["costs"][1]["objects"]["id"], choice["id"]);
}

#[test]
fn previously_unused_combination_of_new_atoms_needs_no_recipe() {
    let rule = card_rule(
        "Creature - Horror",
        "Sacrifice another creature, Remove a counter from this creature, Exile two cards from your graveyard: Draw a card.",
    );
    let kinds = rule["costs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cost| cost["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["sacrificePermanent", "removeCounters", "move"]);
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[1]["quantity"]["value"]["value"], 2);
}

#[test]
fn complete_resolution_costs_lower_to_cost_programs() {
    let rule = card_rule(
        "Creature - Human",
        "At the beginning of combat on your turn, you may pay {1} and exile a card from your graveyard. If you do, draw a card.",
    );
    let text = serde_json::to_string(&rule).unwrap();
    assert!(text.contains(r#""kind":"costSequence""#), "{text}");
    assert!(text.contains(r#""kind":"exileCard""#), "{text}");

    let rule = card_rule(
        "Creature - Faerie",
        "At the beginning of your upkeep, you may remove a counter from this creature. If you do, draw a card.",
    );
    let text = serde_json::to_string(&rule).unwrap();
    assert!(text.contains(r#""counter":"any""#), "{text}");
}
