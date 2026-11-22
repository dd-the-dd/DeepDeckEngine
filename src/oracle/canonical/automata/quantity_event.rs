use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::counter_name::CounterName;

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, NumericAtom, ObjectControllerScope, ObjectKind, ObjectPredicate,
    ReplacementObjectBinding, object_predicate_uses_x, recognize_counter_name,
    recognize_numeric_atom, recognize_object_query_before_separator,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum QuantityEventDomain {
    TokenCreation,
    CounterPlacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum QuantityEventOrigin {
    Any,
    Effect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum QuantityEventVoice {
    ActiveEffect,
    Passive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QuantityEventNode {
    pub domain: QuantityEventDomain,
    pub origin: QuantityEventOrigin,
    pub voice: QuantityEventVoice,
    pub minimum: i64,
    pub affected_controller: ObjectControllerScope,
    pub object: Option<ObjectPredicate>,
    pub counter: Option<CounterName>,
    pub object_binding: ReplacementObjectBinding,
    pub quantity_binding: EventNumericBinding,
}

/// A positive-quantity event guard, distinct from its later transformation.
fn recognize_positive_event_quantity(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    cursor.enter("quantity-event.minimum");
    let minimum = recognize_numeric_atom(cursor)?;
    cursor.require(
        minimum == NumericAtom::Integer(1),
        "the literal one-or-more event threshold",
    )?;
    cursor.consume_phrase(&["or", "more"])?;
    Ok(1)
}

/// Opaque counter names are delegated to the existing contextual name child.
pub(in crate::oracle::canonical) fn recognize_event_counter_kind(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CounterName>> {
    cursor.enter("quantity-event.counter-name");
    let name = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("counters"))
    {
        None
    } else {
        Some(recognize_counter_name(cursor)?)
    };
    cursor.consume("counters")?;
    Ok(name)
}

fn recognize_counter_recipient(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("quantity-event.counter-recipient.article");
    cursor.consume("on")?;
    let article = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a permanent article"))?
        .to_string();
    cursor.require(
        article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an"),
        "`a` or `an` before a permanent query",
    )?;
    cursor.consume(&article)?;
    let query = recognize_object_query_before_separator(cursor)?;
    cursor.enter("quantity-event.counter-recipient.domain");
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent)
            && query.predicate.has_domain(ObjectKind::Permanent),
        "a shared permanent-domain object query",
    )?;
    cursor.enter("quantity-event.counter-recipient.controller");
    cursor.require(
        query.controller == Some(ObjectControllerScope::AbilityController),
        "a recipient controlled by the ability controller",
    )?;
    cursor.enter("quantity-event.counter-recipient.x-binding");
    cursor.require(
        !object_predicate_uses_x(&query.predicate),
        "a replacement predicate without an undeclared X producer",
    )?;
    Ok(query.predicate)
}

/// Recognizes the event clause only. The parent owns `if`, comma, replacement
/// body and full consumption; this child exports typed object/quantity bindings.
pub(in crate::oracle::canonical) enum QuantityEventRecognition {
    Parsed(QuantityEventNode),
    NotClaimed,
    Failed(MachineFailure),
}

/// A counter/token event domain, not cursor advancement, commits this family.
/// A failed unrelated header restores the caller's complete checkpoint.
pub(in crate::oracle::canonical) fn try_recognize_quantity_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> QuantityEventRecognition {
    let checkpoint = cursor.checkpoint();
    let mut domain_committed = false;
    match recognize_quantity_event_inner(cursor, &mut domain_committed) {
        Ok(event) => QuantityEventRecognition::Parsed(event),
        Err(failure) if domain_committed => QuantityEventRecognition::Failed(failure),
        Err(_) => {
            cursor.rollback(checkpoint);
            QuantityEventRecognition::NotClaimed
        }
    }
}

fn recognize_quantity_event_inner(
    cursor: &mut OracleTokenCursor<'_>,
    domain_committed: &mut bool,
) -> MachineResult<QuantityEventNode> {
    cursor.enter("quantity-event.voice");
    let active = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("an"));
    let mut domain = QuantityEventDomain::CounterPlacement;
    let origin = if active {
        cursor.consume_phrase(&["an", "effect", "would"])?;
        cursor.enter("quantity-event.action");
        if crate::oracle::canonical::automata::lemma_at(
            &cursor,
            VerbLemma::Create,
            crate::oracle::canonical::automata::VerbAgreement::Base,
        ) {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Create,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            )?;
            domain = QuantityEventDomain::TokenCreation;
        } else {
            crate::oracle::canonical::automata::consume_lemma(
                cursor,
                crate::oracle::canonical::automata::VerbLemma::Put,
                crate::oracle::canonical::automata::VerbAgreement::Base,
            )?;
        }
        QuantityEventOrigin::Effect
    } else {
        QuantityEventOrigin::Any
    };
    let minimum = recognize_positive_event_quantity(cursor)?;
    let (counter, object) = match domain {
        QuantityEventDomain::TokenCreation => {
            cursor.enter("quantity-event.tokens");
            cursor.consume("tokens")?;
            *domain_committed = true;
            cursor.consume_phrase(&["under", "your", "control"])?;
            (None, None)
        }
        QuantityEventDomain::CounterPlacement => {
            let counter = recognize_event_counter_kind(cursor)?;
            if !active {
                cursor.enter("quantity-event.passive-placement");
                cursor.consume_phrase(&["would", "be", "put"])?;
            }
            *domain_committed = true;
            (counter, Some(recognize_counter_recipient(cursor)?))
        }
    };
    cursor.enter("quantity-event.complete");
    Ok(QuantityEventNode {
        domain,
        origin,
        voice: if active {
            QuantityEventVoice::ActiveEffect
        } else {
            QuantityEventVoice::Passive
        },
        minimum,
        affected_controller: ObjectControllerScope::AbilityController,
        object,
        counter,
        object_binding: ReplacementObjectBinding::EventObject,
        quantity_binding: EventNumericBinding::ReplacementQuantity,
    })
}
