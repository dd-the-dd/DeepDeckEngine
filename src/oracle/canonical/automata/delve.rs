use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, LocalObjectReference, ObjectPredicate, PutCountersOnObjectEffectNode,
    recognize_counter_name, recognize_object_type_union_list, recognize_put_counters_effect,
    recognize_source_reference,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelveKeywordNode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelveEntryCountersNode {
    pub source: LocalObjectReference,
    pub counter: crate::oracle::counter_name::CounterName,
    pub cards: ObjectPredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GraveyardLeaveAggregationNode {
    PerCard,
    OneOrMore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct GraveyardLeaveCounterTriggerNode {
    pub cards: ObjectPredicate,
    pub aggregation: GraveyardLeaveAggregationNode,
    pub effect: PutCountersOnObjectEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DelveState {
    Keyword,
    ReminderOrComplete,
    ReminderOpen,
    Subject,
    Action,
    Zone,
    Temporal,
    Spell,
    PaymentVerb,
    PaymentRelation,
    Payment,
    ReminderTerminator,
    ReminderClose,
    Complete,
}

/// Recognizes Delve and validates its canonical reminder token by token. The
/// reminder is explanatory only; the semantic output remains the Delve
/// keyword primitive used by casting-cost execution.
pub(in crate::oracle::canonical) fn recognize_delve_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DelveKeywordNode> {
    let mut state = DelveState::Keyword;
    loop {
        state = match state {
            DelveState::Keyword => {
                cursor.enter("keyword.delve.keyword");
                let keyword = cursor.consume_type("keywordWord")?;
                cursor.require(
                    keyword.raw.eq_ignore_ascii_case("delve"),
                    "the `delve` keyword",
                )?;
                DelveState::ReminderOrComplete
            }
            DelveState::ReminderOrComplete => {
                cursor.enter("keyword.delve.reminder-or-complete");
                match cursor.peek() {
                    None => DelveState::Complete,
                    Some("(") => DelveState::ReminderOpen,
                    _ => return Err(cursor.fail("a canonical Delve reminder or end of input")),
                }
            }
            DelveState::ReminderOpen => {
                cursor.enter("keyword.delve.reminder.open");
                cursor.consume("(")?;
                DelveState::Subject
            }
            DelveState::Subject => {
                cursor.enter("keyword.delve.reminder.subject");
                cursor.consume_phrase(&["each", "card", "you"])?;
                DelveState::Action
            }
            DelveState::Action => {
                cursor.enter("keyword.delve.reminder.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Exile,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                DelveState::Zone
            }
            DelveState::Zone => {
                cursor.enter("keyword.delve.reminder.zone");
                cursor.consume_phrase(&["from", "your", "graveyard"])?;
                DelveState::Temporal
            }
            DelveState::Temporal => {
                cursor.enter("keyword.delve.reminder.temporal");
                cursor.consume("while")?;
                DelveState::Spell
            }
            DelveState::Spell => {
                cursor.enter("keyword.delve.reminder.spell");
                cursor.consume_phrase(&["casting", "this", "spell"])?;
                DelveState::PaymentVerb
            }
            DelveState::PaymentVerb => {
                cursor.enter("keyword.delve.reminder.payment-verb");
                cursor.consume("pays")?;
                DelveState::PaymentRelation
            }
            DelveState::PaymentRelation => {
                cursor.enter("keyword.delve.reminder.payment-relation");
                cursor.consume("for")?;
                DelveState::Payment
            }
            DelveState::Payment => {
                cursor.enter("keyword.delve.reminder.payment");
                cursor.consume("{1}")?;
                DelveState::ReminderTerminator
            }
            DelveState::ReminderTerminator => {
                cursor.enter("keyword.delve.reminder.terminator");
                cursor.consume(".")?;
                DelveState::ReminderClose
            }
            DelveState::ReminderClose => {
                cursor.enter("keyword.delve.reminder.close");
                cursor.consume(")")?;
                DelveState::Complete
            }
            DelveState::Complete => {
                cursor.enter("keyword.delve.complete");
                cursor.finish()?;
                return Ok(DelveKeywordNode);
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryCountersState {
    Source,
    Event,
    Relation,
    Article,
    CounterKind,
    CounterNoun,
    Recipient,
    Scale,
    Selector,
    CardNoun,
    ExileReference,
    Terminator,
    Complete,
}

/// Recognizes an entry-counter replacement whose count is the matching cards
/// recorded by a casting decision. The decision identity is deliberately a
/// lowering concern, keeping selector and counter syntax reusable.
pub(in crate::oracle::canonical) fn recognize_delve_entry_counters(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<DelveEntryCountersNode> {
    let mut state = EntryCountersState::Source;
    let mut source = None;
    let mut counter = None;
    let mut cards = None;
    loop {
        state = match state {
            EntryCountersState::Source => {
                cursor.enter("replacement.decision-entry-counters.source");
                source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                EntryCountersState::Event
            }
            EntryCountersState::Event => {
                cursor.enter("replacement.decision-entry-counters.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                EntryCountersState::Relation
            }
            EntryCountersState::Relation => {
                cursor.enter("replacement.decision-entry-counters.relation");
                cursor.consume("with")?;
                EntryCountersState::Article
            }
            EntryCountersState::Article => {
                cursor.enter("replacement.decision-entry-counters.article");
                let article = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    article.raw.eq_ignore_ascii_case("a") || article.raw.eq_ignore_ascii_case("an"),
                    "the singular counter article `a` or `an`",
                )?;
                EntryCountersState::CounterKind
            }
            EntryCountersState::CounterKind => {
                cursor.enter("replacement.decision-entry-counters.counter-kind");
                counter = Some(recognize_counter_name(cursor)?);
                EntryCountersState::CounterNoun
            }
            EntryCountersState::CounterNoun => {
                cursor.enter("replacement.decision-entry-counters.counter-noun");
                cursor.consume("counter")?;
                EntryCountersState::Recipient
            }
            EntryCountersState::Recipient => {
                cursor.enter("replacement.decision-entry-counters.recipient");
                cursor.consume_phrase(&["on", "it"])?;
                EntryCountersState::Scale
            }
            EntryCountersState::Scale => {
                cursor.enter("replacement.decision-entry-counters.scale");
                cursor.consume_phrase(&["for", "each"])?;
                EntryCountersState::Selector
            }
            EntryCountersState::Selector => {
                cursor.enter("replacement.decision-entry-counters.selector");
                cards = Some(recognize_object_type_union_list(cursor)?);
                EntryCountersState::CardNoun
            }
            EntryCountersState::CardNoun => {
                cursor.enter("replacement.decision-entry-counters.card-noun");
                cursor.consume("card")?;
                EntryCountersState::ExileReference
            }
            EntryCountersState::ExileReference => {
                cursor.enter("replacement.decision-entry-counters.exile-reference");
                cursor.consume_phrase(&["exiled", "with", "it"])?;
                EntryCountersState::Terminator
            }
            EntryCountersState::Terminator => {
                cursor.enter("replacement.decision-entry-counters.terminator");
                cursor.consume(".")?;
                EntryCountersState::Complete
            }
            EntryCountersState::Complete => {
                cursor.enter("replacement.decision-entry-counters.complete");
                cursor.finish()?;
                return Ok(DelveEntryCountersNode {
                    source: cursor.require_value(source, "the entering source")?,
                    counter: cursor.require_value(counter, "a counter name")?,
                    cards: cursor.require_value(cards, "a card selector")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraveyardLeaveState {
    Trigger,
    Aggregation,
    Selector,
    CardNoun,
    Event,
    Zone,
    Separator,
    Effect,
    Terminator,
    Complete,
}

/// Recognizes a controller-graveyard leave event followed by one independent
/// put-counters effect. Singular Oracle syntax means one trigger per card;
/// `one or more` means one aggregate trigger for the batch.
pub(in crate::oracle::canonical) fn recognize_graveyard_leave_counter_trigger(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<GraveyardLeaveCounterTriggerNode> {
    let mut state = GraveyardLeaveState::Trigger;
    let mut aggregation = None;
    let mut cards = None;
    let mut effect = None;
    loop {
        state = match state {
            GraveyardLeaveState::Trigger => {
                cursor.enter("trigger.graveyard-leave.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("whenever"),
                    "the `whenever` trigger",
                )?;
                GraveyardLeaveState::Aggregation
            }
            GraveyardLeaveState::Aggregation => {
                cursor.enter("trigger.graveyard-leave.aggregation");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("one"))
                {
                    cursor.consume_phrase(&["one", "or", "more"])?;
                    aggregation = Some(GraveyardLeaveAggregationNode::OneOrMore);
                } else {
                    let article = cursor.consume_type("quantifierWord")?;
                    cursor.require(
                        article.raw.eq_ignore_ascii_case("a")
                            || article.raw.eq_ignore_ascii_case("an"),
                        "`a`, `an`, or `one or more`",
                    )?;
                    aggregation = Some(GraveyardLeaveAggregationNode::PerCard);
                }
                GraveyardLeaveState::Selector
            }
            GraveyardLeaveState::Selector => {
                cursor.enter("trigger.graveyard-leave.selector");
                cards = Some(recognize_object_type_union_list(cursor)?);
                GraveyardLeaveState::CardNoun
            }
            GraveyardLeaveState::CardNoun => {
                cursor.enter("trigger.graveyard-leave.card-noun");
                let noun = match aggregation {
                    Some(GraveyardLeaveAggregationNode::PerCard) => "card",
                    Some(GraveyardLeaveAggregationNode::OneOrMore) => "cards",
                    None => return Err(cursor.fail("a populated event aggregation")),
                };
                cursor.consume(noun)?;
                GraveyardLeaveState::Event
            }
            GraveyardLeaveState::Event => {
                cursor.enter("trigger.graveyard-leave.event");
                let event = cursor.consume_type("eventWord")?;
                let expected = match aggregation {
                    Some(GraveyardLeaveAggregationNode::PerCard) => "leaves",
                    Some(GraveyardLeaveAggregationNode::OneOrMore) => "leave",
                    None => return Err(cursor.fail("a populated event aggregation")),
                };
                cursor.require(event.raw.eq_ignore_ascii_case(expected), expected)?;
                GraveyardLeaveState::Zone
            }
            GraveyardLeaveState::Zone => {
                cursor.enter("trigger.graveyard-leave.zone");
                cursor.consume_phrase(&["your", "graveyard"])?;
                GraveyardLeaveState::Separator
            }
            GraveyardLeaveState::Separator => {
                cursor.enter("trigger.graveyard-leave.separator");
                cursor.consume(",")?;
                GraveyardLeaveState::Effect
            }
            GraveyardLeaveState::Effect => {
                cursor.enter("trigger.graveyard-leave.effect");
                let context =
                    EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);
                effect = Some(recognize_put_counters_effect(
                    cursor,
                    name_links,
                    name_failure,
                    &context,
                )?);
                GraveyardLeaveState::Terminator
            }
            GraveyardLeaveState::Terminator => {
                cursor.enter("trigger.graveyard-leave.terminator");
                cursor.consume(".")?;
                GraveyardLeaveState::Complete
            }
            GraveyardLeaveState::Complete => {
                cursor.enter("trigger.graveyard-leave.complete");
                cursor.finish()?;
                return Ok(GraveyardLeaveCounterTriggerNode {
                    cards: cursor.require_value(cards, "a card selector")?,
                    aggregation: cursor.require_value(aggregation, "an event aggregation")?,
                    effect: cursor.require_value(effect, "a put-counters effect")?,
                });
            }
        };
    }
}
