use super::super::*;
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> MachineResult<AttackTaxModifierNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("attack-tax-test", &tokens);
    recognize_attack_tax_modifier(&mut cursor)
}

#[test]
fn fixed_tax_retains_player_payer_amount_and_multiplicity() {
    let node = parse("Creatures can't attack you unless their controller pays {2} for each creature they control that's attacking you.")
        .expect("attack tax parses");

    assert_eq!(node.attackers, ObjectCardType::Creature);
    assert_eq!(node.protected_player, LocalPlayerReference::Controller);
    assert_eq!(node.payer, AttackTaxPayerNode::AttackingCreatureController);
    assert_eq!(
        node.amount,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
}

#[test]
fn fixed_amount_is_a_reusable_numeric_slot() {
    let node = parse("Creatures can't attack you unless their controller pays {3} for each creature they control that's attacking you.")
        .expect("sibling amount parses");

    assert_eq!(
        node.amount,
        NumericExpression::Atom(NumericAtom::Integer(3))
    );
}

#[test]
fn unsupported_protected_subject_fails_before_payment() {
    let failure = parse("Creatures can't attack target opponent unless their controller pays {2} for each creature they control that's attacking target opponent.")
        .expect_err("unsupported protected subject fails");

    assert_eq!(failure.state, "modifier.attack-tax.protected-player");
    assert_eq!(failure.actual.as_deref(), Some("target"));
}
