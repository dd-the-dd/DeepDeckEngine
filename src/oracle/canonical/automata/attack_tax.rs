use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, NumericAtom, NumericExpression, ObjectCardType,
    recognize_player_reference,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttackTaxPayerNode {
    AttackingCreatureController,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AttackTaxModifierNode {
    pub attackers: ObjectCardType,
    pub protected_player: LocalPlayerReference,
    pub payer: AttackTaxPayerNode,
    pub amount: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttackTaxState {
    Attackers,
    Prohibition,
    Attack,
    ProtectedPlayer,
    Exception,
    PayerReference,
    PayerRole,
    PaymentAction,
    Payment,
    PerAttacker,
    AttackerType,
    AttackerReference,
    Control,
    RelativeReference,
    AttackingState,
    RepeatedProtectedPlayer,
    Terminator,
    Complete,
}

fn recognize_generic_mana_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("modifier.attack-tax.payment");
    let amount = cursor
        .peek_token()
        .filter(|token| token.token_type == "manaSymbol")
        .and_then(|token| token.raw.strip_prefix('{'))
        .and_then(|raw| raw.strip_suffix('}'))
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|amount| (0..=i64::from(i32::MAX)).contains(amount));
    cursor.require(
        amount.is_some(),
        "one nonnegative generic mana symbol within the runtime integer range",
    )?;
    cursor.consume_type("manaSymbol")?;
    Ok(NumericExpression::Atom(NumericAtom::Integer(
        amount.expect("validated generic amount"),
    )))
}

/// Recognizes the static restriction
/// `Creatures can't attack PLAYER unless their controller pays {N} for each
/// creature they control that's attacking PLAYER.`
///
/// The protected player, payer relation, numeric amount, and per-attacker
/// multiplicity remain independent typed slots. The current runtime boundary
/// intentionally accepts only the ability controller as protected player.
pub(in crate::oracle::canonical) fn recognize_attack_tax_modifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AttackTaxModifierNode> {
    let mut state = AttackTaxState::Attackers;
    let mut protected_player = None;
    let mut amount = None;
    loop {
        state = match state {
            AttackTaxState::Attackers => {
                cursor.enter("modifier.attack-tax.attackers");
                let token = cursor.consume_type("typeWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("creatures"),
                    "the creature attacker domain",
                )?;
                AttackTaxState::Prohibition
            }
            AttackTaxState::Prohibition => {
                cursor.enter("modifier.attack-tax.prohibition");
                let token = cursor.consume_type("logicWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("can't")
                        || token.raw.eq_ignore_ascii_case("can’t"),
                    "the `can't` attack prohibition",
                )?;
                AttackTaxState::Attack
            }
            AttackTaxState::Attack => {
                cursor.enter("modifier.attack-tax.attack");
                let token = cursor.consume_type("actionWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("attack"),
                    "the `attack` action",
                )?;
                AttackTaxState::ProtectedPlayer
            }
            AttackTaxState::ProtectedPlayer => {
                cursor.enter("modifier.attack-tax.protected-player");
                cursor.require(
                    cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("you")),
                    "the ability controller as protected player",
                )?;
                let player = recognize_player_reference(cursor, None)?;
                cursor.require(
                    player == LocalPlayerReference::Controller,
                    "the ability controller as protected player",
                )?;
                protected_player = Some(player);
                AttackTaxState::Exception
            }
            AttackTaxState::Exception => {
                cursor.enter("modifier.attack-tax.exception");
                cursor.consume("unless")?;
                AttackTaxState::PayerReference
            }
            AttackTaxState::PayerReference => {
                cursor.enter("modifier.attack-tax.payer-reference");
                cursor.consume("their")?;
                AttackTaxState::PayerRole
            }
            AttackTaxState::PayerRole => {
                cursor.enter("modifier.attack-tax.payer-role");
                let token = cursor.consume_type("playerRoleWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("controller"),
                    "the attacking creature's controller",
                )?;
                AttackTaxState::PaymentAction
            }
            AttackTaxState::PaymentAction => {
                cursor.enter("modifier.attack-tax.payment-action");
                let token = cursor.consume_type("costActionWord")?;
                cursor.require(
                    is_verb_form(&token.raw, VerbLemma::Pay, VerbForm::ThirdPersonSingular),
                    "the `pays` action",
                )?;
                AttackTaxState::Payment
            }
            AttackTaxState::Payment => {
                amount = Some(recognize_generic_mana_amount(cursor)?);
                AttackTaxState::PerAttacker
            }
            AttackTaxState::PerAttacker => {
                cursor.enter("modifier.attack-tax.per-attacker");
                cursor.consume_phrase(&["for", "each"])?;
                AttackTaxState::AttackerType
            }
            AttackTaxState::AttackerType => {
                cursor.enter("modifier.attack-tax.attacker-type");
                let token = cursor.consume_type("typeWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("creature"),
                    "one attacking creature",
                )?;
                AttackTaxState::AttackerReference
            }
            AttackTaxState::AttackerReference => {
                cursor.enter("modifier.attack-tax.attacker-reference");
                cursor.consume("they")?;
                AttackTaxState::Control
            }
            AttackTaxState::Control => {
                cursor.enter("modifier.attack-tax.control");
                cursor.consume("control")?;
                AttackTaxState::RelativeReference
            }
            AttackTaxState::RelativeReference => {
                cursor.enter("modifier.attack-tax.relative-reference");
                let token = cursor.consume_type("referenceWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("that's")
                        || token.raw.eq_ignore_ascii_case("that’s"),
                    "the relative attacking-creature reference",
                )?;
                AttackTaxState::AttackingState
            }
            AttackTaxState::AttackingState => {
                cursor.enter("modifier.attack-tax.attacking-state");
                let token = cursor.consume_type("combatStateWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("attacking"),
                    "the attacking state",
                )?;
                AttackTaxState::RepeatedProtectedPlayer
            }
            AttackTaxState::RepeatedProtectedPlayer => {
                cursor.enter("modifier.attack-tax.repeated-protected-player");
                let repeated = recognize_player_reference(cursor, None)?;
                cursor.require(
                    Some(repeated) == protected_player,
                    "the same protected player reference",
                )?;
                AttackTaxState::Terminator
            }
            AttackTaxState::Terminator => {
                cursor.enter("modifier.attack-tax.terminator");
                cursor.consume(".")?;
                AttackTaxState::Complete
            }
            AttackTaxState::Complete => {
                cursor.enter("modifier.attack-tax.complete");
                cursor.finish()?;
                return Ok(AttackTaxModifierNode {
                    attackers: ObjectCardType::Creature,
                    protected_player: cursor
                        .require_value(protected_player, "a protected player reference")?,
                    payer: AttackTaxPayerNode::AttackingCreatureController,
                    amount: cursor.require_value(amount, "a generic mana amount")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "attack_tax_tests.rs"]
mod tests;
