//! Keyword actions of CR 701 read as ordinary verbs of the shared effect
//! grammar. Subjects and objects come from the shared permanent operand; the
//! numeric argument from the shared numeric expression. Each action keeps its
//! identity until lowering, where it expands into ordinary primitives.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
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
    })
}
