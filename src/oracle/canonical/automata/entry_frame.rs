//! The shared entry-replacement frame (CR 614.1c-e, 614.12, 707.9).
//!
//! ```text
//! EntryFrame := [Permission] SOURCE ENTER ENTERING-STATE [CONDITION] "."
//!               [ "As" "it" "enters" "," CHOICE "." ]
//!             | "As" SOURCE "enters" ["the" "battlefield"] "," BODY
//! Permission := "you" "may" "have"                    (the bare verb `enter`)
//! ENTERING-STATE := ["tapped"] ["with" COUNTERS "on" PRONOUN]
//!                   ["as" "a" "copy" "of" QUERY ["on" "the" "battlefield"]
//!                    ["," "except" EXCEPTIONS]]
//! BODY := CHOICE "."
//!       | "you" "may" COST "." OUTCOME SOURCE "enters" ENTERING-STATE
//!         [CONDITION] "."
//! CHOICE := "choose" DOMAIN
//! ```
//!
//! Every element is an independent typed child: the source is the shared
//! source reference, counters reuse the shared counter name and numeric atom,
//! the copied object is a shared object query, the copy exceptions are the
//! shared copiable-value exception list, and the condition is the shared
//! sentence condition program. An optional cost made as the source enters is
//! the shared payment-action list, its `If you do` / `If you don't` is the
//! shared outcome connector, and the state it decides is the same
//! entering-state reader the plain frame uses. The choice domain is
//! one typed binding (`CharacteristicChoiceBinding`) whose stored value later
//! abilities of the same permanent read (CR 614.12). The frame commits once
//! it has read its source and its entry verb; before that it leaves the
//! ability to its siblings.
use crate::oracle::characteristic_vocabulary::{CardColor, lookup_card_color};
use crate::oracle::counter_name::CounterName;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{ChildOutcome, CommitPoint, MachineResult, OracleTokenCursor};
use super::{
    ActionResultSlot, CostListGrammar, OutcomePerformer, PaymentActionNode, PerformanceOutcome,
    recognize_cost_sequence, recognize_outcome_connector_after,
};
use super::{
    CharacteristicChoiceBinding, EffectRecognitionContext, LocalObjectReference,
    LocalPlayerReference, NumericAtom, NumericExpression, ObjectPredicate, ObjectQueryNode,
    SentenceConditionContext, SourceEntryConditionNode, TokenCopyExceptionsNode,
    recognize_boolean_condition_list_before_separator, recognize_copy_exceptions,
    recognize_counter_name, recognize_numeric_atom, recognize_object_query,
    recognize_sentence_condition_atom, recognize_source_reference,
};

/// Counters a permanent enters with (CR 614.1c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryCounterNode {
    pub counter: CounterName,
    pub count: NumericExpression,
}

/// `as a copy of QUERY[, except ...]` (CR 707.9): the permanent enters with
/// the copiable values of a chosen battlefield permanent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryCopyNode {
    pub object: ObjectQueryNode,
    pub exceptions: TokenCopyExceptionsNode,
}

/// The domain of a choice made as a permanent enters (CR 614.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EntryChoiceDomain {
    /// `a color [other than COLOR]` (CR 105.1).
    Color { excluded: Vec<CardColor> },
    /// `a creature type` (CR 205.3m).
    CreatureType,
    /// `a basic land type` (CR 205.3i).
    BasicLandType,
    /// `a card type` (CR 205.2a).
    CardType,
}

impl EntryChoiceDomain {
    /// The persistent binding later abilities of the permanent read.
    pub(in crate::oracle::canonical) const fn binding(&self) -> CharacteristicChoiceBinding {
        match self {
            Self::Color { .. } => CharacteristicChoiceBinding::Color,
            Self::CreatureType => CharacteristicChoiceBinding::CreatureType,
            Self::BasicLandType => CharacteristicChoiceBinding::BasicLandType,
            Self::CardType => CharacteristicChoiceBinding::CardType,
        }
    }
}

/// One choice made by the entering permanent's controller as it enters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryChoiceNode {
    pub player: LocalPlayerReference,
    pub domain: EntryChoiceDomain,
}

/// What a permanent enters with (CR 614.1c): the state shared by an entry
/// frame and by the outcome of an optional entry cost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EnteringStateNode {
    pub tapped: bool,
    pub counters: Vec<EntryCounterNode>,
    pub copy: Option<EntryCopyNode>,
    /// `if` / `unless` checked as the permanent enters.
    pub condition: Option<SourceEntryConditionNode>,
}

impl EnteringStateNode {
    fn modifies_entry(&self) -> bool {
        self.tapped || !self.counters.is_empty() || self.copy.is_some()
    }
}

/// `you may COST. If you do|don't, SOURCE enters STATE.` read as the source
/// enters (CR 614.12): the cost is the shared payment action, the outcome is
/// the shared connector, and the state applies on that outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryOptionalCostNode {
    pub cost: PaymentActionNode,
    pub outcome: PerformanceOutcome,
    pub state: EnteringStateNode,
}

/// One complete entry replacement of the current source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryFrameNode {
    /// `You may have SOURCE enter ...` (CR 614.1c): applied by choice.
    pub optional: bool,
    pub tapped: bool,
    pub counters: Vec<EntryCounterNode>,
    pub copy: Option<EntryCopyNode>,
    /// `if` / `unless` checked as the permanent enters.
    pub condition: Option<SourceEntryConditionNode>,
    pub choice: Option<EntryChoiceNode>,
    pub optional_cost: Option<EntryOptionalCostNode>,
}

impl EntryFrameNode {
    fn empty() -> Self {
        Self {
            optional: false,
            tapped: false,
            counters: Vec::new(),
            copy: None,
            condition: None,
            choice: None,
            optional_cost: None,
        }
    }
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn is_entry_verb(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("enters") || raw.eq_ignore_ascii_case("enter")
}

/// The current source: a resolved name, `this TYPE`, or a bare `this` whose
/// noun the entry verb replaces (`As this enters`).
fn recognize_entering_source(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalObjectReference> {
    cursor.enter("replacement.entry-frame.source");
    if word_at(cursor, 0, "this") && cursor.peek_ahead(1).is_some_and(is_entry_verb) {
        cursor.consume("this")?;
        return Ok(LocalObjectReference::SourceSelf);
    }
    // `this TYPE` enters only as a permanent (CR 110.1): a nonpermanent noun
    // (`this instant`) names no entering object.
    if word_at(cursor, 0, "this")
        && let Some(noun) = cursor.peek_ahead(1)
        && super::positive_object_selector_atom(noun).is_some_and(|predicate| {
            matches!(
                &predicate,
                ObjectPredicate::CardType(card_type) if !card_type.is_permanent_type()
            ) || predicate == ObjectPredicate::ObjectKind(super::ObjectKind::Spell)
        })
    {
        return Err(cursor.fail("a permanent card type or `permanent` after `this`"));
    }
    recognize_source_reference(cursor, name_links, name_failure)
}

/// `enters` / `enter` and the older `the battlefield` complement.
fn recognize_entry_verb(cursor: &mut OracleTokenCursor<'_>, bare: bool) -> MachineResult<()> {
    cursor.enter("replacement.entry-frame.verb");
    cursor.consume(if bare { "enter" } else { "enters" })?;
    if word_at(cursor, 0, "the") && word_at(cursor, 1, "battlefield") {
        cursor.consume_phrase(&["the", "battlefield"])?;
    }
    Ok(())
}

/// `with COUNT KIND counter(s) on it [and COUNT KIND counter(s) on it]`.
fn recognize_entry_counters(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<EntryCounterNode>> {
    cursor.enter("replacement.entry-frame.counters");
    cursor.consume("with")?;
    let mut counters = Vec::new();
    loop {
        cursor.enter("replacement.entry-frame.counters.quantity");
        let atom = recognize_numeric_atom(cursor)?;
        cursor.require(
            !matches!(atom, NumericAtom::Integer(value) if value <= 0),
            "a positive counter quantity",
        )?;
        if word_at(cursor, 0, "additional") {
            cursor.consume("additional")?;
        }
        cursor.enter("replacement.entry-frame.counters.kind");
        let counter = recognize_counter_name(cursor)?;
        cursor.enter("replacement.entry-frame.counters.noun");
        let singular = matches!(atom, NumericAtom::Integer(1));
        cursor.consume(if singular { "counter" } else { "counters" })?;
        counters.push(EntryCounterNode {
            counter,
            count: NumericExpression::Atom(atom),
        });
        cursor.enter("replacement.entry-frame.counters.connector");
        if word_at(cursor, 0, "and") && !word_at(cursor, 1, "doesn't") {
            cursor.consume("and")?;
            continue;
        }
        break;
    }
    cursor.enter("replacement.entry-frame.counters.recipient");
    cursor.consume("on")?;
    let pronoun = cursor
        .peek()
        .filter(|raw| {
            ["it", "him", "her"]
                .iter()
                .any(|candidate| raw.eq_ignore_ascii_case(candidate))
        })
        .map(str::to_string);
    let pronoun =
        cursor.require_value(pronoun, "the entering source pronoun `it`, `him` or `her`")?;
    cursor.consume(&pronoun)?;
    Ok(counters)
}

/// The end of an object query read inside the frame: before `on the
/// battlefield`, a comma, a period or `from`.
fn query_end(cursor: &OracleTokenCursor<'_>) -> usize {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if matches!(raw, "," | ".")
            || raw.eq_ignore_ascii_case("from")
            || (raw.eq_ignore_ascii_case("on")
                && word_at(cursor, offset + 1, "the")
                && word_at(cursor, offset + 2, "battlefield"))
        {
            break;
        }
        offset += 1;
    }
    cursor.position() + offset
}

/// An indefinite or universal determiner before a chosen object (`a`, `an`,
/// `any`).
fn consume_choice_determiner(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .filter(|raw| {
            ["a", "an", "any"]
                .iter()
                .any(|candidate| raw.eq_ignore_ascii_case(candidate))
        })
        .map(str::to_string);
    let raw = cursor.require_value(raw, "`a`, `an` or `any` before the chosen object")?;
    cursor.consume(&raw)
}

/// `as a copy of QUERY [on the battlefield] [, except EXCEPTIONS]`.
fn recognize_entry_copy(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<EntryCopyNode> {
    cursor.enter("replacement.entry-frame.copy");
    cursor.consume_phrase(&["as", "a", "copy", "of"])?;
    cursor.enter("replacement.entry-frame.copy.object");
    consume_choice_determiner(cursor)?;
    let end = query_end(cursor);
    let object = cursor.bounded(end, recognize_object_query)?;
    cursor.enter("replacement.entry-frame.copy.domain");
    cursor.require(
        object.denotes_battlefield_permanents(),
        "a battlefield permanent to copy (CR 707.2)",
    )?;
    if word_at(cursor, 0, "on") {
        cursor.consume_phrase(&["on", "the", "battlefield"])?;
    }
    let exceptions = if cursor.peek() == Some(",") && word_at(cursor, 1, "except") {
        let exceptions = recognize_copy_exceptions(cursor, false)?;
        // An entering copy has no resolving ability to keep (CR 614.1c).
        cursor.enter("replacement.entry-frame.copy-exception.this-ability");
        cursor.require(
            !exceptions.retains_this_ability,
            "an ability-created copy for `has this ability`",
        )?;
        exceptions
    } else {
        TokenCopyExceptionsNode::default()
    };
    Ok(EntryCopyNode { object, exceptions })
}

/// `choose DOMAIN` as the permanent enters.
pub(in crate::oracle::canonical) fn recognize_entry_choice(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EntryChoiceNode> {
    cursor.enter("replacement.entry-choice.action");
    cursor.consume("choose")?;
    cursor.enter("replacement.entry-choice.article");
    consume_choice_determiner(cursor)?;
    cursor.enter("replacement.entry-choice.domain");
    let domain = if word_at(cursor, 0, "color") {
        cursor.consume("color")?;
        let mut excluded = Vec::new();
        if word_at(cursor, 0, "other") && word_at(cursor, 1, "than") {
            cursor.enter("replacement.entry-choice.color-exclusion");
            cursor.consume_phrase(&["other", "than"])?;
            let color = cursor.peek().and_then(lookup_card_color);
            let color = cursor.require_value(color, "a color the choice excludes")?;
            cursor.consume(color.oracle_name())?;
            excluded.push(color);
        }
        EntryChoiceDomain::Color { excluded }
    } else if word_at(cursor, 0, "creature") && word_at(cursor, 1, "type") {
        cursor.consume_phrase(&["creature", "type"])?;
        EntryChoiceDomain::CreatureType
    } else if word_at(cursor, 0, "basic")
        && word_at(cursor, 1, "land")
        && word_at(cursor, 2, "type")
    {
        cursor.consume_phrase(&["basic", "land", "type"])?;
        EntryChoiceDomain::BasicLandType
    } else if word_at(cursor, 0, "card") && word_at(cursor, 1, "type") {
        cursor.consume_phrase(&["card", "type"])?;
        EntryChoiceDomain::CardType
    } else {
        return Err(cursor.fail(
            "a color, creature type, basic land type or card type stored on the entering permanent (CR 614.12)",
        ));
    };
    Ok(EntryChoiceNode {
        player: LocalPlayerReference::Controller,
        domain,
    })
}

/// `if` / `unless` CONDITION, checked as the permanent enters. No effect has
/// introduced an object yet, so the shared atoms bind no sequence antecedent.
fn recognize_entry_condition(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<Option<SourceEntryConditionNode>> {
    cursor.enter("replacement.entry-frame.condition");
    let unless = if word_at(cursor, 0, "unless") {
        true
    } else if word_at(cursor, 0, "if") {
        false
    } else {
        return Ok(None);
    };
    cursor.consume(if unless { "unless" } else { "if" })?;
    let effects = EffectRecognitionContext::default();
    let context = SentenceConditionContext {
        name_links,
        name_failure,
        effects: &effects,
        card_target: None,
        card_focus: false,
        departed_targets: &[],
    };
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, &context)
    })?;
    Ok(Some(SourceEntryConditionNode { condition, unless }))
}

/// A closing reminder restating the rule of a counter whose effect the
/// counter itself carries (CR 122.1d: a stun counter replaces untapping).
/// It adds no rule, so it is read only after such a counter, as the last
/// parenthesized group of the ability.
fn recognize_intrinsic_counter_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    counters: &[EntryCounterNode],
) -> MachineResult<()> {
    cursor.enter("replacement.entry-frame.counter-reminder");
    cursor.require(
        counters
            .iter()
            .any(|counter| counter.counter.as_str() == "stun"),
        "a reminder only after a counter with an intrinsic rule (CR 122.1d)",
    )?;
    cursor.consume("(")?;
    let mut depth = 1_usize;
    while depth > 0 {
        let raw = cursor
            .peek()
            .ok_or_else(|| cursor.fail("the closing parenthesis of the reminder"))?
            .to_string();
        match raw.as_str() {
            "(" => depth += 1,
            ")" => depth -= 1,
            _ => {}
        }
        cursor.consume(&raw)?;
    }
    Ok(())
}

/// `As it enters, choose DOMAIN.` after an entering-state sentence: the same
/// replacement, its pronoun bound to the source of the first sentence.
fn recognize_trailing_entry_choice(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<EntryChoiceNode>> {
    cursor.enter("replacement.entry-frame.trailing-choice");
    if cursor.peek().is_none() {
        return Ok(None);
    }
    cursor.consume_phrase(&["as", "it", "enters", ","])?;
    let choice = recognize_entry_choice(cursor)?;
    cursor.enter("replacement.entry-frame.trailing-choice.terminator");
    cursor.consume(".")?;
    Ok(Some(choice))
}

/// `ENTERING-STATE [CONDITION]` after `SOURCE enter(s)`: the one reader of
/// what a permanent enters with, shared by every entry frame.
fn recognize_entering_state(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EnteringStateNode> {
    let mut state = EnteringStateNode::default();
    cursor.enter("replacement.entry-frame.state");
    if word_at(cursor, 0, "tapped") {
        cursor.consume("tapped")?;
        state.tapped = true;
    }
    if word_at(cursor, 0, "with") {
        state.counters = recognize_entry_counters(cursor)?;
    }
    if word_at(cursor, 0, "as") && word_at(cursor, 1, "a") && word_at(cursor, 2, "copy") {
        state.copy = Some(recognize_entry_copy(cursor)?);
    }
    cursor.enter("replacement.entry-frame.state.coordination");
    cursor.require(
        !(word_at(cursor, 0, "and") && word_at(cursor, 1, "doesn't")),
        "an entering state without a coordinated untap restriction, which is a separate static ability (CR 302.6)",
    )?;
    cursor.require(
        !word_at(cursor, 0, "under"),
        "an entering state without a controller on entry",
    )?;
    state.condition = recognize_entry_condition(cursor, name_links, name_failure)?;
    cursor.enter("replacement.entry-frame.state.complete");
    cursor.require(
        state.modifies_entry(),
        "an entering state: `tapped`, counters or a copy (CR 614.1c)",
    )?;
    Ok(state)
}

/// `[You may have] SOURCE enter(s) STATE [CONDITION].` followed by an
/// optional `As it enters, choose DOMAIN.`
fn recognize_entering_state_frame(
    cursor: &mut OracleTokenCursor<'_>,
    commit: &mut CommitPoint,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EntryFrameNode> {
    let mut frame = EntryFrameNode::empty();
    cursor.enter("replacement.entry-frame.permission");
    if word_at(cursor, 0, "you") {
        cursor.consume_phrase(&["you", "may", "have"])?;
        frame.optional = true;
    }
    let source = recognize_entering_source(cursor, name_links, name_failure)?;
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the current source entering the battlefield",
    )?;
    recognize_entry_verb(cursor, frame.optional)?;
    commit.commit();
    let state = recognize_entering_state(cursor, name_links, name_failure)?;
    frame.tapped = state.tapped;
    frame.counters = state.counters;
    frame.copy = state.copy;
    frame.condition = state.condition;
    cursor.require(
        !(frame.optional && frame.condition.is_some()),
        "one applicability for an optional entry replacement",
    )?;
    cursor.enter("replacement.entry-frame.terminator");
    cursor.consume(".")?;
    if cursor.peek() == Some("(") {
        recognize_intrinsic_counter_reminder(cursor, &frame.counters)?;
    }
    if frame.copy.is_none() && !frame.optional && frame.condition.is_none() {
        frame.choice = recognize_trailing_entry_choice(cursor)?;
    }
    cursor.enter("replacement.entry-frame.complete");
    cursor.finish()?;
    Ok(frame)
}

/// `you may COST. OUTCOME, SOURCE enters STATE [CONDITION].`: the optional
/// cost is the shared payment-action list bounded to its sentence, the
/// outcome is the shared connector with the controller as its performer, and
/// the state is the shared entering-state reader. The parent's lowering
/// decides which costs the entry replacement can apply.
fn recognize_optional_entry_cost(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EntryOptionalCostNode> {
    cursor.enter("replacement.entry-cost.permission");
    cursor.consume_phrase(&["you", "may"])?;
    cursor.enter("replacement.entry-cost.cost");
    let mut sequence = recognize_cost_sequence(
        cursor,
        CostListGrammar {
            comma: false,
            and: false,
        },
    )?;
    cursor.require(sequence.costs.len() == 1, "one optional entry cost")?;
    let cost = sequence.costs.remove(0);
    cursor.enter("replacement.entry-cost.terminator");
    cursor.consume(".")?;
    cursor.enter("replacement.entry-cost.outcome");
    let outcome = recognize_outcome_connector_after(
        cursor,
        ActionResultSlot {
            performer: OutcomePerformer::Controller,
        },
    )?;
    cursor.require(
        outcome != PerformanceOutcome::Reflexive,
        "`If you do` or `If you don't` deciding the entering state",
    )?;
    cursor.enter("replacement.entry-cost.outcome-source");
    let source = recognize_entering_source(cursor, name_links, name_failure)?;
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the same entering source",
    )?;
    recognize_entry_verb(cursor, false)?;
    let state = recognize_entering_state(cursor, name_links, name_failure)?;
    cursor.enter("replacement.entry-cost.outcome-terminator");
    cursor.consume(".")?;
    Ok(EntryOptionalCostNode {
        cost,
        outcome,
        state,
    })
}

/// `As SOURCE enters [the battlefield], BODY`.
fn recognize_as_enters_frame(
    cursor: &mut OracleTokenCursor<'_>,
    commit: &mut CommitPoint,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EntryFrameNode> {
    let mut frame = EntryFrameNode::empty();
    cursor.enter("replacement.entry-frame.as");
    cursor.consume("as")?;
    let source = recognize_entering_source(cursor, name_links, name_failure)?;
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the current source entering the battlefield",
    )?;
    recognize_entry_verb(cursor, false)?;
    cursor.enter("replacement.entry-frame.as.separator");
    cursor.consume(",")?;
    commit.commit();
    cursor.enter("replacement.entry-frame.body");
    if word_at(cursor, 0, "choose") {
        frame.choice = Some(recognize_entry_choice(cursor)?);
        cursor.enter("replacement.entry-choice.terminator");
        cursor.consume(".")?;
    } else if word_at(cursor, 0, "you") && word_at(cursor, 1, "may") {
        frame.optional_cost = Some(recognize_optional_entry_cost(
            cursor,
            name_links,
            name_failure,
        )?);
    } else {
        return Err(
            cursor.fail("a choice or an optional cost made as the source enters (CR 614.12)")
        );
    }
    cursor.enter("replacement.entry-frame.complete");
    cursor.finish()?;
    Ok(frame)
}

/// Reads one complete entry replacement of the current source. The frame
/// commits after its source and entry verb; a different ability shape is a
/// `NoMatch` the caller leaves to its siblings.
pub(in crate::oracle::canonical) fn try_recognize_entry_frame(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> ChildOutcome<EntryFrameNode> {
    cursor.transaction(|cursor, commit| {
        if word_at(cursor, 0, "as") {
            recognize_as_enters_frame(cursor, commit, name_links, name_failure)
        } else {
            recognize_entering_state_frame(cursor, commit, name_links, name_failure)
        }
    })
}

#[cfg(test)]
#[path = "entry_frame_tests.rs"]
mod tests;
