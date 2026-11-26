//! The shared player-chosen cardinality slot: its own ranges, the cost
//! parent that pays a payer-chosen number, the activation projection that
//! declares it, and the resolution sacrifice that picks a member of a range.
use super::super::*;
use super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};

fn tokens(text: &str) -> Vec<crate::oracle::model::RecognizedEntity> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    tokens
}

fn range(text: &str) -> (Option<ChosenQuantityRange>, Option<String>) {
    let tokens = tokens(text);
    let mut cursor = OracleTokenCursor::new("quantity-slot-test", &tokens);
    let range = recognize_chosen_quantity_range(&mut cursor).expect("a well-formed slot");
    (range, cursor.peek().map(str::to_owned))
}

fn payment(text: &str) -> MachineResult<PaymentActionNode> {
    let tokens = tokens(text);
    let mut cursor = OracleTokenCursor::new("quantity-slot-payment-test", &tokens);
    let node = recognize_payment_action(&mut cursor)?;
    cursor.finish()?;
    Ok(node)
}

fn activation_costs(text: &str) -> MachineResult<ActivationCostSequenceNode> {
    let tokens = tokens(text);
    let mut cursor = OracleTokenCursor::new("quantity-slot-activation-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor)?;
    cursor.finish()?;
    Ok(costs)
}

fn sacrifice(text: &str) -> MachineResult<SacrificeEffectNode> {
    let tokens = tokens(text);
    let mut cursor = OracleTokenCursor::new("quantity-slot-sacrifice-test", &tokens);
    let node = recognize_ordinary_sacrifice_effect(&mut cursor, &[], None, None)?;
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(node)
}

const ANY: ChosenAmountBounds = ChosenAmountBounds {
    minimum: 0,
    maximum: None,
    exhaustive: false,
};

const ALL: ChosenAmountBounds = ChosenAmountBounds {
    minimum: 0,
    maximum: None,
    exhaustive: true,
};

#[test]
fn the_slot_reads_each_range_and_leaves_stated_counts_to_the_numeric_grammar() {
    assert_eq!(
        range("any number of artifacts"),
        (
            Some(ChosenQuantityRange::AnyNumber),
            Some("artifacts".into())
        )
    );
    assert_eq!(
        range("one or more creature cards"),
        (
            Some(ChosenQuantityRange::OneOrMore),
            Some("creature".into())
        )
    );
    assert_eq!(
        range("up to two creatures"),
        (
            Some(ChosenQuantityRange::UpTo(NumericExpression::from(2))),
            Some("creatures".into())
        )
    );
    assert_eq!(
        range("up to X target creatures").0,
        Some(ChosenQuantityRange::UpTo(NumericExpression::Atom(
            NumericAtom::VariableX
        )))
    );
    // Not a range: nothing is consumed and the parent reads its own count.
    for text in [
        "two creatures",
        "a creature",
        "one or more of the following",
        "up to",
    ] {
        assert_eq!(range(text).0, None, "{text}");
    }
    assert!(!ChosenQuantityRange::UpTo(NumericExpression::from(1)).is_plural());
    assert_eq!(ChosenQuantityRange::OneOrMore.minimum(), 1);
    assert_eq!(ChosenQuantityRange::AnyNumber.maximum(), None);
}

#[test]
fn a_cost_reads_a_number_the_payer_chooses_with_literal_bounds() {
    let PaymentActionNode::RemoveCounters(removal) =
        payment("Remove any number of storage counters from this land").unwrap()
    else {
        panic!("a counter removal");
    };
    assert_eq!(removal.count, CostQuantity::Chosen(ANY));
    assert_eq!(
        removal.counter.as_ref().map(|counter| counter.as_str()),
        Some("storage")
    );
    let PaymentActionNode::Exile {
        objects: PaymentObjectsNode::Chosen(chosen),
        ..
    } = payment("Exile one or more creature cards from your graveyard").unwrap()
    else {
        panic!("a chosen exile");
    };
    assert_eq!(
        chosen.count,
        CostQuantity::Chosen(ChosenAmountBounds {
            minimum: 1,
            maximum: None,
            exhaustive: false,
        })
    );
    let PaymentActionNode::Sacrifice(PaymentObjectsNode::Chosen(chosen)) =
        payment("Sacrifice up to three permanents").unwrap()
    else {
        panic!("a chosen sacrifice");
    };
    assert_eq!(
        chosen.count,
        CostQuantity::Chosen(ChosenAmountBounds {
            minimum: 0,
            maximum: Some(3),
            exhaustive: false,
        })
    );
    // An exact count keeps the shared atom.
    let PaymentActionNode::RemoveCounters(exact) =
        payment("Remove two charge counters from this artifact").unwrap()
    else {
        panic!("a counter removal");
    };
    assert_eq!(exact.count, NumericAtom::Integer(2));
    // Near-miss: `up to X` names two numbers the payer would pick.
    assert_eq!(
        payment("Sacrifice up to X creatures").unwrap_err().state,
        "quantity-slot.cost-bound"
    );
}

#[test]
fn an_activation_declares_one_payer_chosen_number_beside_its_announced_x() {
    let storage =
        activation_costs("{T}, Remove any number of storage counters from this land").unwrap();
    assert_eq!(storage.cost_chosen_amount(), Some(ANY));
    assert!(!storage.declares_x());
    assert_eq!(
        storage.cost_chosen_amount_slot(),
        Some(CostChosenAmountSlot {
            verb: CostAmountVerb::Removed,
            counter: CounterName::from_parts(["storage"]),
            predicate: None,
        })
    );
    // An announced X and a payer-chosen number are two producers.
    let both =
        activation_costs("{X}, {T}, Remove any number of charge counters from this artifact")
            .unwrap();
    assert!(both.declares_x());
    assert_eq!(both.cost_chosen_amount(), Some(ANY));
    // Chosen objects keep the shared chosen-objects schema.
    let exile = activation_costs("{T}, Exile any number of cards from your graveyard").unwrap();
    assert!(matches!(
        exile.costs.as_slice(),
        [
            LocalActivationCostNode::TapSelf(_),
            LocalActivationCostNode::ChosenX(ChosenXCostNode {
                action: ChosenXAction::Exile,
                amount: ChosenObjectsAmount::PayerChosen(ANY),
                ..
            })
        ]
    ));
    assert_eq!(
        exile.cost_chosen_amount_slot().map(|slot| slot.verb),
        Some(CostAmountVerb::Exiled)
    );
    // Near-misses: unnamed counters of several kinds, and two payer-chosen
    // numbers a restatement could not tell apart.
    assert_eq!(
        activation_costs("{T}, Remove any number of counters from this artifact")
            .unwrap_err()
            .state,
        "activation.remove-counters.chosen-kind"
    );
    assert_eq!(
        activation_costs(
            "Remove any number of storage counters from this land, Exile any number of cards from your graveyard"
        )
        .unwrap_err()
        .state,
        "activation.cost-chosen-amount.single"
    );
}

#[test]
fn a_resolution_sacrifice_picks_a_member_of_a_range() {
    for (text, expected) in [
        (
            "sacrifice any number of artifacts.",
            ChosenQuantityRange::AnyNumber,
        ),
        (
            "sacrifice one or more lands.",
            ChosenQuantityRange::OneOrMore,
        ),
        (
            "sacrifice up to two creatures.",
            ChosenQuantityRange::UpTo(NumericExpression::from(2)),
        ),
    ] {
        let SacrificeEffectNode::Selected { count, .. } = sacrifice(text).unwrap() else {
            panic!("a selected sacrifice: {text}");
        };
        assert_eq!(count, SelectionCount::Range(expected), "{text}");
    }
    // A stated count is unchanged, and compares with the shared expression.
    let SacrificeEffectNode::Selected { count, .. } =
        sacrifice("sacrifice two artifacts.").unwrap()
    else {
        panic!("a selected sacrifice");
    };
    assert_eq!(count, NumericExpression::from(2));
    assert!(sacrifice("sacrifice another creature.").is_err());
}

#[test]
fn a_counter_cost_reads_the_universal_determiner_as_the_number_the_holder_fixes() {
    // `all KIND counters`: every counter of that kind, fixed when paid.
    let PaymentActionNode::RemoveCounters(removal) =
        payment("Remove all charge counters from this artifact").unwrap()
    else {
        panic!("a counter removal");
    };
    assert_eq!(removal.count, CostQuantity::Chosen(ALL));
    assert!(removal.count.is_plural());
    // It is the activation's one recorded number: `that much` and
    // `counters removed this way` restate it.
    let relic =
        activation_costs("{2}, {T}, Remove all charge counters from this artifact").unwrap();
    assert_eq!(relic.cost_chosen_amount(), Some(ALL));
    assert!(!relic.declares_x());
    assert_eq!(
        relic.cost_chosen_amount_slot(),
        Some(CostChosenAmountSlot {
            verb: CostAmountVerb::Removed,
            counter: CounterName::from_parts(["charge"]),
            predicate: None,
        })
    );
    // Sibling: a named source and the +1/+1 kind compose unchanged.
    let hydra = activation_costs("{T}, Remove all +1/+1 counters from this creature").unwrap();
    assert_eq!(hydra.cost_chosen_amount(), Some(ALL));
    // Near-misses: counters of every kind are not one removal; `all` of an
    // object query is not a counter quantity; a singular noun disagrees.
    assert_eq!(
        activation_costs("{T}, Remove all counters from this artifact")
            .unwrap_err()
            .state,
        "activation.remove-counters.chosen-kind"
    );
    assert!(payment("Sacrifice all creatures you control").is_err());
    assert_eq!(
        payment("Remove all charge counter from this artifact")
            .unwrap_err()
            .state,
        "cost.payment.remove.noun"
    );
}
