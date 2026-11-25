//! Keyword actions of CR 701 read as ordinary verbs of the shared effect
//! grammar. Subjects and objects come from the shared permanent operand; the
//! numeric argument from the shared numeric expression. Each action keeps its
//! identity until lowering, where it expands into ordinary primitives.
//!
//! Random and contested outcomes are controller actions too, each with the
//! argument frame of its lexicon row (`verb_lexicon.rs`):
//!
//! ```text
//! CoinFlip := "flip" COUNT ("coin" | "coins")                         CR 705
//! DieRoll  := "roll" COUNT ("d"K | K"-sided" ("die" | "dice"))         CR 706
//! Clash    := "clash" "with" "an" "opponent"                          CR 701.30
//! ```
//!
//! A performer subject (`each player flips a coin`) reaches the same frame
//! through the performer's person shift. The outcome is a definite
//! antecedent of the rest of the program: `the flip`, `the result` and a
//! bare `you win` name the latest outcome of their kind
//! (`RandomResultQuantity`), read by the shared condition comparison and the
//! shared numeric definition grammar.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    ActionResultQuantity, BooleanConditionNode, ConditionAtom, ConditionComparison, ConditionNode,
    ConditionNumber, ConditionPlayerSubject, ConditionScope, NumericComparisonOperator, VerbForm,
    VerbLemma, is_verb_form, lookup_verb, recognize_numeric_atom,
};
use super::{
    ControllerKeywordAction, ControllerKeywordActionNode, EffectRecognitionContext,
    ExplicitActivationSourceAvailability, KeywordQuantityPolicy, LocalObjectReference, NumericAtom,
    NumericExpression, ObjectCardType, ObjectPredicate, PermanentKeywordAction,
    PermanentKeywordActionNode, PermanentOperandNode, PutCountersOnObjectEffectNode, TargetBinding,
    TargetCardinality, TargetListNode, TargetSelector, TargetSlot,
    recognize_contextual_permanent_operand, recognize_numeric_expression,
};

/// Reads the numeric argument of a keyword action according to its policy.
/// A negative literal is never a count of repetitions or counters.
fn recognize_keyword_quantity(
    cursor: &mut OracleTokenCursor<'_>,
    action: PermanentKeywordAction,
    antecedent: Option<TargetBinding>,
) -> MachineResult<Option<NumericExpression>> {
    let quantity = match action.quantity_policy() {
        KeywordQuantityPolicy::None => return Ok(None),
        KeywordQuantityPolicy::Optional => {
            // An omitted argument is the unparameterized action; the next
            // token then belongs to the enclosing sequence.
            let mut trial = cursor.clone();
            match recognize_numeric_expression(&mut trial, antecedent) {
                Ok(quantity) => {
                    *cursor = trial;
                    quantity
                }
                Err(_) => return Ok(None),
            }
        }
        KeywordQuantityPolicy::Required => recognize_numeric_expression(cursor, antecedent)?,
    };
    cursor.require(
        !matches!(
            &quantity,
            NumericExpression::Atom(NumericAtom::Integer(value)) if *value < 0
        ),
        "a nonnegative keyword-action quantity",
    )?;
    Ok(Some(quantity))
}

/// `SUBJECT explores` / `SUBJECT connives [N]` / `SUBJECT endures N`: the
/// parent has already read the subject with the shared permanent operand and
/// positioned the cursor on the verb.
pub(in crate::oracle::canonical) fn recognize_subject_keyword_action(
    cursor: &mut OracleTokenCursor<'_>,
    subject: PermanentOperandNode,
    antecedent: Option<TargetBinding>,
) -> MachineResult<PermanentKeywordActionNode> {
    cursor.enter("effect.keyword-action.subject-verb");
    let verb = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a third-person keyword-action verb"))?
        .to_owned();
    let action = PermanentKeywordAction::from_subject_verb(&verb)
        .ok_or_else(|| cursor.fail("a third-person keyword-action verb"))?;
    cursor.consume(&verb)?;
    cursor.enter("effect.keyword-action.quantity");
    let quantity = recognize_keyword_quantity(cursor, action, antecedent)?;
    Ok(PermanentKeywordActionNode {
        action,
        targets: subject,
        quantity,
    })
}

/// `suspect OBJECT` (CR 701.60a) / `detain OBJECT` (CR 701.35a).
pub(in crate::oracle::canonical) fn recognize_imperative_permanent_keyword_action(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentKeywordActionNode> {
    cursor.enter("effect.keyword-action.imperative-verb");
    let verb = cursor
        .peek()
        .ok_or_else(|| cursor.fail("an imperative keyword-action verb"))?
        .to_owned();
    let action = PermanentKeywordAction::from_imperative_object_verb(&verb)
        .ok_or_else(|| cursor.fail("an imperative keyword-action verb"))?;
    let mut trial = cursor.clone();
    trial.consume(&verb)?;
    cursor.enter("effect.keyword-action.object");
    let targets =
        recognize_contextual_permanent_operand(&mut trial, name_links, name_failure, context)?;
    *cursor = trial;
    Ok(PermanentKeywordActionNode {
        action,
        targets,
        quantity: None,
    })
}

/// A permanent's own ability can name `this permanent` implicitly. Spells
/// and abilities without a proven battlefield source cannot.
fn permanent_source_available(context: &EffectRecognitionContext) -> bool {
    context.source.is_some()
        || context.activation_source_availability
            == ExplicitActivationSourceAvailability::RetainedBattlefieldPermanent
}

/// `adapt N` (CR 701.46a) / `monstrosity N` (CR 701.37a): both are defined
/// over `this permanent`, the source of the ability.
pub(in crate::oracle::canonical) fn recognize_self_quantity_keyword_action(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentKeywordActionNode> {
    cursor.enter("effect.keyword-action.self-verb");
    let verb = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a self-referential keyword-action verb"))?
        .to_owned();
    let action = PermanentKeywordAction::from_self_quantity_verb(&verb)
        .ok_or_else(|| cursor.fail("a self-referential keyword-action verb"))?;
    cursor.consume(&verb)?;
    cursor.enter("effect.keyword-action.self-source");
    cursor.require(
        permanent_source_available(context),
        "a permanent source for the self-referential keyword action",
    )?;
    cursor.enter("effect.keyword-action.quantity");
    let quantity = recognize_keyword_quantity(cursor, action, antecedent)?;
    Ok(PermanentKeywordActionNode {
        action,
        targets: PermanentOperandNode::Reference(LocalObjectReference::SourceSelf),
        quantity,
    })
}

/// `populate` (CR 701.36a) / `bolster N` (CR 701.39a) / `discover N`
/// (CR 701.57a) / `learn` (CR 701.48a).
pub(in crate::oracle::canonical) fn recognize_controller_keyword_action(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<ControllerKeywordActionNode> {
    cursor.enter("effect.keyword-action.controller-verb");
    let verb = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a controller keyword-action verb"))?
        .to_owned();
    let action = ControllerKeywordAction::from_imperative_verb(&verb)
        .ok_or_else(|| cursor.fail("a controller keyword-action verb"))?;
    cursor.consume(&verb)?;
    // Random and contested outcomes (CR 705, 706, 701.30) read their own
    // argument frame: the coins, the dice or the opponent.
    match action {
        ControllerKeywordAction::FlipCoins => return recognize_coin_flip_arguments(cursor),
        ControllerKeywordAction::RollDice(_) => return recognize_die_roll_arguments(cursor),
        ControllerKeywordAction::Clash => return recognize_clash_arguments(cursor),
        _ => {}
    }
    if action == ControllerKeywordAction::Manifest {
        cursor.enter("effect.keyword-action.manifest.object");
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("dread"))
        {
            cursor.consume("dread")?;
            return Ok(ControllerKeywordActionNode {
                action: ControllerKeywordAction::ManifestDread,
                quantity: None,
            });
        }
        // The cards come from the shared library-top operand; its owner is
        // the manifesting controller.
        let cards = match super::try_recognize_card_library_top(cursor) {
            super::CollectionOperandAttempt::Recognized(fragment) => fragment.node,
            super::CollectionOperandAttempt::FailedCommitted(failure)
            | super::CollectionOperandAttempt::RejectedCandidate(failure) => return Err(failure),
            super::CollectionOperandAttempt::NotClaimed => {
                return Err(cursor.fail("`dread` or the top cards of your library"));
            }
        };
        return Ok(ControllerKeywordActionNode {
            action,
            quantity: Some(cards.count),
        });
    }
    let quantity = if action.takes_quantity() {
        cursor.enter("effect.keyword-action.quantity");
        let quantity = recognize_numeric_expression(cursor, antecedent)?;
        cursor.require(
            !matches!(
                &quantity,
                NumericExpression::Atom(NumericAtom::Integer(value)) if *value < 0
            ),
            "a nonnegative keyword-action quantity",
        )?;
        Some(quantity)
    } else {
        None
    };
    Ok(ControllerKeywordActionNode { action, quantity })
}

/// The number of faces of each rolled die (CR 706.1). The claim of the verb
/// (`ControllerKeywordAction::from_imperative_verb`) does not read the die:
/// it carries `UNREAD`, which the argument frame replaces and the lowering
/// refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DieSides(pub u16);

impl DieSides {
    /// The die before the argument frame read it.
    pub(in crate::oracle::canonical) const UNREAD: Self = Self(0);
}

/// A quantity of the latest random or contested outcome of the program
/// (CR 705, 706, 701.30). The written phrase is a definite description: `the
/// flip`, `the result` and a bare `you win` after a clash denote the latest
/// outcome of that kind the resolving program produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RandomResultQuantity {
    /// `the result`, `the total of those results` (CR 706.2): the result of
    /// the latest die roll, the total when several dice were rolled.
    DieResult,
    /// The flips of the latest coin flip the flipping player won (CR 705.2).
    CoinFlipsWon,
    /// The flips of the latest coin flip the flipping player lost.
    CoinFlipsLost,
    /// The coins of the latest flip that came up heads (CR 705.3).
    CoinsHeads,
    /// The coins of the latest flip that came up tails.
    CoinsTails,
    /// 1 when the acting player won the latest clash, else 0 (CR 701.30d).
    ClashWon,
}

/// The controller action a lexicon verb claims: the random and contested
/// outcomes (`flip`, `roll`, `clash`), whose arguments are read by the
/// action's own frame, and `recruit` (CR 701.70a), which takes none.
pub(in crate::oracle::canonical) fn lexical_controller_action(
    raw: &str,
) -> Option<ControllerKeywordAction> {
    if is_verb_form(raw, VerbLemma::Recruit, VerbForm::Base) {
        Some(ControllerKeywordAction::Recruit)
    } else if is_verb_form(raw, VerbLemma::Flip, VerbForm::Base) {
        Some(ControllerKeywordAction::FlipCoins)
    } else if is_verb_form(raw, VerbLemma::Roll, VerbForm::Base) {
        Some(ControllerKeywordAction::RollDice(DieSides::UNREAD))
    } else if is_verb_form(raw, VerbLemma::Clash, VerbForm::Base) {
        Some(ControllerKeywordAction::Clash)
    } else {
        None
    }
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The written number of coins or dice: a positive literal or `X`, and
/// whether it is singular (the noun that follows agrees with it).
fn recognize_outcome_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(NumericExpression, bool)> {
    let atom = recognize_numeric_atom(cursor)?;
    match atom {
        NumericAtom::Integer(value) if value > 0 => Ok((NumericExpression::Atom(atom), value == 1)),
        NumericAtom::VariableX => Ok((NumericExpression::Atom(atom), false)),
        NumericAtom::Integer(_) => Err(cursor.fail("a positive number of coins or dice")),
    }
}

/// `flip a coin` / `flip N coins` (CR 705.1). `flip it` / `flip this
/// creature` (the Kamigawa flip of a flip card, CR 712) is no coin flip: its
/// operand is not a count and fails here.
fn recognize_coin_flip_arguments(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControllerKeywordActionNode> {
    cursor.enter("effect.random-outcome.coin.count");
    let (count, singular) = recognize_outcome_count(cursor)?;
    cursor.enter("effect.random-outcome.coin.noun");
    cursor.consume(if singular { "coin" } else { "coins" })?;
    Ok(ControllerKeywordActionNode {
        action: ControllerKeywordAction::FlipCoins,
        quantity: Some(count),
    })
}

/// The faces of a die token: `d20` (CR 706.1a), else `None`.
fn polyhedral_die_sides(raw: &str) -> Option<u16> {
    let digits = raw.strip_prefix('d').or_else(|| raw.strip_prefix('D'))?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u16>().ok()
}

/// The faces of a `K-sided` adjective (`six-sided`), else `None`.
fn sided_die_sides(raw: &str) -> Option<u16> {
    let lower = raw.to_ascii_lowercase();
    let faces = lower.strip_suffix("-sided")?;
    parse_number_word(faces).and_then(|faces| u16::try_from(faces).ok())
}

/// `roll a d20` / `roll two d6` / `roll a six-sided die` / `roll X six-sided
/// dice` (CR 706.1). A die has at least two faces; anything else after the
/// count (`roll the planar die`, CR 901.9) fails here.
fn recognize_die_roll_arguments(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControllerKeywordActionNode> {
    cursor.enter("effect.random-outcome.die.count");
    let (count, singular) = recognize_outcome_count(cursor)?;
    cursor.enter("effect.random-outcome.die.faces");
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a die: `dN` or `N-sided die`"))?
        .to_owned();
    let sides = if let Some(sides) = polyhedral_die_sides(&raw) {
        cursor.consume(&raw)?;
        sides
    } else if let Some(sides) = sided_die_sides(&raw) {
        cursor.consume(&raw)?;
        cursor.enter("effect.random-outcome.die.noun");
        cursor.consume(if singular { "die" } else { "dice" })?;
        sides
    } else {
        return Err(cursor.fail("a die: `dN` or `N-sided die`"));
    };
    cursor.require(sides >= 2, "a die with at least two faces (CR 706.1)")?;
    Ok(ControllerKeywordActionNode {
        action: ControllerKeywordAction::RollDice(DieSides(sides)),
        quantity: Some(count),
    })
}

/// `clash with an opponent` (CR 701.30b): the clashing player chooses the
/// opponent while the action resolves. A named opponent (`with defending
/// player`) has no frame yet and fails here.
fn recognize_clash_arguments(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControllerKeywordActionNode> {
    cursor.enter("effect.random-outcome.clash.opponent");
    cursor.consume_phrase(&["with", "an", "opponent"])?;
    Ok(ControllerKeywordActionNode {
        action: ControllerKeywordAction::Clash,
        quantity: None,
    })
}

/// The program position reading an outcome condition, when the program
/// installed one. The lowering reads the outcome's definite binding
/// (`RandomResultQuantity`), never this index.
fn outcome_reader(cursor: &OracleTokenCursor<'_>) -> usize {
    cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .unwrap_or_default()
}

fn outcome_quantity(
    cursor: &OracleTokenCursor<'_>,
    quantity: RandomResultQuantity,
) -> NumericExpression {
    NumericExpression::ActionResultQuantity {
        reader_effect: outcome_reader(cursor),
        quantity: ActionResultQuantity::RandomResult(quantity),
    }
}

/// The written shape of an outcome condition at the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutcomeConditionShape {
    /// `you|they win|lose|won|lost the flip`.
    Flip,
    /// `it comes up heads|tails`, `both coins come up heads|tails`.
    Face,
    /// `you roll|rolled [a] N [or higher|lower]`.
    Roll,
    /// `you win` before the condition's comma: the latest clash.
    Clash,
}

fn outcome_condition_shape(cursor: &OracleTokenCursor<'_>) -> Option<OutcomeConditionShape> {
    let lemma_at = |offset: usize, lemmas: &[VerbLemma]| {
        cursor
            .peek_ahead(offset)
            .and_then(|raw| lookup_verb(raw, lemmas))
            .is_some_and(|(_, forms)| {
                forms.contains(&VerbForm::Base) || forms.contains(&VerbForm::Past)
            })
    };
    let subject = word_at(cursor, 0, "you") || word_at(cursor, 0, "they");
    if subject
        && lemma_at(1, &[VerbLemma::Win, VerbLemma::Lose])
        && word_at(cursor, 2, "the")
        && word_at(cursor, 3, "flip")
    {
        return Some(OutcomeConditionShape::Flip);
    }
    if (word_at(cursor, 0, "it") && word_at(cursor, 1, "comes") && word_at(cursor, 2, "up"))
        || (word_at(cursor, 0, "both")
            && word_at(cursor, 1, "coins")
            && word_at(cursor, 2, "come")
            && word_at(cursor, 3, "up"))
    {
        return Some(OutcomeConditionShape::Face);
    }
    // `you rolled a die this turn` is the event history of rolling, not an
    // outcome: only a written face (`you roll a 6`) is claimed here.
    let article = usize::from(word_at(cursor, 2, "a") || word_at(cursor, 2, "an"));
    if word_at(cursor, 0, "you")
        && lemma_at(1, &[VerbLemma::Roll])
        && cursor
            .peek_ahead(2 + article)
            .and_then(parse_number_word)
            .is_some_and(|face| face > 0)
    {
        return Some(OutcomeConditionShape::Roll);
    }
    if word_at(cursor, 0, "you") && lemma_at(1, &[VerbLemma::Win]) && word_at(cursor, 2, ",") {
        return Some(OutcomeConditionShape::Clash);
    }
    None
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.advance_to(cursor.position() + 1)
}

/// The die face written after `roll`: `a 1`, `12`, `a 6 or higher`.
fn recognize_rolled_threshold(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(NumericComparisonOperator, i64)> {
    if word_at(cursor, 0, "a") || word_at(cursor, 0, "an") {
        consume_current(cursor)?;
    }
    cursor.enter("condition.random-outcome.roll.face");
    let face = cursor
        .peek()
        .and_then(parse_number_word)
        .filter(|face| *face > 0)
        .ok_or_else(|| cursor.fail("the die face rolled"))?;
    consume_current(cursor)?;
    cursor.enter("condition.random-outcome.roll.bound");
    if !word_at(cursor, 0, "or") {
        return Ok((NumericComparisonOperator::Equal, face));
    }
    let operator = match cursor.peek_ahead(1).map(str::to_ascii_lowercase).as_deref() {
        Some("higher" | "more" | "greater") => NumericComparisonOperator::GreaterThanOrEqual,
        Some("lower" | "less") => NumericComparisonOperator::LessThanOrEqual,
        // `If you rolled 7 or 11`: a disjunction of faces belongs to the
        // Boolean list's `or`, not to this bound.
        _ => return Ok((NumericComparisonOperator::Equal, face)),
    };
    consume_current(cursor)?;
    consume_current(cursor)?;
    Ok((operator, face))
}

/// An outcome condition of a random or contested action at an effect
/// program position: `you win|lose the flip` (CR 705.2), `it comes up
/// heads` (CR 705.3), `you roll a 6 [or higher]` (CR 706.2) and `you win`
/// after a clash (CR 701.30d). It is the shared numeric comparison over the
/// outcome's quantity, so `Otherwise` and the Boolean connectives compose
/// with it unchanged. `Ok(None)` when no outcome condition starts here; once
/// claimed, any other scope than the program's is an explicit failure.
pub(in crate::oracle::canonical) fn try_recognize_random_outcome_condition(
    cursor: &mut OracleTokenCursor<'_>,
    scope: ConditionScope,
) -> MachineResult<Option<ConditionAtom>> {
    let Some(shape) = outcome_condition_shape(cursor) else {
        return Ok(None);
    };
    cursor.enter("condition.random-outcome");
    cursor.require(
        scope == ConditionScope::Program,
        "the effect program whose random or contested action the condition reads",
    )?;
    let greater_than_zero = (NumericComparisonOperator::GreaterThan, 0);
    let (quantity, (operator, right)) = match shape {
        OutcomeConditionShape::Flip => {
            consume_current(cursor)?;
            cursor.enter("condition.random-outcome.flip.verb");
            let won = cursor
                .peek()
                .and_then(|raw| lookup_verb(raw, &[VerbLemma::Win]))
                .is_some();
            consume_current(cursor)?;
            cursor.consume_phrase(&["the", "flip"])?;
            let quantity = if won {
                RandomResultQuantity::CoinFlipsWon
            } else {
                RandomResultQuantity::CoinFlipsLost
            };
            (quantity, greater_than_zero)
        }
        OutcomeConditionShape::Face => {
            let both = word_at(cursor, 0, "both");
            if both {
                cursor.consume_phrase(&["both", "coins", "come", "up"])?;
            } else {
                cursor.consume_phrase(&["it", "comes", "up"])?;
            }
            cursor.enter("condition.random-outcome.face");
            let quantity = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                Some("heads") => RandomResultQuantity::CoinsHeads,
                Some("tails") => RandomResultQuantity::CoinsTails,
                _ => return Err(cursor.fail("`heads` or `tails`")),
            };
            consume_current(cursor)?;
            let bound = if both {
                (NumericComparisonOperator::GreaterThanOrEqual, 2)
            } else {
                greater_than_zero
            };
            (quantity, bound)
        }
        OutcomeConditionShape::Roll => {
            cursor.consume("you")?;
            consume_current(cursor)?;
            (
                RandomResultQuantity::DieResult,
                recognize_rolled_threshold(cursor)?,
            )
        }
        OutcomeConditionShape::Clash => {
            cursor.consume("you")?;
            consume_current(cursor)?;
            (RandomResultQuantity::ClashWon, greater_than_zero)
        }
    };
    Ok(Some(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(outcome_quantity(cursor, quantity)),
        operator,
        right: ConditionNumber::Integer(right),
    })))
}

/// One bound of a die-result row: the face values it covers (CR 706.3:
/// a results table maps result ranges to instructions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultRowRange {
    /// `N`, `N—M`, `N-M`: the results from `N` to `M` included.
    Between(i64, i64),
    /// `N+`, `N or more|higher|greater`.
    AtLeast(i64),
    /// `N or less|lower`.
    AtMost(i64),
    /// `N or M`: either result.
    Either(i64, i64),
}

fn face_value(raw: &str) -> Option<i64> {
    (!raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| raw.parse::<i64>().ok())
        .flatten()
        .filter(|value| *value > 0)
}

/// The written separator between a row's results and its instruction.
fn row_separator(raw: &str) -> bool {
    matches!(raw, "|" | "\u{2014}")
}

/// Reads a die-result row head (`1—9 |`, `20 |`, `15+ |`, `9 or less |`,
/// `1 or 2 —`, `6 —`) on `cursor`; `None` when the tokens are no row head.
/// The row head is structural: the range is complete before its separator.
fn read_result_row_head(cursor: &mut OracleTokenCursor<'_>) -> Option<ResultRowRange> {
    let first = cursor.peek()?.to_owned();
    // `1-9` is one token: the hyphen joins the two results.
    let (low, joined_high) = match first.split_once('-') {
        Some((low, high)) => (face_value(low)?, Some(face_value(high)?)),
        None => (face_value(&first)?, None),
    };
    cursor.advance_to(cursor.position() + 1).ok()?;
    let range = if let Some(high) = joined_high {
        ResultRowRange::Between(low, high)
    } else if matches!(cursor.peek(), Some("\u{2014}" | "-"))
        && cursor.peek_ahead(1).and_then(face_value).is_some()
        && cursor.peek_ahead(2).is_some_and(row_separator)
    {
        let high = cursor.peek_ahead(1).and_then(face_value)?;
        cursor.advance_to(cursor.position() + 2).ok()?;
        ResultRowRange::Between(low, high)
    } else if cursor.peek() == Some("+") {
        cursor.advance_to(cursor.position() + 1).ok()?;
        ResultRowRange::AtLeast(low)
    } else if word_at(cursor, 0, "or") {
        let bound = cursor.peek_ahead(1)?.to_ascii_lowercase();
        let range = match bound.as_str() {
            "less" | "lower" => ResultRowRange::AtMost(low),
            "more" | "higher" | "greater" => ResultRowRange::AtLeast(low),
            other => ResultRowRange::Either(low, face_value(other)?),
        };
        cursor.advance_to(cursor.position() + 2).ok()?;
        range
    } else {
        ResultRowRange::Between(low, low)
    };
    if let ResultRowRange::Between(low, high) = range
        && low > high
    {
        return None;
    }
    if !cursor.peek().is_some_and(row_separator) {
        return None;
    }
    cursor.advance_to(cursor.position() + 1).ok()?;
    Some(range)
}

/// True when a die-result row head starts at the cursor: the results of a
/// row of the table that follows a die roll (CR 706.3).
pub(in crate::oracle::canonical) fn die_result_row_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut trial = cursor.clone();
    read_result_row_head(&mut trial).is_some()
}

/// True when the instruction of the row at the cursor is one sentence: the
/// token after its first top-level period ends the text or heads the next
/// row. Quoted abilities and reminder text are nested scopes.
fn row_instruction_is_one_sentence(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            _ if quoted => {}
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => {
                let mut next = cursor.clone();
                if next.advance_to(cursor.position() + offset + 1).is_err() {
                    return false;
                }
                // A reminder after the period belongs to the same row.
                return next.peek().is_none()
                    || next.peek() == Some("(")
                    || die_result_row_starts(&next);
            }
            _ => {}
        }
        offset += 1;
    }
    false
}

/// Reads the head of a die-result row as the condition of its conditional
/// instruction: the latest die result lies in the row's results (CR 706.3).
/// The row's instruction must be one sentence (a longer row has no
/// multi-sentence branch yet and fails here), and an earlier action of the
/// program must have rolled the die.
pub(in crate::oracle::canonical) fn recognize_die_result_row(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    cursor.enter("condition.die-result-row");
    let mut trial = cursor.clone();
    let range = read_result_row_head(&mut trial)
        .ok_or_else(|| cursor.fail("a die-result row: `N—M |`, `N |`, `N+ |`"))?;
    *cursor = trial;
    cursor.enter("condition.die-result-row.roll");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    cursor.require(
        reader.is_some(),
        "a die roll earlier in the program whose result the row reads",
    )?;
    cursor.enter("condition.die-result-row.instruction");
    cursor.require(
        row_instruction_is_one_sentence(cursor),
        "a die-result row whose instruction is one sentence",
    )?;
    let result = outcome_quantity(cursor, RandomResultQuantity::DieResult);
    let compare = |operator: NumericComparisonOperator, value: i64| {
        BooleanConditionNode::Atom(ConditionAtom::Compare(ConditionComparison {
            subject: ConditionPlayerSubject::Controller,
            left: ConditionNumber::Expression(result.clone()),
            operator,
            right: ConditionNumber::Integer(value),
        }))
    };
    Ok(match range {
        ResultRowRange::Between(low, high) if low == high => {
            compare(NumericComparisonOperator::Equal, low)
        }
        ResultRowRange::Between(low, high) => BooleanConditionNode::And(vec![
            compare(NumericComparisonOperator::GreaterThanOrEqual, low),
            compare(NumericComparisonOperator::LessThanOrEqual, high),
        ]),
        ResultRowRange::AtLeast(low) => compare(NumericComparisonOperator::GreaterThanOrEqual, low),
        ResultRowRange::AtMost(high) => compare(NumericComparisonOperator::LessThanOrEqual, high),
        ResultRowRange::Either(first, second) => BooleanConditionNode::Or(vec![
            compare(NumericComparisonOperator::Equal, first),
            compare(NumericComparisonOperator::Equal, second),
        ]),
    })
}

/// `the result`, `the total of those results`, `the number of coins that
/// came up heads|tails`: a quantity of the latest outcome as an operand of
/// the shared numeric definition grammar (`equal to the result`, `where X
/// is the result minus 1`, `If the result is 3 or less`).
pub(in crate::oracle::canonical) fn random_result_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "the")
        && ((word_at(cursor, 1, "result") && !word_at(cursor, 2, "of"))
            || (word_at(cursor, 1, "total")
                && word_at(cursor, 2, "of")
                && word_at(cursor, 3, "those")
                && word_at(cursor, 4, "results"))
            || (word_at(cursor, 1, "number")
                && word_at(cursor, 2, "of")
                && word_at(cursor, 3, "coins")
                && word_at(cursor, 4, "that")
                && word_at(cursor, 5, "came")
                && word_at(cursor, 6, "up")))
}

/// Reads the quantity `random_result_starts` claims. The program must have
/// an earlier action: the first action of a program has no outcome to read.
pub(in crate::oracle::canonical) fn recognize_random_result(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.random-result");
    let quantity = if word_at(cursor, 1, "result") {
        cursor.consume_phrase(&["the", "result"])?;
        RandomResultQuantity::DieResult
    } else if word_at(cursor, 1, "total") {
        cursor.consume_phrase(&["the", "total", "of", "those", "results"])?;
        RandomResultQuantity::DieResult
    } else {
        cursor.consume_phrase(&["the", "number", "of", "coins", "that", "came", "up"])?;
        cursor.enter("numeric.random-result.face");
        let quantity = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("heads") => RandomResultQuantity::CoinsHeads,
            Some("tails") => RandomResultQuantity::CoinsTails,
            _ => return Err(cursor.fail("`heads` or `tails`")),
        };
        consume_current(cursor)?;
        quantity
    };
    cursor.enter("numeric.random-result.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "an earlier random action of this program whose outcome the phrase restates",
    )?;
    Ok(NumericExpression::ActionResultQuantity {
        reader_effect: reader,
        quantity: ActionResultQuantity::RandomResult(quantity),
    })
}

/// `support N` (CR 701.41a) is defined as "Put a +1/+1 counter on each of up
/// to N [other] target creatures", `other` only for a permanent's ability.
/// It is composed as that ordinary counter action over the shared target list;
/// the target cardinality must be a literal.
pub(in crate::oracle::canonical) fn recognize_support_action(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<PutCountersOnObjectEffectNode> {
    cursor.enter("effect.support.action");
    cursor.consume("support")?;
    cursor.enter("effect.support.quantity");
    let quantity = recognize_numeric_expression(cursor, None)?;
    let maximum = match quantity {
        NumericExpression::Atom(NumericAtom::Integer(value)) => u32::try_from(value).ok(),
        _ => None,
    }
    .filter(|value| *value > 0)
    .ok_or_else(|| cursor.fail("a positive literal support quantity"))?;
    let counter = crate::oracle::counter_name::CounterName::from_parts(["+1/+1"])
        .ok_or_else(|| cursor.fail("the +1/+1 counter name"))?;
    Ok(PutCountersOnObjectEffectNode {
        direction: super::CounterChangeDirection::Put,
        permanent: PermanentOperandNode::Targets(TargetListNode {
            slots: vec![TargetSlot {
                selector: TargetSelector::Object(ObjectPredicate::CardType(
                    ObjectCardType::Creature,
                )),
                cardinality: TargetCardinality {
                    minimum: 0,
                    maximum,
                },
                exclude_source: permanent_source_available(context),
                count: None,
            }],
        }),
        counter,
        count: NumericExpression::from(1),
        kinds: super::CounterKindsNode::Single,
    })
}
