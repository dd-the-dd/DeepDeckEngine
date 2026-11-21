use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AlternativeManaCostEnvelopeNode, CompoundCostNode, CostConnector, CostSequenceNode,
    CurrentSpellManaCostReferenceNode, ExileQualifiedCardFromHandCostNode, NumericAtom,
    PayLifeCostNode, recognize_controlled_permanent_presence_condition,
    recognize_not_your_turn_condition, recognize_numeric_atom,
};
use crate::oracle::characteristic_vocabulary::lookup_card_color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PayLifeCostState {
    Action,
    Amount,
    AmountSupported,
    Resource,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_pay_life_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PayLifeCostNode> {
    recognize_pay_life_cost_with_verb(cursor, super::ActingSubject::CONTROLLER)
}

/// `pay N life` with the lexicon verb `pay` in the payer's agreement (the
/// shared clause, resource-payment frame) and the life amount child.
pub(in crate::oracle::canonical) fn recognize_pay_life_cost_with_verb(
    cursor: &mut OracleTokenCursor<'_>,
    payer: super::ActingSubject,
) -> MachineResult<PayLifeCostNode> {
    let mut state = PayLifeCostState::Action;
    let mut amount = None;
    let mut amount_token = None;
    loop {
        state = match state {
            PayLifeCostState::Action => {
                cursor.enter("cost.pay-life.action");
                let verb = super::recognize_operand_verb(
                    cursor,
                    payer.agreement,
                    &[super::VerbLemma::Pay],
                    &[super::ArgumentFrame::ResourcePayment],
                )?;
                cursor.require(
                    verb.is_some(),
                    &format!(
                        "`{}`",
                        super::verb_entry(super::VerbLemma::Pay).surface(payer.agreement.form())
                    ),
                )?;
                PayLifeCostState::Amount
            }
            PayLifeCostState::Amount => {
                cursor.enter("cost.pay-life.amount");
                amount_token = cursor.peek_token().cloned();
                amount = Some(recognize_numeric_atom(cursor)?);
                PayLifeCostState::AmountSupported
            }
            PayLifeCostState::AmountSupported => {
                cursor.enter("cost.pay-life.amount-supported");
                match amount {
                    Some(NumericAtom::Integer(_)) => PayLifeCostState::Resource,
                    Some(NumericAtom::VariableX) => {
                        let token = amount_token
                            .as_ref()
                            .ok_or_else(|| cursor.fail("a fixed life payment"))?;
                        return Err(cursor.fail_with_span(
                            "a fixed integer life payment",
                            token.index,
                            token.span_start,
                            token.span_end,
                            token.raw.clone(),
                        ));
                    }
                    None => return Err(cursor.fail("a populated life payment")),
                }
            }
            PayLifeCostState::Resource => {
                cursor.enter("cost.pay-life.resource");
                cursor.consume("life")?;
                PayLifeCostState::Complete
            }
            PayLifeCostState::Complete => {
                cursor.enter("cost.pay-life.complete");
                let NumericAtom::Integer(amount) =
                    cursor.require_value(amount, "a populated life payment")?
                else {
                    return Err(cursor.fail("a fixed integer life payment"));
                };
                return Ok(PayLifeCostNode { amount });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExileQualifiedCardCostState {
    Action,
    Article,
    Color,
    CardNoun,
    SourceRelation,
    Possessor,
    Zone,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_exile_qualified_card_from_hand_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ExileQualifiedCardFromHandCostNode> {
    let mut state = ExileQualifiedCardCostState::Action;
    let mut color = None;
    loop {
        state = match state {
            ExileQualifiedCardCostState::Action => {
                cursor.enter("cost.exile-card.action");
                cursor.consume("exile")?;
                ExileQualifiedCardCostState::Article
            }
            ExileQualifiedCardCostState::Article => {
                cursor.enter("cost.exile-card.article");
                cursor.require(
                    cursor.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                    }),
                    "`a` or `an`",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                ExileQualifiedCardCostState::Color
            }
            ExileQualifiedCardCostState::Color => {
                cursor.enter("cost.exile-card.selector.color");
                let token = cursor.consume_type("colorWord")?;
                color = Some(
                    lookup_card_color(&token.raw)
                        .ok_or_else(|| cursor.fail("a supported Magic color"))?,
                );
                ExileQualifiedCardCostState::CardNoun
            }
            ExileQualifiedCardCostState::CardNoun => {
                cursor.enter("cost.exile-card.selector.card");
                cursor.consume("card")?;
                ExileQualifiedCardCostState::SourceRelation
            }
            ExileQualifiedCardCostState::SourceRelation => {
                cursor.enter("cost.exile-card.source-relation");
                cursor.consume("from")?;
                ExileQualifiedCardCostState::Possessor
            }
            ExileQualifiedCardCostState::Possessor => {
                cursor.enter("cost.exile-card.possessor");
                cursor.consume("your")?;
                ExileQualifiedCardCostState::Zone
            }
            ExileQualifiedCardCostState::Zone => {
                cursor.enter("cost.exile-card.zone");
                let zone = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("the `hand` zone"))?;
                cursor.require(zone.eq_ignore_ascii_case("hand"), "`hand`")?;
                cursor.consume_type("zoneWord")?;
                ExileQualifiedCardCostState::Complete
            }
            ExileQualifiedCardCostState::Complete => {
                cursor.enter("cost.exile-card.complete");
                return Ok(ExileQualifiedCardFromHandCostNode {
                    color: cursor.require_value(color, "a populated card color")?,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_compound_alternative_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CostSequenceNode<CompoundCostNode>> {
    cursor.enter("cost.sequence.first");
    let mut costs = Vec::new();
    let mut connectors = Vec::new();
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("pay"))
    {
        costs.push(CompoundCostNode::PayLife(recognize_pay_life_cost(cursor)?));
        cursor.enter("cost.sequence.connector");
        cursor.consume("and")?;
        connectors.push(CostConnector::And);
        cursor.enter("cost.sequence.second");
    }
    costs.push(CompoundCostNode::ExileQualifiedCardFromHand(
        recognize_exile_qualified_card_from_hand_cost(cursor)?,
    ));
    cursor.enter("cost.sequence.complete");
    Ok(CostSequenceNode { costs, connectors })
}

pub(in crate::oracle::canonical) fn recognize_current_spell_mana_cost_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CurrentSpellManaCostReferenceNode> {
    cursor.enter("cost-reference.current-spell.this");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("its"))
    {
        let reference = cursor.consume_type("referenceWord")?;
        cursor.require(
            reference.raw.eq_ignore_ascii_case("its"),
            "the pronominal current spell reference `its`",
        )?;
    } else {
        cursor.consume("this")?;
        cursor.enter("cost-reference.current-spell.object");
        let object = cursor.consume_type("possessiveTypeWord")?;
        cursor.require(
            object.raw.eq_ignore_ascii_case("spell's")
                || object.raw.eq_ignore_ascii_case("spell’s"),
            "the possessive current spell reference",
        )?;
    }
    cursor.enter("cost-reference.current-spell.mana");
    let mana = cursor.consume_type("costWord")?;
    cursor.require(mana.raw.eq_ignore_ascii_case("mana"), "`mana`")?;
    cursor.enter("cost-reference.current-spell.cost");
    let cost = cursor.consume_type("costWord")?;
    cursor.require(cost.raw.eq_ignore_ascii_case("cost"), "`cost`")?;
    cursor.enter("cost-reference.current-spell.complete");
    Ok(CurrentSpellManaCostReferenceNode)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionalFreeCastState {
    Condition,
    Player,
    Permission,
    Cast,
    CurrentSpell,
    Omission,
    Payment,
    ReplacedCost,
    Terminator,
    Complete,
}

/// Recognizes the generic conditional zero-mana alternative-cost envelope:
/// `If CONDITION, you may cast this spell without paying its mana cost.`
/// The condition and the replaced-cost reference remain independent children;
/// an empty cost sequence represents the valid zero-cost alternative.
pub(in crate::oracle::canonical) fn recognize_conditional_free_cast_envelope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AlternativeManaCostEnvelopeNode> {
    let mut state = ConditionalFreeCastState::Condition;
    let mut condition = None;
    let mut replaced_cost = None;
    loop {
        state = match state {
            ConditionalFreeCastState::Condition => {
                cursor.enter("alternative-mana-cost.free-cast.condition");
                condition = Some(recognize_controlled_permanent_presence_condition(cursor)?);
                ConditionalFreeCastState::Player
            }
            ConditionalFreeCastState::Player => {
                cursor.enter("alternative-mana-cost.free-cast.player");
                cursor.consume("you")?;
                ConditionalFreeCastState::Permission
            }
            ConditionalFreeCastState::Permission => {
                cursor.enter("alternative-mana-cost.free-cast.permission");
                let permission = cursor.consume_type("modalityWord")?;
                cursor.require(permission.raw.eq_ignore_ascii_case("may"), "`may`")?;
                ConditionalFreeCastState::Cast
            }
            ConditionalFreeCastState::Cast => {
                cursor.enter("alternative-mana-cost.free-cast.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(action.raw.eq_ignore_ascii_case("cast"), "`cast`")?;
                ConditionalFreeCastState::CurrentSpell
            }
            ConditionalFreeCastState::CurrentSpell => {
                cursor.enter("alternative-mana-cost.free-cast.current-spell");
                cursor.consume("this")?;
                let object = cursor.consume_type("typeWord")?;
                cursor.require(object.raw.eq_ignore_ascii_case("spell"), "`spell`")?;
                ConditionalFreeCastState::Omission
            }
            ConditionalFreeCastState::Omission => {
                cursor.enter("alternative-mana-cost.free-cast.omission");
                let omission = cursor.consume_type("costOmissionWord")?;
                cursor.require(omission.raw.eq_ignore_ascii_case("without"), "`without`")?;
                ConditionalFreeCastState::Payment
            }
            ConditionalFreeCastState::Payment => {
                cursor.enter("alternative-mana-cost.free-cast.payment");
                let payment = cursor.consume_type("costActionWord")?;
                cursor.require(payment.raw.eq_ignore_ascii_case("paying"), "`paying`")?;
                ConditionalFreeCastState::ReplacedCost
            }
            ConditionalFreeCastState::ReplacedCost => {
                cursor.enter("alternative-mana-cost.free-cast.replaced-cost");
                replaced_cost = Some(recognize_current_spell_mana_cost_reference(cursor)?);
                ConditionalFreeCastState::Terminator
            }
            ConditionalFreeCastState::Terminator => {
                cursor.enter("alternative-mana-cost.free-cast.terminator");
                cursor.consume(".")?;
                ConditionalFreeCastState::Complete
            }
            ConditionalFreeCastState::Complete => {
                cursor.enter("alternative-mana-cost.free-cast.complete");
                cursor.finish()?;
                return Ok(AlternativeManaCostEnvelopeNode {
                    condition,
                    costs: CostSequenceNode {
                        costs: Vec::new(),
                        connectors: Vec::new(),
                    },
                    replaces: cursor
                        .require_value(replaced_cost, "the current spell's mana cost")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlternativeManaCostEnvelopeState {
    ConditionOrSubject,
    Subject,
    Permission,
    Costs,
    Substitution,
    ReplacedCost,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_alternative_mana_cost_envelope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AlternativeManaCostEnvelopeNode> {
    let mut state = AlternativeManaCostEnvelopeState::ConditionOrSubject;
    let mut condition = None;
    let mut costs = None;
    let mut replaced_cost = None;
    loop {
        state = match state {
            AlternativeManaCostEnvelopeState::ConditionOrSubject => {
                cursor.enter("alternative-mana-cost.condition-or-subject");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
                {
                    condition = Some(recognize_not_your_turn_condition(cursor)?);
                }
                AlternativeManaCostEnvelopeState::Subject
            }
            AlternativeManaCostEnvelopeState::Subject => {
                cursor.enter("alternative-mana-cost.subject");
                cursor.consume("you")?;
                AlternativeManaCostEnvelopeState::Permission
            }
            AlternativeManaCostEnvelopeState::Permission => {
                cursor.enter("alternative-mana-cost.permission");
                let permission = cursor.consume_type("modalityWord")?;
                cursor.require(permission.raw.eq_ignore_ascii_case("may"), "`may`")?;
                AlternativeManaCostEnvelopeState::Costs
            }
            AlternativeManaCostEnvelopeState::Costs => {
                cursor.enter("alternative-mana-cost.costs");
                costs = Some(recognize_compound_alternative_cost_sequence(cursor)?);
                AlternativeManaCostEnvelopeState::Substitution
            }
            AlternativeManaCostEnvelopeState::Substitution => {
                cursor.enter("alternative-mana-cost.substitution");
                cursor.consume_phrase(&["rather", "than"])?;
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(action.raw.eq_ignore_ascii_case("pay"), "`pay`")?;
                AlternativeManaCostEnvelopeState::ReplacedCost
            }
            AlternativeManaCostEnvelopeState::ReplacedCost => {
                cursor.enter("alternative-mana-cost.replaced-cost");
                replaced_cost = Some(recognize_current_spell_mana_cost_reference(cursor)?);
                AlternativeManaCostEnvelopeState::Terminator
            }
            AlternativeManaCostEnvelopeState::Terminator => {
                cursor.enter("alternative-mana-cost.terminator");
                cursor.consume(".")?;
                AlternativeManaCostEnvelopeState::Complete
            }
            AlternativeManaCostEnvelopeState::Complete => {
                cursor.enter("alternative-mana-cost.complete");
                cursor.finish()?;
                return Ok(AlternativeManaCostEnvelopeNode {
                    condition,
                    costs: cursor.require_value(costs, "a populated alternative cost sequence")?,
                    replaces: cursor
                        .require_value(replaced_cost, "the current spell's mana cost")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "alternative_cost_tests.rs"]
mod tests;
