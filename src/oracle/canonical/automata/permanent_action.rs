//! Ordinary action payloads share one subject grammar and lexical binding.
use super::super::{MachineFailure, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

pub(in crate::oracle::canonical) enum PermanentActionAttempt {
    NotClaimed,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(LocalEffectNode),
}

pub(in crate::oracle::canonical) fn ordinary_keyword_subject(
    node: TemporaryKeywordGrantNode,
) -> TemporaryKeywordGrantNode<PermanentOperandNode> {
    let objects = match node.objects {
        TemporaryKeywordGrantObjectsNode::Targets(targets) => {
            PermanentOperandNode::Targets(targets)
        }
        TemporaryKeywordGrantObjectsNode::ControlledSet(set) => {
            PermanentOperandNode::Collection(ObjectQueryNode {
                predicate: set.predicate,
                controller: Some(set.controller),
            })
        }
    };
    TemporaryKeywordGrantNode {
        objects,
        keywords: node.keywords,
        abilities: node.abilities,
        duration: node.duration,
    }
}

/// A recognized subject plus the immediately following action owns the claim.
/// The payload child receives its typed subject and end cursor, without reparsing.
pub(in crate::oracle::canonical) fn try_recognize_permanent_action(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> PermanentActionAttempt {
    if let Some(reference) = context.verb_chain_subject
        && cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("gains") || raw.eq_ignore_ascii_case("becomes")
        })
    {
        return recognize_verb_chain_action(cursor, reference, context);
    }
    let mut subject_end = cursor.clone();
    let subject = match recognize_contextual_permanent_operand(
        &mut subject_end,
        name_links,
        name_failure,
        context,
    ) {
        Ok(subject) => subject,
        Err(failure) => return PermanentActionAttempt::RejectedCandidate(failure),
    };
    let action_offset = usize::from(
        subject_end
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("also")),
    );
    let Some(action) = subject_end.peek_ahead(action_offset) else {
        return PermanentActionAttempt::NotClaimed;
    };
    let keyword = action.eq_ignore_ascii_case("gain") || action.eq_ignore_ascii_case("gains");
    let stats = action_offset == 0
        && (action.eq_ignore_ascii_case("get") || action.eq_ignore_ascii_case("gets"));
    let becomes = action_offset == 0 && action.eq_ignore_ascii_case("becomes");
    let deals = action_offset == 0
        && action.eq_ignore_ascii_case("deals")
        && matches!(
            subject,
            PermanentOperandNode::Targets(_) | PermanentOperandNode::BoundReference(_)
        );
    let fights = action_offset == 0
        && (action.eq_ignore_ascii_case("fights") || action.eq_ignore_ascii_case("fight"));
    if !keyword && !stats && !becomes && !deals && !fights {
        return PermanentActionAttempt::NotClaimed;
    }
    if becomes {
        // A type-change frame is only a candidate: other readers own the
        // `becomes a P/T ...` animation shapes, so a non-matching type list
        // must not commit this parent.
        let mut trial = cursor.clone();
        let fragment = match super::become_types::recognize_become_types_subject(
            &mut trial,
            |cursor| {
                *cursor = subject_end.clone();
                Ok(subject.clone())
            },
            context.preposed_duration.as_ref(),
        ) {
            Ok(fragment) => fragment,
            Err(failure) => return PermanentActionAttempt::RejectedCandidate(failure),
        };
        if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context) {
            return PermanentActionAttempt::FailedCommitted(failure);
        }
        *cursor = trial;
        return PermanentActionAttempt::Parsed(LocalEffectNode::BecomeTypes(fragment.node));
    }
    if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context) {
        return PermanentActionAttempt::FailedCommitted(failure);
    }
    if deals || fights {
        let mut trial = subject_end.clone();
        let result = if deals {
            let antecedent = match &subject {
                PermanentOperandNode::Targets(targets) => {
                    match (targets.slots.as_slice(), context.current_effect_index) {
                        ([_], Some(effect_index)) => {
                            DamageSourceAntecedent::Target(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            })
                        }
                        _ => DamageSourceAntecedent::Unavailable,
                    }
                }
                PermanentOperandNode::BoundReference(PermanentEffectReference::Target(binding)) => {
                    DamageSourceAntecedent::Target(*binding)
                }
                _ => DamageSourceAntecedent::Unavailable,
            };
            recognize_damage_predicate(&mut trial, subject.clone(), antecedent)
                .map(LocalEffectNode::DealDamage)
        } else {
            recognize_fight_predicate(
                &mut trial,
                subject.clone(),
                name_links,
                name_failure,
                context,
            )
            .map(LocalEffectNode::Fight)
        };
        return match result {
            Ok(node) => {
                *cursor = trial;
                PermanentActionAttempt::Parsed(node)
            }
            Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
        };
    }
    let mut trial = cursor.clone();
    let result = if keyword {
        super::keyword_grant::recognize_temporary_keyword_grant_subject(
            &mut trial,
            |cursor| {
                *cursor = subject_end.clone();
                Ok(subject.clone())
            },
            false,
            context.preposed_duration.as_ref(),
        )
        .map(LocalEffectNode::TemporaryKeywordGrant)
    } else {
        super::temporary_power_toughness::recognize_temporary_power_toughness_subject(
            &mut trial,
            |cursor| {
                *cursor = subject_end.clone();
                Ok(subject.clone())
            },
            context.preposed_duration.as_ref(),
        )
        .map(|fragment| LocalEffectNode::ModifyStats(fragment.node))
    };
    match result {
        Ok(node) => {
            *cursor = trial;
            PermanentActionAttempt::Parsed(node)
        }
        Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
    }
}

/// `fights OBJECT` (CR 701.14) after a recognized permanent subject. The
/// second fighter is read by the same contextual permanent operand grammar.
fn recognize_fight_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    first: PermanentOperandNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> Result<FightEffectNode, MachineFailure> {
    cursor.enter("effect.fight.action");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("fight"))
    {
        cursor.consume("fight")?;
    } else {
        cursor.consume("fights")?;
    }
    cursor.enter("effect.fight.second");
    let second = recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    cursor.require(
        !matches!(second, PermanentOperandNode::Collection(_)),
        "one permanent fighting the subject, not a collection",
    )?;
    Ok(FightEffectNode { first, second })
}

/// Source references are admitted only by a parent that establishes source
/// context, not by interpreting an instant's name as a battlefield object.
fn subject_context_failure(
    cursor: &OracleTokenCursor<'_>,
    subject: &PermanentOperandNode,
    subject_end: &OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> Option<MachineFailure> {
    let explicit_activation_source_available = matches!(
        subject,
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf)
    ) && context.activation_source_availability
        == ExplicitActivationSourceAvailability::RetainedBattlefieldPermanent;
    if matches!(subject, PermanentOperandNode::Reference(_))
        && context.source.is_none()
        && !explicit_activation_source_available
    {
        return Some(subject_end.fail("a proven permanent-source context for a source subject"));
    }
    if matches!(subject, PermanentOperandNode::Reference(_))
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        use crate::oracle::characteristic_vocabulary::{
            AcceptedForms, ObjectDomain, lookup_card_type, lookup_object_domain,
        };
        let nominal = cursor.peek_ahead(1).unwrap_or_default();
        let incompatible = lookup_card_type(nominal, AcceptedForms::SingularOnly)
            .is_some_and(|kind| !kind.may_appear_on_permanent())
            || lookup_object_domain(nominal, AcceptedForms::SingularOnly)
                == Some(ObjectDomain::Spell);
        if incompatible {
            return Some(
                subject_end.fail("a source nominal compatible with the permanent subject domain"),
            );
        }
    }
    None
}

/// Verb coordination (`X gets +1/+1 and becomes a Doctor and gains flying`)
/// shares the grammatical subject of the previous continuous-effect action.
/// The sequence supplies that proven subject; this child never invents one.
fn recognize_verb_chain_action(
    cursor: &mut OracleTokenCursor<'_>,
    reference: PermanentEffectReference,
    context: &EffectRecognitionContext,
) -> PermanentActionAttempt {
    let subject = match reference {
        PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
        other => PermanentOperandNode::BoundReference(other),
    };
    let mut trial = cursor.clone();
    let node = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("becomes"))
    {
        super::become_types::recognize_become_types_subject(
            &mut trial,
            |_| Ok(subject.clone()),
            context.preposed_duration.as_ref(),
        )
        .map(|fragment| LocalEffectNode::BecomeTypes(fragment.node))
    } else {
        super::keyword_grant::recognize_temporary_keyword_grant_subject(
            &mut trial,
            |_| Ok(subject.clone()),
            false,
            context.preposed_duration.as_ref(),
        )
        .map(LocalEffectNode::TemporaryKeywordGrant)
    };
    match node {
        Ok(node) => {
            *cursor = trial;
            PermanentActionAttempt::Parsed(node)
        }
        Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
    }
}

#[cfg(test)]
#[path = "permanent_action_tests.rs"]
mod tests;
