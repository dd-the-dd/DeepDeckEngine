//! Actor and object are shared by an ordered list of alternative event verbs.
//! The enclosing trigger parent owns guards, effects and full body consumption.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActorObjectEventAction {
    CreateToken,
    SacrificePermanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActorObjectEventAlternativesNode {
    pub actor: EventActorScope,
    pub actions: Vec<ActorObjectEventAction>,
    pub object: ObjectQueryNode,
    pub exclude_source: bool,
}

pub(in crate::oracle::canonical) enum ActorObjectEventAttempt {
    NotClaimed,
    Recognized(ActorObjectEventAlternativesNode),
    Rejected(MachineFailure),
}

/// Both event families supply the permanent domain. Token status is a noun
/// and a stable identity property; type/color/numeric support remains a
/// separate lowering capability, never inferred from lexical recognition.
pub(in crate::oracle::canonical) fn recognize_permanent_event_object(
    cursor: &mut OracleTokenCursor<'_>,
    head_state: &'static str,
    domain_state: &'static str,
    controller_state: &'static str,
) -> MachineResult<(ObjectQueryNode, bool)> {
    cursor.enter(head_state);
    let exclude_source = if cursor
        .peek()
        .is_some_and(|s| s.eq_ignore_ascii_case("another"))
    {
        cursor.consume("another")?;
        true
    } else {
        let article = cursor
            .peek()
            .filter(|s| s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an"))
            .ok_or_else(|| cursor.fail("one singular permanent event, introduced by a/an/another"))?
            .to_string();
        cursor.consume(&article)?;
        false
    };
    let object = recognize_object_query_before_separator(cursor)?;
    cursor.enter(domain_state);
    cursor.require(
        object.predicate.supports_domain(ObjectKind::Permanent)
            && object.predicate.has_nominal_head(),
        "a permanent-compatible ObjectQuery with a nominal head",
    )?;
    cursor.enter(controller_state);
    cursor.require(
        object.controller.is_none(),
        "an object predicate without an additional unrepresented controller relation",
    )?;
    Ok((object, exclude_source))
}

/// One event verb of the lexicon in the actor's agreement (the shared
/// clause); the object is read once, after the alternatives.
fn read_action(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
) -> MachineResult<Option<ActorObjectEventAction>> {
    cursor.enter("event.actor-object.action");
    let agreement = if actor == EventActorScope::Controller {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    Ok(recognize_operand_verb(
        cursor,
        agreement,
        &[VerbLemma::Create, VerbLemma::Sacrifice],
        &[ArgumentFrame::TokenCreation, ArgumentFrame::ObjectSelection],
    )?
    .map(|(lemma, _)| match lemma {
        VerbLemma::Create => ActorObjectEventAction::CreateToken,
        _ => ActorObjectEventAction::SacrificePermanent,
    }))
}

fn recognize(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ActorObjectEventAlternativesNode>> {
    let Some(word) = cursor
        .peek()
        .filter(|s| s.eq_ignore_ascii_case("when") || s.eq_ignore_ascii_case("whenever"))
    else {
        return Ok(None);
    };
    let introducer = word.to_string();
    cursor.consume(&introducer)?;
    let Some(actor) = try_recognize_event_actor_scope(cursor)? else {
        return Ok(None);
    };
    let Some(first) = read_action(cursor, actor)? else {
        return Ok(None);
    };
    let mut actions = vec![first];
    let mut final_or = false;
    while cursor
        .peek()
        .is_some_and(|s| s == "," || s.eq_ignore_ascii_case("or"))
    {
        cursor.enter("event.actor-object.alternative");
        if cursor.peek() == Some(",") {
            cursor.consume(",")?;
        }
        final_or = cursor.peek().is_some_and(|s| s.eq_ignore_ascii_case("or"));
        if final_or {
            cursor.consume("or")?;
        }
        let next = read_action(cursor, actor)?.ok_or_else(|| {
            cursor.fail("another supported event action sharing the actor and object")
        })?;
        actions.push(next);
    }
    // Preserve the established single-sacrifice reader and its diagnostics.
    if actions == [ActorObjectEventAction::SacrificePermanent] {
        return Ok(None);
    }
    cursor.enter("event.actor-object.alternative-complete");
    cursor.require(
        actions.len() == 1 || final_or,
        "event alternatives joined by a final or",
    )?;
    let (object, exclude_source) = recognize_permanent_event_object(
        cursor,
        "event.actor-object.object-head",
        "event.actor-object.object-domain",
        "event.actor-object.object-controller",
    )?;
    cursor.enter("event.actor-object.separator");
    cursor.consume(",")?;
    Ok(Some(ActorObjectEventAlternativesNode {
        actor,
        actions,
        object,
        exclude_source,
    }))
}

pub(in crate::oracle::canonical) fn try_recognize_actor_object_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> ActorObjectEventAttempt {
    let checkpoint = cursor.checkpoint();
    match recognize(cursor) {
        Ok(Some(event)) => ActorObjectEventAttempt::Recognized(event),
        Ok(None) => {
            cursor.rollback(checkpoint);
            ActorObjectEventAttempt::NotClaimed
        }
        Err(failure) => ActorObjectEventAttempt::Rejected(failure),
    }
}

#[cfg(test)]
#[path = "actor_object_event_tests.rs"]
mod tests;
