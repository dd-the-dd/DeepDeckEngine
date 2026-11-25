use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AlternativeManaCostEnvelopeNode, CompoundCostNode, CostConnector, CostSequenceNode,
    CurrentSpellManaCostReferenceNode, ExileQualifiedCardFromHandCostNode, NumericAtom,
    PayLifeCostNode, recognize_controlled_permanent_presence_condition,
    recognize_not_your_turn_condition, recognize_numeric_atom,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
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
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Exile,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
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
    if crate::oracle::canonical::automata::lemma_at(
        &cursor,
        VerbLemma::Pay,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    ) {
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
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Pay, VerbForm::Base),
                    "`pay`",
                )?;
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

// ---------------------------------------------------------------------------
// The alternative-cast permission sentence (CR 118.9, 601.2b)
//
// ```text
// Sentence := [ "If" COND "," ] "you" "may" Permission [ "if" COND ] "."
// Permission := COSTS "rather" "than" "pay" CurrentSpellManaCost
//             | "cast" "this" "spell" "without" "paying" "its" "mana" "cost"
// COSTS    := the shared activation-cost sequence (`, ` / `and`)
// COND     := the one condition language, as a state evaluated when the
//             spell is cast
// ```
//
// One parent for every printed alternative cost: the payments are the
// shared cost children, the condition the shared Boolean atom program, and
// the zero-cost permission an absent cost sequence.

/// A printed alternative-cast permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AlternativeCastSentenceNode {
    pub condition: Option<super::ConditionNode>,
    /// The payments instead of the mana cost; `None` for `without paying
    /// its mana cost`.
    pub costs: Option<super::ActivationCostSequenceNode>,
}

const RATHER_THAN_PAY: &[&str] = &["rather", "than", "pay"];
const WITHOUT_PAYING: &[&str] = &[
    "cast", "this", "spell", "without", "paying", "its", "mana", "cost",
];

fn words_at(
    tokens: &[crate::oracle::model::RecognizedEntity],
    index: usize,
    words: &[&str],
) -> bool {
    words.iter().enumerate().all(|(offset, word)| {
        tokens
            .get(index + offset)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(word))
    })
}

/// Structural claim: `you may ... rather than pay this spell's mana cost`
/// or `you may cast this spell without paying its mana cost`, opening the
/// ability or after an `If CONDITION,` introducer.
pub(in crate::oracle::canonical) fn alternative_cast_sentence_starts(
    tokens: &[crate::oracle::model::RecognizedEntity],
) -> bool {
    let Some(permission) =
        (0..tokens.len()).find(|index| words_at(tokens, *index, &["you", "may"]))
    else {
        return false;
    };
    let introduced = permission == 0
        || (words_at(tokens, 0, &["if"])
            && tokens
                .get(permission.wrapping_sub(1))
                .is_some_and(|token| token.raw == ","));
    introduced
        && (words_at(tokens, permission + 2, WITHOUT_PAYING)
            || (permission + 2..tokens.len()).any(|index| {
                words_at(tokens, index, RATHER_THAN_PAY)
                    && tokens.get(index + 3).is_some_and(|token| {
                        token.raw.eq_ignore_ascii_case("this")
                            || token.raw.eq_ignore_ascii_case("its")
                    })
            }))
}

fn first_root_index(
    cursor: &OracleTokenCursor<'_>,
    matches: impl Fn(&[crate::oracle::model::RecognizedEntity], usize) -> bool,
) -> Option<usize> {
    let tokens = cursor.token_slice(0, usize::MAX);
    (cursor.position()..tokens.len()).find(|index| matches(tokens, *index))
}

fn recognize_bounded_condition(
    cursor: &mut OracleTokenCursor<'_>,
    end: usize,
    context: &super::ConditionContext<'_>,
) -> MachineResult<super::ConditionNode> {
    cursor.bounded(end, |cursor| {
        super::recognize_boolean_condition_list(cursor, |cursor| {
            super::recognize_condition_atom(cursor, context)
        })
    })
}

/// Reads one alternative-cast permission sentence.
pub(in crate::oracle::canonical) fn recognize_alternative_cast_sentence(
    cursor: &mut OracleTokenCursor<'_>,
    context: &super::ConditionContext<'_>,
) -> MachineResult<AlternativeCastSentenceNode> {
    let mut condition = None;
    cursor.enter("alternative-cast.condition");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
    {
        cursor.consume("if")?;
        let comma = first_root_index(cursor, |tokens, index| tokens[index].raw == ",");
        let comma = cursor.require_value(comma, "the comma closing the `If` condition")?;
        condition = Some(recognize_bounded_condition(cursor, comma, context)?);
        cursor.enter("alternative-cast.condition-separator");
        cursor.consume(",")?;
    }
    cursor.enter("alternative-cast.permission");
    cursor.consume_phrase(&["you", "may"])?;
    let costs = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("cast"))
    {
        cursor.enter("alternative-cast.without-paying");
        cursor.consume_phrase(WITHOUT_PAYING)?;
        None
    } else {
        cursor.enter("alternative-cast.costs");
        let rather = first_root_index(cursor, |tokens, index| {
            words_at(tokens, index, RATHER_THAN_PAY)
        });
        let rather = cursor.require_value(rather, "`rather than pay` after the payments")?;
        let costs = cursor.bounded(rather, super::recognize_activation_cost_sequence)?;
        cursor.enter("alternative-cast.substitution");
        cursor.consume_phrase(&["rather", "than"])?;
        let action = cursor.consume_type("costActionWord")?;
        cursor.require(
            is_verb_form(&action.raw, VerbLemma::Pay, VerbForm::Base),
            "`pay`",
        )?;
        cursor.enter("alternative-cast.replaced-cost");
        recognize_current_spell_mana_cost_reference(cursor)?;
        Some(costs)
    };
    cursor.enter("alternative-cast.postposed-condition");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
    {
        cursor.require(condition.is_none(), "one condition on the permission")?;
        cursor.consume("if")?;
        let period = first_root_index(cursor, |tokens, index| tokens[index].raw == ".");
        let period = cursor.require_value(period, "the period closing the permission")?;
        condition = Some(recognize_bounded_condition(cursor, period, context)?);
    }
    cursor.enter("alternative-cast.terminator");
    cursor.consume(".")?;
    cursor.enter("alternative-cast.complete");
    cursor.finish()?;
    Ok(AlternativeCastSentenceNode { condition, costs })
}

#[path = "casting_keyword.rs"]
mod casting_keyword;

pub(in crate::oracle::canonical) use casting_keyword::{
    CastingKeyword, CastingKeywordNode, GiftInstruction, GiftTiming, KeywordCostNode,
    casting_keyword_starts, recognize_casting_keyword,
};

#[cfg(test)]
#[path = "alternative_cost_tests.rs"]
mod tests;
