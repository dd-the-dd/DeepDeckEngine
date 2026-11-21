use crate::oracle::{
    counter_name::CounterName,
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, EffectRecognitionContext, GrantedKeyword, LocalObjectReference,
    NumericAtom, NumericComparisonOperator, ObjectControllerScope, ObjectKind, ObjectPredicate,
    PutCountersOnObjectEffectNode, TargetBinding, TargetListNode, TargetSelector,
    recognize_counter_name, recognize_effect_duration, recognize_keyword_list,
    recognize_numeric_atom, recognize_object_query, recognize_put_counters_effect,
    recognize_target_list,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AbilityWordMarkerNode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControlledPermanentEntryEventNode {
    pub object: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetCounterPlacementNode {
    pub count: NumericAtom,
    pub counter: CounterName,
    pub targets: TargetListNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCounterThresholdNode {
    pub source: LocalObjectReference,
    pub counter: CounterName,
    pub operator: NumericComparisonOperator,
    pub threshold: NumericAtom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BoundTemporaryKeywordGrantNode {
    pub object: TargetBinding,
    pub keywords: Vec<GrantedKeyword>,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReflexiveCounterFollowupNode {
    pub condition: SourceCounterThresholdNode,
    pub placement: TargetCounterPlacementNode,
    pub grant: BoundTemporaryKeywordGrantNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LabeledControlledEntryReflexiveCounterNode {
    pub label: AbilityWordMarkerNode,
    pub event: ControlledPermanentEntryEventNode,
    pub antecedent: PutCountersOnObjectEffectNode,
    pub followup: ReflexiveCounterFollowupNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledEntryState {
    Trigger,
    Article,
    Object,
    Event,
    Separator,
    Complete,
}

/// Recognizes `Whenever a PERMANENT you control enters,` independently from
/// the effect that follows it. The permanent predicate and controller scope
/// are delegated to the shared selector automaton.
fn recognize_controlled_permanent_entry_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledPermanentEntryEventNode> {
    let mut state = ControlledEntryState::Trigger;
    let mut object = None;
    loop {
        state = match state {
            ControlledEntryState::Trigger => {
                cursor.enter("event.controlled-permanent-entry.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("whenever"),
                    "the `whenever` trigger",
                )?;
                ControlledEntryState::Article
            }
            ControlledEntryState::Article => {
                cursor.enter("event.controlled-permanent-entry.article");
                let article = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`a` or `an` before a permanent selector"))?
                    .to_string();
                cursor.require(
                    article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an"),
                    "`a` or `an` before a permanent selector",
                )?;
                cursor.consume(&article)?;
                ControlledEntryState::Object
            }
            ControlledEntryState::Object => {
                cursor.enter("event.controlled-permanent-entry.object");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller == Some(ObjectControllerScope::AbilityController),
                    "a permanent controlled by the ability controller",
                )?;
                cursor.require(
                    query.predicate.has_domain(ObjectKind::Permanent),
                    "a selector that establishes the permanent domain",
                )?;
                object = Some(query.predicate);
                ControlledEntryState::Event
            }
            ControlledEntryState::Event => {
                cursor.enter("event.controlled-permanent-entry.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                ControlledEntryState::Separator
            }
            ControlledEntryState::Separator => {
                cursor.enter("event.controlled-permanent-entry.separator");
                cursor.consume(",")?;
                ControlledEntryState::Complete
            }
            ControlledEntryState::Complete => {
                cursor.enter("event.controlled-permanent-entry.complete");
                return Ok(ControlledPermanentEntryEventNode {
                    object: cursor.require_value(object, "a controlled permanent predicate")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterPlacementState {
    Action,
    Count,
    Counter,
    CounterNoun,
    Relation,
    Recipient,
    Complete,
}

fn recognize_positive_literal(
    cursor: &mut OracleTokenCursor<'_>,
    expectation: &'static str,
) -> MachineResult<NumericAtom> {
    let atom = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor
            .peek()
            .ok_or_else(|| cursor.fail(expectation))?
            .to_string();
        cursor.consume(&article)?;
        NumericAtom::Integer(1)
    } else {
        recognize_numeric_atom(cursor)?
    };
    cursor.require(
        matches!(atom, NumericAtom::Integer(value) if value > 0),
        expectation,
    )?;
    Ok(atom)
}

pub(in crate::oracle::canonical) fn recognize_target_counter_placement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCounterPlacementNode> {
    let mut state = CounterPlacementState::Action;
    let mut count = None;
    let mut counter = None;
    let mut recipient = None;
    loop {
        state = match state {
            CounterPlacementState::Action => {
                cursor.enter("effect.counter-placement.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(action.raw.eq_ignore_ascii_case("put"), "the `put` action")?;
                CounterPlacementState::Count
            }
            CounterPlacementState::Count => {
                cursor.enter("effect.counter-placement.count");
                count = Some(recognize_positive_literal(
                    cursor,
                    "a positive literal counter quantity",
                )?);
                CounterPlacementState::Counter
            }
            CounterPlacementState::Counter => {
                cursor.enter("effect.counter-placement.counter");
                counter = Some(recognize_counter_name(cursor)?);
                CounterPlacementState::CounterNoun
            }
            CounterPlacementState::CounterNoun => {
                cursor.enter("effect.counter-placement.counter-noun");
                let singular = matches!(count, Some(NumericAtom::Integer(1)));
                cursor.consume(if singular { "counter" } else { "counters" })?;
                CounterPlacementState::Relation
            }
            CounterPlacementState::Relation => {
                cursor.enter("effect.counter-placement.relation");
                cursor.consume("on")?;
                CounterPlacementState::Recipient
            }
            CounterPlacementState::Recipient => {
                cursor.enter("effect.counter-placement.recipient");
                let targets = recognize_target_list(cursor)?;
                cursor.require(
                    targets.slots.len() == 1
                        && targets.slots[0].cardinality.minimum == 1
                        && targets.slots[0].cardinality.maximum == 1
                        && permanent_target_selector(&targets.slots[0].selector),
                    "exactly one permanent target",
                )?;
                recipient = Some(targets);
                CounterPlacementState::Complete
            }
            CounterPlacementState::Complete => {
                cursor.enter("effect.counter-placement.complete");
                return Ok(TargetCounterPlacementNode {
                    count: cursor.require_value(count, "a counter quantity")?,
                    counter: cursor.require_value(counter, "a counter name")?,
                    targets: cursor.require_value(recipient, "a counter recipient")?,
                });
            }
        };
    }
}

fn permanent_target_selector(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate.has_domain(ObjectKind::Permanent)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(permanent_target_selector)
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterThresholdState {
    If,
    Source,
    Predicate,
    Threshold,
    ComparatorConnector,
    ComparatorDirection,
    Counter,
    CounterNoun,
    Relation,
    RepeatedSource,
    Separator,
    Complete,
}

/// Recognizes `if it has N or more/fewer NAME counters on it,` as an
/// intervening-if condition over the source. Both `it` references require the
/// source binding supplied by the surrounding reflexive sequence.
fn recognize_source_counter_threshold(
    cursor: &mut OracleTokenCursor<'_>,
    source: Option<&LocalObjectReference>,
) -> MachineResult<SourceCounterThresholdNode> {
    let mut state = CounterThresholdState::If;
    let mut threshold = None;
    let mut operator = None;
    let mut counter = None;
    loop {
        state = match state {
            CounterThresholdState::If => {
                cursor.enter("condition.source-counter-threshold.if");
                cursor.consume("if")?;
                CounterThresholdState::Source
            }
            CounterThresholdState::Source => {
                cursor.enter("condition.source-counter-threshold.source");
                cursor.consume("it")?;
                cursor.require(source.is_some(), "a source binding for `it`")?;
                CounterThresholdState::Predicate
            }
            CounterThresholdState::Predicate => {
                cursor.enter("condition.source-counter-threshold.predicate");
                cursor.consume("has")?;
                CounterThresholdState::Threshold
            }
            CounterThresholdState::Threshold => {
                cursor.enter("condition.source-counter-threshold.threshold");
                threshold = Some(recognize_positive_literal(
                    cursor,
                    "a positive literal counter threshold",
                )?);
                CounterThresholdState::ComparatorConnector
            }
            CounterThresholdState::ComparatorConnector => {
                cursor.enter("condition.source-counter-threshold.comparator-connector");
                cursor.consume("or")?;
                CounterThresholdState::ComparatorDirection
            }
            CounterThresholdState::ComparatorDirection => {
                cursor.enter("condition.source-counter-threshold.comparator-direction");
                operator = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("more") => {
                            cursor.consume("more")?;
                            NumericComparisonOperator::GreaterThanOrEqual
                        }
                        Some("fewer") => {
                            cursor.consume("fewer")?;
                            NumericComparisonOperator::LessThanOrEqual
                        }
                        _ => return Err(cursor.fail("`more` or `fewer`")),
                    },
                );
                CounterThresholdState::Counter
            }
            CounterThresholdState::Counter => {
                cursor.enter("condition.source-counter-threshold.counter");
                counter = Some(recognize_counter_name(cursor)?);
                CounterThresholdState::CounterNoun
            }
            CounterThresholdState::CounterNoun => {
                cursor.enter("condition.source-counter-threshold.counter-noun");
                cursor.consume("counters")?;
                CounterThresholdState::Relation
            }
            CounterThresholdState::Relation => {
                cursor.enter("condition.source-counter-threshold.relation");
                cursor.consume("on")?;
                CounterThresholdState::RepeatedSource
            }
            CounterThresholdState::RepeatedSource => {
                cursor.enter("condition.source-counter-threshold.repeated-source");
                cursor.consume("it")?;
                cursor.require(
                    source.is_some(),
                    "the same source binding for the second `it`",
                )?;
                CounterThresholdState::Separator
            }
            CounterThresholdState::Separator => {
                cursor.enter("condition.source-counter-threshold.separator");
                cursor.consume(",")?;
                CounterThresholdState::Complete
            }
            CounterThresholdState::Complete => {
                cursor.enter("condition.source-counter-threshold.complete");
                return Ok(SourceCounterThresholdNode {
                    source: source
                        .cloned()
                        .ok_or_else(|| cursor.fail("a populated source binding"))?,
                    counter: cursor.require_value(counter, "a threshold counter name")?,
                    operator: cursor.require_value(operator, "a counter comparison operator")?,
                    threshold: cursor.require_value(threshold, "a counter threshold")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabeledReflexiveState {
    Label,
    LabelSeparator,
    Event,
    Antecedent,
    AntecedentTerminator,
    ReflexiveTrigger,
    ReflexivePlayer,
    ReflexiveAction,
    ReflexiveSeparator,
    Condition,
    TargetedPlacement,
    TargetedPlacementTerminator,
    TargetAnaphor,
    KeywordAction,
    Keywords,
    Duration,
    Terminator,
    Complete,
}

/// Composes an ability-word-labeled controlled-entry trigger with a counter
/// placement and its rules-defined reflexive continuation. The label remains
/// a marker only; every behavior comes from typed child nodes.
pub(in crate::oracle::canonical) fn recognize_labeled_controlled_entry_reflexive_counter(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LabeledControlledEntryReflexiveCounterNode> {
    let mut state = LabeledReflexiveState::Label;
    let mut label = None;
    let mut event = None;
    let mut antecedent = None;
    let mut condition = None;
    let mut placement = None;
    let mut keywords = None;
    let mut duration = None;
    let target_binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    loop {
        state = match state {
            LabeledReflexiveState::Label => {
                cursor.enter("trigger.labeled-reflexive-counter.label");
                cursor.require(
                    cursor
                        .peek_token()
                        .is_some_and(|token| token.token_type == "abilityWord"),
                    "an ability-word label",
                )?;
                while cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "abilityWord")
                {
                    cursor.consume_type("abilityWord")?;
                }
                label = Some(AbilityWordMarkerNode);
                LabeledReflexiveState::LabelSeparator
            }
            LabeledReflexiveState::LabelSeparator => {
                cursor.enter("trigger.labeled-reflexive-counter.label-separator");
                let separator = cursor.consume_type("separator")?;
                cursor.require(
                    separator.raw == "\u{2014}",
                    "an em dash after the ability word",
                )?;
                LabeledReflexiveState::Event
            }
            LabeledReflexiveState::Event => {
                cursor.enter("trigger.labeled-reflexive-counter.event");
                event = Some(recognize_controlled_permanent_entry_event(cursor)?);
                LabeledReflexiveState::Antecedent
            }
            LabeledReflexiveState::Antecedent => {
                cursor.enter("trigger.labeled-reflexive-counter.antecedent");
                antecedent = Some(recognize_put_counters_effect(
                    cursor,
                    name_links,
                    name_failure,
                    &EffectRecognitionContext::default(),
                )?);
                LabeledReflexiveState::AntecedentTerminator
            }
            LabeledReflexiveState::AntecedentTerminator => {
                cursor.enter("trigger.labeled-reflexive-counter.antecedent-terminator");
                cursor.consume(".")?;
                LabeledReflexiveState::ReflexiveTrigger
            }
            LabeledReflexiveState::ReflexiveTrigger => {
                cursor.enter("trigger.labeled-reflexive-counter.reflexive-trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("when"),
                    "the reflexive `when`",
                )?;
                LabeledReflexiveState::ReflexivePlayer
            }
            LabeledReflexiveState::ReflexivePlayer => {
                cursor.enter("trigger.labeled-reflexive-counter.reflexive-player");
                cursor.consume("you")?;
                LabeledReflexiveState::ReflexiveAction
            }
            LabeledReflexiveState::ReflexiveAction => {
                cursor.enter("trigger.labeled-reflexive-counter.reflexive-action");
                cursor.consume("do")?;
                LabeledReflexiveState::ReflexiveSeparator
            }
            LabeledReflexiveState::ReflexiveSeparator => {
                cursor.enter("trigger.labeled-reflexive-counter.reflexive-separator");
                cursor.consume(",")?;
                LabeledReflexiveState::Condition
            }
            LabeledReflexiveState::Condition => {
                cursor.enter("trigger.labeled-reflexive-counter.condition");
                let source = antecedent
                    .as_ref()
                    .and_then(|placement| match &placement.permanent {
                        super::PermanentOperandNode::Reference(reference) => Some(reference),
                        super::PermanentOperandNode::Collection { .. }
                        | super::PermanentOperandNode::Targets(_)
                        | super::PermanentOperandNode::BoundReference(_) => None,
                    });
                condition = Some(recognize_source_counter_threshold(cursor, source)?);
                LabeledReflexiveState::TargetedPlacement
            }
            LabeledReflexiveState::TargetedPlacement => {
                cursor.enter("trigger.labeled-reflexive-counter.targeted-placement");
                placement = Some(recognize_target_counter_placement(cursor)?);
                LabeledReflexiveState::TargetedPlacementTerminator
            }
            LabeledReflexiveState::TargetedPlacementTerminator => {
                cursor.enter("trigger.labeled-reflexive-counter.targeted-placement-terminator");
                cursor.consume(".")?;
                LabeledReflexiveState::TargetAnaphor
            }
            LabeledReflexiveState::TargetAnaphor => {
                cursor.enter("trigger.labeled-reflexive-counter.target-anaphor");
                cursor.consume("it")?;
                cursor.require(
                    placement.is_some(),
                    "a prior targeted counter-placement binding for `it`",
                )?;
                LabeledReflexiveState::KeywordAction
            }
            LabeledReflexiveState::KeywordAction => {
                cursor.enter("trigger.labeled-reflexive-counter.keyword-action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("gains"),
                    "the `gains` action",
                )?;
                LabeledReflexiveState::Keywords
            }
            LabeledReflexiveState::Keywords => {
                cursor.enter("trigger.labeled-reflexive-counter.keywords");
                keywords = Some(recognize_keyword_list(cursor)?);
                LabeledReflexiveState::Duration
            }
            LabeledReflexiveState::Duration => {
                cursor.enter("trigger.labeled-reflexive-counter.duration");
                duration = Some(recognize_effect_duration(cursor)?);
                LabeledReflexiveState::Terminator
            }
            LabeledReflexiveState::Terminator => {
                cursor.enter("trigger.labeled-reflexive-counter.terminator");
                cursor.consume(".")?;
                LabeledReflexiveState::Complete
            }
            LabeledReflexiveState::Complete => {
                cursor.enter("trigger.labeled-reflexive-counter.complete");
                cursor.finish()?;
                return Ok(LabeledControlledEntryReflexiveCounterNode {
                    label: cursor.require_value(label, "an ability-word marker")?,
                    event: cursor.require_value(event, "a controlled-entry event")?,
                    antecedent: cursor
                        .require_value(antecedent, "an antecedent counter placement")?,
                    followup: ReflexiveCounterFollowupNode {
                        condition: cursor.require_value(condition, "a source counter condition")?,
                        placement: cursor
                            .require_value(placement, "a targeted counter placement")?,
                        grant: BoundTemporaryKeywordGrantNode {
                            object: target_binding,
                            keywords: cursor
                                .require_value(keywords, "one or more granted keywords")?,
                            duration: cursor.require_value(duration, "a keyword duration")?,
                        },
                    },
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "landfall_reflexive_counter_tests.rs"]
mod tests;
