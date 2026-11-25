use crate::oracle::canonical::automata::VerbLemma;
use crate::oracle::counter_name::CounterName;

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, NumericAtom, ObjectControllerScope, ObjectPredicate,
    ReplacementObjectBinding, object_predicate_uses_x, recognize_counter_name,
    recognize_numeric_atom, recognize_object_query, recognize_optional_source_exclusion,
    recognize_source_reference,
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

/// How the counter recipient relates to the replacement's source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RecipientSourceRelation {
    /// `a creature you control`: any permanent the query admits.
    #[default]
    Any,
    /// `another creature you control`: other than the source.
    ExcludeSource,
    /// `this creature`, the source's name: the source itself.
    Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QuantityEventNode {
    pub domain: QuantityEventDomain,
    pub origin: QuantityEventOrigin,
    pub voice: QuantityEventVoice,
    pub minimum: i64,
    /// The controller of the created tokens or of the counter recipient;
    /// `None` when the event names no player (`If one or more tokens would
    /// be created`, `put on a creature`): every player's.
    pub affected_controller: Option<ObjectControllerScope>,
    pub recipient_source: RecipientSourceRelation,
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

/// The permanent counters would be put on: `a|an|another QUERY` with its
/// controller relation, or the source itself (`this creature`, its name).
struct CounterRecipient {
    predicate: Option<ObjectPredicate>,
    controller: Option<ObjectControllerScope>,
    source: RecipientSourceRelation,
}

fn recognize_counter_recipient(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterRecipient> {
    cursor.enter("quantity-event.counter-recipient.article");
    cursor.consume("on")?;
    let names = cursor.source_names().to_vec();
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        || names
            .iter()
            .any(|link| link.token_start == cursor.position())
    {
        recognize_source_reference(cursor, &names, None)?;
        return Ok(CounterRecipient {
            predicate: None,
            controller: None,
            source: RecipientSourceRelation::Source,
        });
    }
    let article = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a permanent article"))?
        .to_string();
    cursor.require(
        article.eq_ignore_ascii_case("a")
            || article.eq_ignore_ascii_case("an")
            || article.eq_ignore_ascii_case("another"),
        "`a`, `an` or `another` before a permanent query",
    )?;
    cursor.consume(&article)?;
    let mut exclude_source = article.eq_ignore_ascii_case("another");
    if !exclude_source {
        exclude_source = recognize_optional_source_exclusion(cursor)?;
    }
    let query = recognize_object_query(cursor)?;
    cursor.enter("quantity-event.counter-recipient.domain");
    cursor.require(
        query.denotes_battlefield_permanents(),
        "a shared permanent-domain object query",
    )?;
    cursor.enter("quantity-event.counter-recipient.controller");
    cursor.require(
        matches!(
            query.controller,
            None | Some(
                ObjectControllerScope::AbilityController
                    | ObjectControllerScope::OpponentOfAbilityController
            )
        ),
        "a recipient controlled by the ability controller, an opponent, or any player",
    )?;
    cursor.enter("quantity-event.counter-recipient.x-binding");
    cursor.require(
        !object_predicate_uses_x(&query.predicate),
        "a replacement predicate without an undeclared X producer",
    )?;
    Ok(CounterRecipient {
        predicate: Some(query.predicate),
        controller: query.controller,
        source: if exclude_source || query.exclude_source {
            RecipientSourceRelation::ExcludeSource
        } else {
            RecipientSourceRelation::Any
        },
    })
}

/// The offset of the `tokens` noun closing `[OBJECT] tokens would be created`
/// (CR 111.1, 614.1a): the passive token-creation event. The words before it
/// are the shared object predicate of the created tokens.
fn passive_token_creation_noun(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw.eq_ignore_ascii_case("would") || matches!(raw, "," | ".") {
            return None;
        }
        if raw.eq_ignore_ascii_case("tokens")
            && cursor
                .peek_ahead(offset + 1)
                .is_some_and(|next| next.eq_ignore_ascii_case("would"))
            && cursor
                .peek_ahead(offset + 2)
                .is_some_and(|next| next.eq_ignore_ascii_case("be"))
            && cursor
                .peek_ahead(offset + 3)
                .is_some_and(|next| next.eq_ignore_ascii_case("created"))
        {
            return Some(offset);
        }
        offset += 1;
    }
    None
}

/// `under your control` closing a token-creation event; absent, the event
/// names every player's tokens (CR 111.2: tokens enter under the control
/// of the player who creates them).
fn recognize_token_controller(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectControllerScope>> {
    cursor.enter("quantity-event.tokens.controller");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("under"))
    {
        cursor.consume_phrase(&["under", "your", "control"])?;
        Ok(Some(ObjectControllerScope::AbilityController))
    } else {
        Ok(None)
    }
}

/// `[OBJECT] tokens would be created [under your control]`: the object
/// predicate, when stated, is read by the shared object query.
fn recognize_passive_token_creation(
    cursor: &mut OracleTokenCursor<'_>,
    noun: usize,
    domain_committed: &mut bool,
) -> MachineResult<(Option<ObjectPredicate>, Option<ObjectControllerScope>)> {
    cursor.enter("quantity-event.passive-tokens.object");
    let object = if noun > 0 {
        let end = cursor.position() + noun;
        let query = cursor.bounded(end, recognize_object_query)?;
        cursor.require(
            query.controller.is_none() && !query.exclude_source,
            "a created-token predicate without a controller relation",
        )?;
        cursor.require(
            !object_predicate_uses_x(&query.predicate),
            "a created-token predicate without an undeclared X producer",
        )?;
        Some(query.predicate)
    } else {
        None
    };
    cursor.enter("quantity-event.passive-tokens.event");
    cursor.consume_phrase(&["tokens", "would", "be", "created"])?;
    *domain_committed = true;
    let controller = recognize_token_controller(cursor)?;
    Ok((object, controller))
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
    if !active && let Some(noun) = passive_token_creation_noun(cursor) {
        let (object, affected_controller) =
            recognize_passive_token_creation(cursor, noun, domain_committed)?;
        cursor.enter("quantity-event.complete");
        return Ok(QuantityEventNode {
            domain: QuantityEventDomain::TokenCreation,
            origin,
            voice: QuantityEventVoice::Passive,
            minimum,
            affected_controller,
            recipient_source: RecipientSourceRelation::Any,
            object,
            counter: None,
            object_binding: ReplacementObjectBinding::EventObject,
            quantity_binding: EventNumericBinding::ReplacementQuantity,
        });
    }
    let (counter, object, affected_controller, recipient_source) = match domain {
        QuantityEventDomain::TokenCreation => {
            cursor.enter("quantity-event.tokens");
            cursor.consume("tokens")?;
            *domain_committed = true;
            let controller = recognize_token_controller(cursor)?;
            (None, None, controller, RecipientSourceRelation::Any)
        }
        QuantityEventDomain::CounterPlacement => {
            let counter = recognize_event_counter_kind(cursor)?;
            if !active {
                cursor.enter("quantity-event.passive-placement");
                cursor.consume_phrase(&["would", "be", "put"])?;
            }
            *domain_committed = true;
            let recipient = recognize_counter_recipient(cursor)?;
            (
                counter,
                recipient.predicate,
                recipient.controller,
                recipient.source,
            )
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
        affected_controller,
        recipient_source,
        object,
        counter,
        object_binding: ReplacementObjectBinding::EventObject,
        quantity_binding: EventNumericBinding::ReplacementQuantity,
    })
}
