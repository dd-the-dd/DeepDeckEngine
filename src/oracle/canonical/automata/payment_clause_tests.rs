//! The cost frame is a client of the shared clause: any lexicon lemma whose
//! frame a cost reads is a payment action in the payer's agreement. Real
//! cards per lemma, a third-person payer, and frames with no cost reader or
//! no engine cost schema that must stay explicit failures.
use super::super::*;
use super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

fn payment_by(text: &str, payer: ActingSubject) -> MachineResult<PaymentActionNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("payment-clause-test", &tokens);
    let node = recognize_conjugated_payment_action(&mut cursor, payer)?;
    cursor.finish()?;
    Ok(node)
}

fn status(name: &str, type_line: &str, text: &str) -> (String, Option<String>) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let state = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "canonical")
        .and_then(|ability| ability.diagnostics.first())
        .and_then(|diagnostic| diagnostic.state.clone());
    (parsed.status, state)
}

#[test]
fn every_cost_lemma_reads_its_operand_frame() {
    let me = ActingSubject::CONTROLLER;
    assert!(matches!(
        payment_by("Return a land you control to its owner's hand", me),
        Ok(PaymentActionNode::ReturnToHand(PaymentObjectsNode::Chosen(
            _
        )))
    ));
    assert!(matches!(
        payment_by("Return this enchantment to its owner's hand", me),
        Ok(PaymentActionNode::ReturnToHand(PaymentObjectsNode::Source(
            _
        )))
    ));
    assert!(matches!(
        payment_by("Put a -1/-1 counter on this creature", me),
        Ok(PaymentActionNode::PutCounters(PutCountersPaymentNode {
            count: 1,
            holder: CounterHolderNode::Source(_),
            ..
        }))
    ));
    assert!(matches!(
        payment_by("Put two +1/+1 counters on a creature you control", me),
        Ok(PaymentActionNode::PutCounters(PutCountersPaymentNode {
            count: 2,
            holder: CounterHolderNode::One(_),
            ..
        }))
    ));
    assert!(matches!(
        payment_by("Mill two cards", me),
        Ok(PaymentActionNode::Mill { count: 2 })
    ));
    assert!(matches!(
        payment_by("Exert this creature", me),
        Ok(PaymentActionNode::Exert(PaymentObjectsNode::Source(_)))
    ));
    assert!(matches!(
        payment_by("Reveal this card from your hand", me),
        Ok(PaymentActionNode::Reveal {
            origin: Some(PaymentOriginZone::Hand),
            ..
        })
    ));
    assert!(matches!(
        payment_by("Untap a tapped land you control", me),
        Ok(PaymentActionNode::Untap(_))
    ));
}

#[test]
fn the_payer_agreement_comes_from_the_lexicon() {
    // Unused combination: a third-person payer and the new mill lemma.
    assert!(matches!(
        payment_by("mills two cards", ActingSubject::THIRD_SINGULAR),
        Ok(PaymentActionNode::Mill { count: 2 })
    ));
    assert!(matches!(
        payment_by("pays 3 life", ActingSubject::THIRD_SINGULAR),
        Ok(PaymentActionNode::Pay(ResourcePaymentNode::Life(_)))
    ));
    for (text, payer) in [
        (
            "Returns a land you control to its owner's hand",
            ActingSubject::CONTROLLER,
        ),
        ("mill a card", ActingSubject::THIRD_SINGULAR),
    ] {
        assert_eq!(
            payment_by(text, payer).unwrap_err().state,
            "clause.verb",
            "{text}"
        );
    }
    // `put` into a zone is the lemma's other frame: claimed, no cost reader.
    assert_eq!(
        payment_by(
            "Put a card from your hand on top of your library",
            ActingSubject::CONTROLLER
        )
        .unwrap_err()
        .state,
        "cost.payment.frame"
    );
}

#[test]
fn activation_costs_wire_engine_schemas_or_fail_explicitly() {
    for (name, type_line, text) in [
        (
            "Quirion Ranger",
            "Creature — Elf",
            "Return a Forest you control to its owner's hand: Untap target creature. Activate only once each turn.",
        ),
        (
            "Attunement",
            "Enchantment",
            "Return this enchantment to its owner's hand: Draw three cards, then discard four cards.",
        ),
        (
            "Devoted Druid",
            "Creature — Elf Druid",
            "Put a -1/-1 counter on this creature: Untap this creature.",
        ),
        (
            "Hatchet Bully",
            "Creature — Goblin Warrior",
            "{2}{R}, {T}, Put a -1/-1 counter on a creature you control: This creature deals 2 damage to any target.",
        ),
        (
            "Soratami Seer Probe",
            "Creature — Moonfolk",
            "{4}, Return two lands you control to their owner's hand: Draw a card.",
        ),
    ] {
        assert_eq!(status(name, type_line, text).0, "canonical", "{text}");
    }
    for (type_line, text) in [
        (
            "Artifact Creature — Construct",
            "{T}, Mill a card: Add {C}.",
        ),
        (
            "Creature — Cat",
            "{W}, {T}, Exert this creature: Create a 1/1 white Cat creature token with lifelink.",
        ),
        (
            "Creature — Merfolk",
            "{T}, Untap a tapped land you control: Draw a card.",
        ),
    ] {
        assert_eq!(
            status("Unsupported Cost Probe", type_line, text),
            (
                "unsupported".to_string(),
                Some("engine.gap.activation-cost-action".to_string())
            ),
            "{text}"
        );
    }
}
