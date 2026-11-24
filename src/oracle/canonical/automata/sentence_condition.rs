//! The one condition atom reader and the sentence-level conditional
//! instruction `If CONDITION, EFFECTS.` with an optional `Otherwise,
//! EFFECTS.` sentence (CR 608.2c).
//!
//! `recognize_condition_atom` is the only atom grammar of the condition
//! language (`condition.rs`). It reads subject first: a player subject
//! (`you`, `an opponent`, `each player`), an object subject (the source, a
//! referenced object), an object quantity (`a creature`, `no spells`), the
//! current turn (`it's your turn`), or a numeric operand (`X`, `that
//! creature's power`). Every parent supplies a `ConditionContext` whose scope
//! is the allow-list of atom families its evaluation moment can resolve.
//!
//! The effect-sequence parent owns the introducer position, the sentence
//! boundaries and the branch ranges; this child owns only the Boolean
//! program between `if` and its comma. A referenced object is a symbolic
//! binding resolved from the sequence's proven antecedents; it is never
//! guessed.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// The object an object-state condition tests. Bindings stay symbolic until
/// lowering resolves them to the declared target or the bound card result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionObjectReference {
    /// A battlefield permanent: the source, a declared target, or the object
    /// observed by the triggering event.
    Permanent(PermanentEffectReference),
    /// A card declared as the target of an earlier card zone change.
    CardTarget(TargetBinding),
    /// The singular card result of the current card-collection scope (the
    /// looked-at, revealed or chosen card).
    CardFocus,
    /// A declared permanent target that an earlier action of the program
    /// moved off the battlefield, read with its last known information
    /// (CR 608.2h): the numeric properties and the type line its departure
    /// recorded.
    DepartedTarget(TargetBinding),
}

impl ConditionObjectReference {
    /// The ability's own source.
    pub(in crate::oracle::canonical) const SOURCE: Self = Self::Permanent(
        PermanentEffectReference::Source(LocalObjectReference::SourceSelf),
    );
}

/// The Boolean program of a sentence conditional, an intervening `if` or a
/// static `as long as`: one program over the one atom grammar.
pub(in crate::oracle::canonical) type SentenceConditionNode = ConditionNode;

/// A conditional instruction over the flat action list of its sequence.
/// `then_first..then_end` are the actions of the `If` sentence and
/// `then_end..else_end` those of its `Otherwise` sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SentenceConditionalNode {
    pub condition: SentenceConditionNode,
    pub then_first: usize,
    pub then_end: usize,
    pub else_end: Option<usize>,
}

impl SentenceConditionalNode {
    pub(in crate::oracle::canonical) fn end(&self) -> usize {
        self.else_end.unwrap_or(self.then_end)
    }

    pub(in crate::oracle::canonical) fn shifted(&self, offset: usize) -> Self {
        Self {
            condition: self.condition.clone(),
            then_first: self.then_first + offset,
            then_end: self.then_end + offset,
            else_end: self.else_end.map(|end| end + offset),
        }
    }
}

/// Antecedents proven by the enclosing sequence at the condition's position.
pub(in crate::oracle::canonical) struct SentenceConditionContext<'a> {
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    pub effects: &'a EffectRecognitionContext,
    /// The singular card declared by the latest card-target zone change.
    pub card_target: Option<TargetBinding>,
    /// Whether the latest object-introducing action is a card-collection
    /// action whose singular result `it` may denote (checked at lowering).
    pub card_focus: bool,
    /// Permanent targets that an earlier action of this sequence moved out of
    /// the battlefield. Testing them needs last-known information.
    pub departed_targets: &'a [TargetBinding],
}

/// What a condition parent supplies to the one atom reader.
#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) struct ConditionContext<'a> {
    /// The allow-list of atom families of the parent's evaluation moment.
    pub scope: ConditionScope,
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    /// The antecedents of an effect-program position; absent outside a
    /// program, where `it` can only denote the source (`source_subject`).
    pub program: Option<&'a SentenceConditionContext<'a>>,
    /// True when `it` denotes the ability's own source (the subject of a
    /// static ability is its source).
    pub source_subject: bool,
    /// The optional payment whose omission `you don't` reads.
    pub payment: Option<ChoiceBinding>,
    /// How the ability's source is attached, which `enchanted PERMANENT` /
    /// `equipped creature` names (CR 301.5a, 303.4a).
    pub source_attachment: Option<AttachedObjectRole>,
    /// Outside a program, the object `it` denotes when it is not the source
    /// (the attached permanent a static ability's subject names).
    pub subject_object: Option<ConditionObjectReference>,
}

impl<'a> ConditionContext<'a> {
    pub(in crate::oracle::canonical) fn standalone(
        scope: ConditionScope,
        name_links: &'a [ResolvedNameLink],
        name_failure: Option<&'a NameResolutionFailure>,
        source_subject: bool,
    ) -> Self {
        Self {
            scope,
            name_links,
            name_failure,
            program: None,
            source_subject,
            payment: None,
            source_attachment: None,
            subject_object: None,
        }
    }

    pub(in crate::oracle::canonical) fn program(
        scope: ConditionScope,
        context: &'a SentenceConditionContext<'a>,
    ) -> Self {
        Self {
            scope,
            name_links: context.name_links,
            name_failure: context.name_failure,
            program: Some(context),
            source_subject: false,
            payment: None,
            source_attachment: context.effects.source_attachment,
            subject_object: None,
        }
    }

    fn atom_kind_state(&self) -> &'static str {
        match self.scope {
            ConditionScope::Static => "condition.static.atom-kind",
            ConditionScope::Program | ConditionScope::TriggerGuard => {
                "condition.sentence.atom-kind"
            }
            ConditionScope::Unless => "condition.unless.atom-kind",
            ConditionScope::Only(_) => "condition.envelope.atom-kind",
        }
    }
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn contraction_is(cursor: &OracleTokenCursor<'_>, offset: usize, base: &str, tail: &str) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        let raw = raw.replace('\u{2019}', "'").to_ascii_lowercase();
        raw == format!("{base}'{tail}")
    })
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a token"))?
        .to_string();
    cursor.consume(&raw)
}

/// True at a sentence-initial `if` whose condition this child may read.
pub(in crate::oracle::canonical) fn sentence_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "if")
}

/// Reads `if CONDITION,`. The comma is consumed here because the sequence
/// parent has already proved that this `if` opens its sentence.
pub(in crate::oracle::canonical) fn recognize_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionNode> {
    cursor.enter("condition.sentence.if");
    cursor.consume("if")?;
    cursor.enter("condition.sentence.boolean-program");
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, context)
    })?;
    cursor.enter("condition.sentence.separator");
    cursor.consume(",")?;
    cursor.enter("condition.sentence.instead");
    cursor.require(
        !word_is(cursor, 0, "instead"),
        "a conditional sentence that does not replace an earlier instruction with `instead`",
    )?;
    Ok(condition)
}

/// Reads a postposed `if CONDITION` (`Draw a card if you control a
/// Wizard.`, #11 Branch): the same condition as the preposed form, without
/// its comma; the sequence parent owns the sentence end and the gated range.
pub(in crate::oracle::canonical) fn recognize_postposed_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionNode> {
    cursor.enter("condition.sentence.postposed-if");
    cursor.consume("if")?;
    cursor.enter("condition.sentence.boolean-program");
    recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, context)
    })
}

/// One atom at an effect-program position (every family).
pub(in crate::oracle::canonical) fn recognize_sentence_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    recognize_condition_atom(
        cursor,
        &ConditionContext::program(ConditionScope::Program, context),
    )
}

/// The one atom grammar of the condition language. The atom is read subject
/// first; the parent's scope is checked on the typed result.
pub(in crate::oracle::canonical) fn recognize_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter(context.atom_kind_state());
    let atom = read_condition_atom(cursor, context)?;
    cursor.enter("condition.allow-list");
    cursor.require(context.scope.allows_atom(&atom), context.scope.refusal())?;
    Ok(atom)
}

fn read_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    // ActionOutcome: `you can't`, `you don't`.
    if word_is(cursor, 0, "you")
        && (contraction_is(cursor, 1, "can", "t") || word_is(cursor, 1, "cannot"))
    {
        cursor.enter("condition.action-outcome.previous-action");
        cursor.consume("you")?;
        consume_current(cursor)?;
        return Ok(ConditionAtom::ActionOutcome(
            ActionOutcomeNode::PreviousActionImpossible,
        ));
    }
    // `you don't control QUERY` is the negated control proposition, the
    // absence count of the shared controlled-permanent query, not a denied
    // payment.
    if word_is(cursor, 0, "you")
        && contraction_is(cursor, 1, "don", "t")
        && word_is(cursor, 2, "control")
    {
        return recognize_controlled_quantity_atom(cursor);
    }
    if word_is(cursor, 0, "you") && contraction_is(cursor, 1, "don", "t") {
        cursor.enter("condition.action-outcome.payment-omitted");
        let payment = cursor.require_value(
            context.payment,
            "an optional payment bound by the parent that `you don't` denies",
        )?;
        cursor.consume("you")?;
        consume_current(cursor)?;
        return Ok(ConditionAtom::ActionOutcome(
            ActionOutcomeNode::CostNotPaid { payment },
        ));
    }
    // EventHistory read by the shared past life clause.
    if let Some(atom) = super::try_recognize_life_history_atom(cursor)? {
        return Ok(atom);
    }
    // Compare: two controlled-permanent counts of different players.
    if super::opponent_count_comparison_starts(cursor) {
        return super::recognize_opponent_count_comparison_atom(cursor);
    }
    // Compare: the controller's permanents.
    if word_is(cursor, 0, "you") && word_is(cursor, 1, "control") {
        return recognize_controlled_permanents_atom(cursor);
    }
    // Compare: one measure of two players (`an opponent has more cards in
    // hand than you`, `you have more life than each opponent`).
    if super::player_comparative_starts(cursor) {
        return super::recognize_player_comparative_atom(cursor);
    }
    // Compare: the state of the player the binding environment names.
    if word_is(cursor, 0, "that")
        && word_is(cursor, 1, "player")
        && (word_is(cursor, 2, "controls") || word_is(cursor, 2, "has"))
    {
        return recognize_bound_player_state_atom(cursor);
    }
    // Compare: a player population's state.
    if super::collective_opponents_condition_starts(cursor) {
        return super::recognize_collective_opponents_condition(cursor).map(player_state_atom);
    }
    if super::quantified_player_condition_starts(cursor) {
        return super::recognize_quantified_player_condition(cursor).map(player_state_atom);
    }
    // CastFact: `you cast it`, `you cast this spell from your hand`.
    if word_is(cursor, 0, "you")
        && word_is(cursor, 1, "cast")
        && cast_object_follows(cursor, context)
    {
        return recognize_controller_cast_source_atom(cursor, context);
    }
    // CastFact: `{R} was spent to cast this spell`, `at least three red mana
    // was spent to cast it`, `no mana was spent to cast that spell`.
    if mana_spent_atom_starts(cursor) {
        return recognize_mana_spent_atom(cursor, context);
    }
    // EventHistory with a player actor: `an opponent searched their library
    // this turn`, `a player cast two or more spells last turn`.
    if super::actor_event_history_starts(cursor) {
        return super::recognize_actor_event_history_atom(cursor);
    }
    // Compare / PlayerDesignation with the controller as subject.
    if contraction_is(cursor, 0, "you", "ve")
        || (word_is(cursor, 0, "you")
            && (word_is(cursor, 1, "have") || contraction_is(cursor, 1, "haven", "t")))
    {
        return super::recognize_controller_history_atom(cursor);
    }
    // EventHistory with an object quantity: `a creature died this turn`, `no
    // spells were cast last turn`.
    if super::object_event_history_starts(cursor) {
        return super::recognize_object_event_history_atom(cursor);
    }
    // ObjectSatisfies: `there are no KIND counters on OBJECT` (CR 122.1).
    if existential_counter_follows(cursor) {
        return recognize_existential_counter_atom(cursor, context)
            .map(ConditionAtom::ObjectSatisfies);
    }
    // Compare: `there are N or more CARDS in your graveyard`.
    if word_is(cursor, 0, "there") || contraction_is(cursor, 0, "there", "s") {
        return super::recognize_graveyard_count_atom(cursor);
    }
    // Compare: `X is N or more`, `that creature's power is greater than ...`.
    if word_is(cursor, 0, "x") && word_is(cursor, 1, "is") {
        return recognize_numeric_comparison_atom(cursor);
    }
    if object_property_operand_starts(cursor) {
        return recognize_numeric_relation_atom(cursor);
    }
    // TurnOwnership.
    if contraction_is(cursor, 0, "it", "s")
        && (word_is(cursor, 1, "your") || (word_is(cursor, 1, "not") && word_is(cursor, 2, "your")))
    {
        return recognize_controller_turn_proposition(cursor).map(ConditionAtom::TurnOwnership);
    }
    // ObjectSatisfies with the attached permanent as subject (`equipped
    // creature is a Human`, `enchanted creature is red`).
    if super::attached_object_reference_starts(cursor) {
        return recognize_attached_object_atom(cursor, context).map(ConditionAtom::ObjectSatisfies);
    }
    // ObjectSatisfies / CastFact with the source as subject.
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        context.name_links,
        context.name_failure,
    ) {
        return recognize_source_condition_atom(cursor, context);
    }
    // ObjectSatisfies with a referenced object as subject.
    if contraction_is(cursor, 0, "it", "s")
        || word_is(cursor, 0, "it")
        || word_is(cursor, 0, "its")
        || word_is(cursor, 0, "that")
    {
        return recognize_referenced_object_atom(cursor, context)
            .map(ConditionAtom::ObjectSatisfies);
    }
    // Compare: a numeric subject of the shared definition grammar (`your
    // devotion to white is less than five`, `your life total is 5 or less`).
    if let Some(atom) = try_numeric_subject_comparison(cursor)? {
        return Ok(atom);
    }
    Err(cursor.fail(
        "a supported condition: a player's state or history, a count comparison, an object's state, turn ownership, a cast fact of this spell, or the outcome of an earlier action",
    ))
}

/// `EXPRESSION is COMPARISON`, where EXPRESSION is any operand of the
/// shared numeric definition grammar. The subject is read transactionally:
/// only an expression followed by `is|was` commits this reader.
fn try_numeric_subject_comparison(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ConditionAtom>> {
    let mut trial = cursor.clone();
    let Ok(left) = recognize_variable_definition_expression(&mut trial) else {
        return Ok(None);
    };
    if !(word_is(&trial, 0, "is") || word_is(&trial, 0, "was")) {
        return Ok(None);
    }
    *cursor = trial;
    cursor.enter("condition.numeric-subject.copula");
    consume_current(cursor)?;
    let (operator, right, halved) = recognize_comparison_tail(cursor)?;
    // `L OP half R` is `2 * L OP R`: the comparison needs no rounding.
    let left = if halved {
        NumericExpression::multiply(NumericExpression::from(2), left)
    } else {
        left
    };
    Ok(Some(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(left),
        operator,
        right,
    })))
}

/// After the copula: `less than [or equal to] R`, `greater than [or equal
/// to] R`, `equal to R`, a threshold (`5 or less`, `at least 10`, `exactly
/// 20`) or a bare number. R is a number or an expression of the shared
/// definition grammar, optionally `half R` (returned as `true`).
fn recognize_comparison_tail(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(NumericComparisonOperator, ConditionNumber, bool)> {
    cursor.enter("condition.numeric-subject.operator");
    let relation = if word_is(cursor, 0, "less") && word_is(cursor, 1, "than") {
        Some(if word_is(cursor, 2, "or") {
            NumericComparisonOperator::LessThanOrEqual
        } else {
            NumericComparisonOperator::LessThan
        })
    } else if word_is(cursor, 0, "greater") && word_is(cursor, 1, "than") {
        Some(if word_is(cursor, 2, "or") {
            NumericComparisonOperator::GreaterThanOrEqual
        } else {
            NumericComparisonOperator::GreaterThan
        })
    } else if word_is(cursor, 0, "equal") && word_is(cursor, 1, "to") {
        Some(NumericComparisonOperator::Equal)
    } else {
        None
    };
    if let Some(operator) = relation {
        consume_current(cursor)?;
        consume_current(cursor)?;
        if matches!(
            operator,
            NumericComparisonOperator::LessThanOrEqual
                | NumericComparisonOperator::GreaterThanOrEqual
        ) {
            cursor.consume_phrase(&["or", "equal", "to"])?;
        }
        cursor.enter("condition.numeric-subject.right");
        let halved = word_is(cursor, 0, "half");
        if halved {
            cursor.consume("half")?;
        }
        let number = cursor
            .peek()
            .and_then(|raw| raw.parse::<i64>().ok().or_else(|| parse_number_word(raw)))
            .filter(|_| !word_is(cursor, 0, "a") && !word_is(cursor, 0, "an"));
        let right = match number {
            Some(value) if !halved => {
                consume_current(cursor)?;
                ConditionNumber::Integer(value)
            }
            _ => ConditionNumber::Expression(recognize_variable_definition_expression(cursor)?),
        };
        return Ok((operator, right, halved));
    }
    let bare = cursor
        .peek()
        .and_then(|raw| raw.parse::<i64>().ok().or_else(|| parse_number_word(raw)))
        .filter(|_| !word_is(cursor, 1, "or"));
    if let Some(value) = bare {
        consume_current(cursor)?;
        return Ok((
            NumericComparisonOperator::Equal,
            ConditionNumber::Integer(value),
            false,
        ));
    }
    let threshold = recognize_count_threshold(cursor, false)?;
    cursor.require(
        !(threshold.operator == ThresholdOperator::AtMost && threshold.value == 0),
        "a numeric threshold rather than `no`",
    )?;
    let (operator, value) = threshold_comparison(threshold);
    Ok((operator, ConditionNumber::Integer(value), false))
}

/// `you control a|an|another PERMANENT` (at least one) or `you control N or
/// more|fewer [other] PERMANENTS`: one comparison over the controller's
/// permanents, shared by every condition parent.
pub(in crate::oracle::canonical) fn recognize_controlled_permanents_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.controlled-permanents.kind");
    // `you control no QUERY` / `you control exactly N QUERY`: the absence or
    // exact count read by the shared threshold (CR 109.4 control).
    if word_is(cursor, 2, "no") || word_is(cursor, 2, "exactly") {
        return recognize_controlled_quantity_atom(cursor);
    }
    let presence = cursor.peek_ahead(2).is_some_and(|word| {
        word.eq_ignore_ascii_case("a")
            || word.eq_ignore_ascii_case("an")
            || word.eq_ignore_ascii_case("another")
    });
    let query = if presence {
        recognize_controlled_permanent_presence_query(cursor)?
    } else {
        recognize_controlled_permanent_count_query(cursor)?
    };
    Ok(query.into_atom())
}

/// `you control THRESHOLD [other] QUERY` with a quantifier the fixed count
/// query does not read (`no`, `exactly N`), or its negation `you don't
/// control a|another QUERY`: the controller's count of the shared permanent
/// query compared with the shared threshold.
fn recognize_controlled_quantity_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.controlled-permanents.quantity.subject");
    cursor.consume("you")?;
    let negated = contraction_is(cursor, 0, "don", "t");
    if negated {
        consume_current(cursor)?;
    }
    cursor.consume("control")?;
    cursor.enter("condition.controlled-permanents.quantity.threshold");
    // `you don't control a|an|another QUERY` (CR 109.4): no permanent of the
    // query; `another` is the query's source exclusion.
    let mut another = false;
    let threshold = if negated {
        if word_is(cursor, 0, "a") || word_is(cursor, 0, "an") {
            consume_current(cursor)?;
        } else if word_is(cursor, 0, "another") {
            consume_current(cursor)?;
            another = true;
        } else {
            // `you don't control two or more QUERY` negates a count
            // threshold (fewer than two), not the absence of the query.
            let counted = cursor.peek().is_some_and(|raw| {
                ["exactly", "at", "no"]
                    .iter()
                    .any(|word| raw.eq_ignore_ascii_case(word))
                    || raw.parse::<i64>().is_ok()
                    || parse_number_word(raw).is_some()
            });
            cursor.require(
                !counted,
                "an absence (`a`, `an`, `another`) after `you don't control`, not a negated count",
            )?;
        }
        CountThreshold {
            operator: ThresholdOperator::AtMost,
            value: 0,
        }
    } else {
        recognize_count_threshold(cursor, false)?
    };
    cursor.enter("condition.controlled-permanents.quantity.query");
    let mut query = super::recognize_query_excluding_source(cursor, recognize_object_query)?;
    cursor.require(
        !(another && query.exclude_source),
        "one source exclusion for one object query",
    )?;
    query.exclude_source |= another;
    cursor.require(
        query.controller.is_none(),
        "an unscoped permanent selector after `you control`",
    )?;
    let exclude_source = query.exclude_source;
    let controlled = ObjectQueryNode {
        controller: Some(ObjectControllerScope::AbilityController),
        ..query
    };
    cursor.require(
        controlled.denotes_battlefield_permanents(),
        "a selector that establishes the permanent domain",
    )?;
    Ok(ConditionAtom::Compare(ConditionComparison::measured(
        ConditionPlayerSubject::Controller,
        PlayerStateMeasure::ControlledPermanents {
            predicate: controlled.predicate,
            exclude_source,
        },
        threshold,
    )))
}

/// `that player controls|has ...`: a player-state proposition whose subject
/// is resolved by the one binding-environment resolver (`recognize_that_player`).
/// Only the player the triggering event records has a condition lowering; a
/// player declared by the program is refused instead of being re-read.
fn recognize_bound_player_state_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.bound-player.subject");
    let environment = installed_binding_environment(cursor);
    let subject = match recognize_that_player(cursor, &environment)? {
        BoundPlayerReference::EventPlayer => ConditionPlayerSubject::EventPlayer,
        _ => {
            cursor.enter("condition.bound-player.binding");
            return Err(cursor.fail(
                "the player the triggering event records; a declared player is not read by a condition",
            ));
        }
    };
    cursor.enter("condition.bound-player.verb");
    if word_is(cursor, 0, "has") {
        cursor.consume("has")?;
        let proposition = recognize_player_possession_proposition(cursor, subject)?;
        return Ok(player_state_atom(PlayerStateConditionNode {
            subject,
            proposition,
        }));
    }
    cursor.consume("controls")?;
    if word_is(cursor, 0, "more") || word_is(cursor, 0, "fewer") {
        // `that player controls more QUERY than you`: two counts of one
        // query, the bound player's and the controller's.
        cursor.enter("condition.bound-player.comparative");
        let more = word_is(cursor, 0, "more");
        consume_current(cursor)?;
        let query = recognize_object_query(cursor)?;
        cursor.require(
            query.controller.is_none(),
            "a permanent query without its own controller relation",
        )?;
        cursor.require(
            query.predicate.supports_domain(ObjectKind::Permanent),
            "a predicate valid for a battlefield permanent",
        )?;
        cursor.enter("condition.bound-player.comparative.than");
        cursor.consume("than")?;
        cursor.consume("you")?;
        let measure = || PlayerStateMeasure::ControlledPermanents {
            predicate: query.predicate.clone(),
            exclude_source: false,
        };
        return Ok(ConditionAtom::Compare(ConditionComparison {
            subject,
            left: ConditionNumber::Player(measure()),
            operator: if more {
                NumericComparisonOperator::GreaterThan
            } else {
                NumericComparisonOperator::LessThan
            },
            right: ConditionNumber::ControllerMeasure(measure()),
        }));
    }
    let proposition = recognize_controlled_permanents_proposition(cursor)?;
    Ok(player_state_atom(PlayerStateConditionNode {
        subject,
        proposition,
    }))
}

/// A player-state proposition is a comparison over a measure of its subject.
pub(in crate::oracle::canonical) fn player_state_atom(
    node: PlayerStateConditionNode,
) -> ConditionAtom {
    ConditionAtom::Compare(ConditionComparison::measured(
        node.subject,
        node.proposition.measure,
        node.proposition.threshold,
    ))
}

/// After `you cast`: a pronoun or a reference to the ability's own source
/// (a cast fact), rather than a counted spell query (an event history).
fn cast_object_follows(cursor: &OracleTokenCursor<'_>, context: &ConditionContext<'_>) -> bool {
    ["it", "him", "her", "them"]
        .iter()
        .any(|pronoun| word_is(cursor, 2, pronoun))
        || source_reference_starts_at(
            cursor.position() + 2,
            cursor.peek_ahead(2),
            context.name_links,
            context.name_failure,
        )
}

/// A genitive property operand starts here: `that NOUN's power` (the binding
/// environment's object) or `this NOUN's power` (the source).
fn object_property_operand_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    super::bound_object_property_starts(cursor)
        || (word_is(cursor, 0, "this")
            && cursor
                .token_slice(cursor.position() + 1, cursor.position() + 2)
                .first()
                .is_some_and(|token| token.token_type == "possessiveTypeWord")
            && (word_is(cursor, 2, "power") || word_is(cursor, 2, "toughness")))
}

/// `EXPRESSION is greater than|less than|equal to EXPRESSION`.
fn recognize_numeric_relation_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.relation.left");
    let left = recognize_numeric_expression(cursor, None)?;
    cursor.enter("condition.sentence.relation.copula");
    cursor.consume("is")?;
    cursor.enter("condition.sentence.relation.operator");
    let operator = if word_is(cursor, 0, "greater") && word_is(cursor, 1, "than") {
        cursor.consume("greater")?;
        cursor.consume("than")?;
        NumericComparisonOperator::GreaterThan
    } else if word_is(cursor, 0, "less") && word_is(cursor, 1, "than") {
        cursor.consume("less")?;
        cursor.consume("than")?;
        NumericComparisonOperator::LessThan
    } else if word_is(cursor, 0, "equal") && word_is(cursor, 1, "to") {
        cursor.consume("equal")?;
        cursor.consume("to")?;
        NumericComparisonOperator::Equal
    } else {
        return Err(cursor.fail("`greater than`, `less than` or `equal to`"));
    };
    cursor.enter("condition.sentence.relation.right");
    let right = recognize_numeric_expression(cursor, None)?;
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(left),
        operator,
        right: ConditionNumber::Expression(right),
    }))
}

/// `EXPRESSION is THRESHOLD`: the left operand is read by the shared numeric
/// expression grammar and the right one by the shared count threshold.
fn recognize_numeric_comparison_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.numeric.left");
    let left = recognize_numeric_expression(cursor, None)?;
    cursor.enter("condition.sentence.numeric.copula");
    cursor.consume("is")?;
    cursor.enter("condition.sentence.numeric.threshold");
    let threshold = recognize_count_threshold(cursor, false)?;
    cursor.require(
        !(threshold.operator == ThresholdOperator::AtMost && threshold.value == 0),
        "a numeric threshold rather than `no`",
    )?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(left),
        operator,
        right: ConditionNumber::Integer(value),
    }))
}

/// `this spell was kicked`, `this spell was cast from ZONE`, and
/// `this creature is CHARACTERISTIC` over the ability's own source.
fn recognize_source_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.source.subject");
    let spell = recognize_source_reference(cursor, context.name_links, context.name_failure)?;
    cursor.enter("condition.source.predicate");
    if word_is(cursor, 0, "was") && word_is(cursor, 1, "kicked") {
        cursor.consume("was")?;
        cursor.consume("kicked")?;
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::Kicked,
        }));
    }
    if word_is(cursor, 0, "was") && word_is(cursor, 1, "cast") {
        cursor.consume("was")?;
        cursor.consume("cast")?;
        cursor.enter("condition.source.cast-origin");
        cursor.require(
            word_is(cursor, 0, "from"),
            "`was kicked`, or `was cast from` a zone (an unqualified cast record of a spell is not read)",
        )?;
        cursor.consume("from")?;
        let (zone, negated) = recognize_cast_zone(cursor)?;
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::Cast {
                zone: Some(zone),
                negated,
            },
        }));
    }
    let predicate = recognize_object_state_predicate(cursor, false)?;
    finish_referenced_object(cursor, context, ConditionObjectReference::SOURCE, predicate)
        .map(ConditionAtom::ObjectSatisfies)
}

/// `you cast it`, `you cast this spell from your hand`: the controller cast
/// the ability's own source. A pronoun must denote that source; a permanent
/// remembers only whether it was cast and whether from a hand.
fn recognize_controller_cast_source_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.cast.subject");
    cursor.consume("you")?;
    cursor.consume("cast")?;
    cursor.enter("condition.sentence.cast.object");
    let pronoun = ["it", "him", "her", "them"]
        .iter()
        .any(|pronoun| word_is(cursor, 0, pronoun));
    let spell = if pronoun {
        consume_current(cursor)?;
        let reference = resolve_object_anaphor(cursor, context)?;
        cursor.enter("condition.sentence.cast.source-pronoun");
        cursor.require(
            reference == ConditionObjectReference::SOURCE,
            "a pronoun that denotes the ability's own source",
        )?;
        LocalObjectReference::SourceSelf
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?
    };
    if !word_is(cursor, 0, "from") {
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::Cast {
                zone: None,
                negated: false,
            },
        }));
    }
    cursor.consume("from")?;
    let (zone, negated) = recognize_cast_zone(cursor)?;
    cursor.enter("condition.sentence.cast.permanent-zone");
    cursor.require(
        !pronoun || zone == SourceCastZone::Hand,
        "`from your hand`; a permanent does not remember another cast zone",
    )?;
    Ok(ConditionAtom::CastFact(CastFactNode {
        spell: CastSpellReference::Source(spell),
        fact: CastFact::Cast {
            zone: Some(zone),
            negated,
        },
    }))
}

/// Lexical lookahead: `AMOUNT was|were|wasn't spent to cast` before the end
/// of the clause, where AMOUNT is the subject of the cast fact.
fn mana_spent_atom_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    (1..10)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "," | "." | ";" | ":"))
        })
        .any(|offset| {
            word_is(cursor, offset, "spent")
                && (word_is(cursor, offset - 1, "was")
                    || word_is(cursor, offset - 1, "were")
                    || contraction_is(cursor, offset - 1, "wasn", "t"))
                && word_is(cursor, offset + 1, "to")
                && word_is(cursor, offset + 2, "cast")
        })
}

/// The color words of mana amounts, in `ManaSpentAmount::SYMBOLS` order.
const MANA_AMOUNT_COLORS: [&str; 6] = ["white", "blue", "black", "red", "green", "colorless"];

/// The mana symbol at the cursor without its braces (`{R}` is `R`).
fn mana_symbol_at(cursor: &OracleTokenCursor<'_>) -> Option<String> {
    cursor
        .peek()
        .filter(|raw| raw.starts_with('{') && raw.ends_with('}') && raw.len() > 2)
        .map(|raw| raw[1..raw.len() - 1].to_ascii_uppercase())
}

/// `{R}`, `{U}{U}`, `{R}{G}` (each symbol at least its count), `at least
/// three red mana`, `five or more mana`, `no mana`: the amount subject of a
/// mana-spent fact. The source of spent mana (`mana from a Treasure`) and
/// snow or hybrid symbols are not recorded and fail here.
fn recognize_mana_spent_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaSpentAmount> {
    cursor.enter("condition.cast.mana-spent.amount");
    if mana_symbol_at(cursor).is_some() {
        let mut counts = [0_u8; 6];
        while let Some(symbol) = mana_symbol_at(cursor) {
            cursor.enter("condition.cast.mana-spent.symbol");
            let index = ManaSpentAmount::SYMBOLS
                .iter()
                .position(|known| *known == symbol)
                .ok_or_else(|| {
                    cursor.fail(
                        "a colored or colorless mana symbol (snow, hybrid and generic mana are not recorded by symbol)",
                    )
                })?;
            counts[index] = counts[index].saturating_add(1);
            consume_current(cursor)?;
        }
        return Ok(ManaSpentAmount::Symbols(counts));
    }
    cursor.require(
        !(word_is(cursor, 0, "mana") && word_is(cursor, 1, "from")),
        "an amount of spent mana (the source that produced spent mana is not recorded)",
    )?;
    if word_is(cursor, 0, "no") && word_is(cursor, 1, "mana") {
        cursor.consume_phrase(&["no", "mana"])?;
        return Ok(ManaSpentAmount::Total {
            operator: NumericComparisonOperator::Equal,
            value: 0,
        });
    }
    let threshold = recognize_count_threshold(cursor, false)?;
    cursor.enter("condition.cast.mana-spent.color");
    let color = cursor.peek().and_then(|raw| {
        MANA_AMOUNT_COLORS
            .iter()
            .position(|color| raw.eq_ignore_ascii_case(color))
    });
    if color.is_some() {
        consume_current(cursor)?;
    }
    cursor.enter("condition.cast.mana-spent.noun");
    cursor.consume("mana")?;
    match color {
        Some(index) => {
            cursor.require(
                threshold.operator == ThresholdOperator::AtLeast,
                "a lower bound on the mana of one color",
            )?;
            let mut counts = [0_u8; 6];
            counts[index] = u8::try_from(threshold.value)
                .map_err(|_| cursor.fail("a bounded amount of mana of one color"))?;
            Ok(ManaSpentAmount::Symbols(counts))
        }
        None => {
            let (operator, value) = threshold_comparison(threshold);
            Ok(ManaSpentAmount::Total { operator, value })
        }
    }
}

/// `this spell` (or the source's name), `that spell` (the spell a cast event
/// observed) or `it`, which names the observed spell after a cast event and
/// otherwise must denote the ability's own source.
fn recognize_cast_spell_reference(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<CastSpellReference> {
    cursor.enter("condition.cast.mana-spent.spell");
    let event_spell = context
        .program
        .is_some_and(|program| program.effects.frame.event_spell)
        || cursor
            .binding_environment()
            .is_some_and(|environment| environment.frame.event_spell);
    if word_is(cursor, 0, "that") && word_is(cursor, 1, "spell") {
        cursor.require(
            event_spell,
            "a cast event that records the spell `that spell` names",
        )?;
        cursor.consume_phrase(&["that", "spell"])?;
        return Ok(CastSpellReference::EventSpell);
    }
    if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        if event_spell {
            return Ok(CastSpellReference::EventSpell);
        }
        // Outside a program (`sacrifice it unless {W} was spent to cast
        // it`), the guarded action's own antecedent names the object.
        let environment_object = cursor
            .binding_environment()
            .and_then(|environment| environment.object.as_ref())
            .map(|antecedent| ConditionObjectReference::Permanent(antecedent.reference));
        let reference = match environment_object {
            Some(reference)
                if context.program.is_none()
                    && !context.source_subject
                    && context.subject_object.is_none() =>
            {
                reference
            }
            _ => resolve_object_anaphor(cursor, context)?,
        };
        cursor.enter("condition.cast.mana-spent.source-pronoun");
        cursor.require(
            reference == ConditionObjectReference::SOURCE,
            "a pronoun that denotes the ability's own source or the spell a cast event observed",
        )?;
        return Ok(CastSpellReference::Source(LocalObjectReference::SourceSelf));
    }
    recognize_source_reference(cursor, context.name_links, context.name_failure)
        .map(CastSpellReference::Source)
}

/// `AMOUNT was|wasn't spent to cast SPELL` (CR 601.2h): the cast record of
/// the ability's source or of the spell a cast event observed.
fn recognize_mana_spent_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    let amount = recognize_mana_spent_amount(cursor)?;
    cursor.enter("condition.cast.mana-spent.verb");
    let negated = contraction_is(cursor, 0, "wasn", "t");
    cursor.require(
        negated || word_is(cursor, 0, "was") || word_is(cursor, 0, "were"),
        "`was`, `were` or `wasn't` before `spent`",
    )?;
    consume_current(cursor)?;
    cursor.consume_phrase(&["spent", "to", "cast"])?;
    let spell = recognize_cast_spell_reference(cursor, context)?;
    Ok(ConditionAtom::CastFact(CastFactNode {
        spell,
        fact: CastFact::ManaSpent { amount, negated },
    }))
}

/// The origin of a cast: a zone noun phrase, or `anywhere other than` one.
fn recognize_cast_zone(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(SourceCastZone, bool)> {
    cursor.enter("condition.source.cast-zone");
    let negated = word_is(cursor, 0, "anywhere");
    if negated {
        cursor.consume("anywhere")?;
        cursor.enter("condition.source.cast-zone.complement");
        cursor.consume("other")?;
        cursor.consume("than")?;
    }
    let zone = if word_is(cursor, 0, "your") && word_is(cursor, 1, "hand") {
        cursor.consume("your")?;
        cursor.consume("hand")?;
        SourceCastZone::Hand
    } else if word_is(cursor, 0, "a") && word_is(cursor, 1, "graveyard") {
        cursor.consume("a")?;
        cursor.consume("graveyard")?;
        SourceCastZone::Graveyard
    } else if word_is(cursor, 0, "exile") {
        crate::oracle::canonical::automata::consume_lemma(
            cursor,
            crate::oracle::canonical::automata::VerbLemma::Exile,
            crate::oracle::canonical::automata::VerbAgreement::Base,
        )?;
        SourceCastZone::Exile
    } else {
        return Err(cursor.fail("`your hand`, `a graveyard` or `exile`"));
    };
    cursor.enter("condition.source.cast-zone.complement-zone");
    cursor.require(
        !negated || zone == SourceCastZone::Hand,
        "`anywhere other than your hand`; another complement has no cast record",
    )?;
    Ok((zone, negated))
}

/// `enchanted PERMANENT` / `equipped creature` followed by the shared object
/// state predicate: the permanent the source is attached to, which the
/// runtime binds (CR 301.5a, 303.4a). The role must match the source face.
fn recognize_attached_object_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.attached-object.subject");
    if contraction_is(cursor, 1, "creature", "s") {
        // `equipped creature's power is greater than EXPR`: a property of the
        // attached permanent compared with the shared numeric grammar.
        cursor.enter("condition.attached-object.possessive");
        let role = if word_is(cursor, 0, "equipped") {
            AttachedObjectRole::Equipped
        } else {
            AttachedObjectRole::Enchanted
        };
        cursor.require(
            context.source_attachment == Some(role),
            "the attachment relation of the source face",
        )?;
        consume_current(cursor)?;
        consume_current(cursor)?;
        let object = ConditionObjectReference::Permanent(PermanentEffectReference::Attached(role));
        cursor.enter("condition.attached-object.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.attached-object.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        let (operator, right, halved) = recognize_comparison_tail(cursor)?;
        cursor.require(!halved, "a whole operand of the property comparison")?;
        let right = match right {
            ConditionNumber::Integer(value) => NumericExpression::from(value),
            ConditionNumber::Expression(expression) => expression,
            _ => return Err(cursor.fail("a numeric operand of the property comparison")),
        };
        let predicate = ObjectPredicate::NumericComparison {
            left: property,
            operator,
            right: Box::new(right),
        };
        return finish_referenced_object(cursor, context, object, predicate);
    }
    let role = recognize_attached_object_reference(cursor, context.source_attachment)?;
    let object = ConditionObjectReference::Permanent(PermanentEffectReference::Attached(role));
    let predicate = recognize_object_state_predicate(cursor, false)?;
    finish_referenced_object(cursor, context, object, predicate)
}

/// `it's a CHARACTERISTIC`, `it isn't …`, `that NOUN is …`, `it has mana
/// value N or less`, `its PROPERTY is N or greater`, `it's tapped`.
fn recognize_referenced_object_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.sentence.object.subject");
    if word_is(cursor, 0, "its") {
        cursor.consume("its")?;
        let object = resolve_object_anaphor(cursor, context)?;
        cursor.enter("condition.sentence.object.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.sentence.object.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        cursor.enter("condition.sentence.object.comparison");
        let predicate = recognize_numeric_property_comparison_tail(cursor, property)?;
        return finish_referenced_object(cursor, context, object, predicate);
    }
    if contraction_is(cursor, 0, "it", "s") {
        consume_current(cursor)?;
        let object = resolve_object_anaphor(cursor, context)?;
        let predicate = recognize_object_state_predicate(cursor, true)?;
        return finish_referenced_object(cursor, context, object, predicate);
    }
    let object = if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        resolve_object_anaphor(cursor, context)?
    } else if word_is(cursor, 0, "that") && word_is(cursor, 1, "card") {
        cursor.consume("that")?;
        cursor.consume("card")?;
        resolve_card_anaphor(cursor, context)?
    } else {
        // `that NOUN`: the one nominal-anaphor resolver of the binding
        // environment. A condition reads the object's last known
        // information, so the nominal may name a departed event object (`If
        // that creature was a Villain` after `another creature dies`); a
        // nominal without an antecedent in scope fails there, never guessed.
        cursor.enter("condition.sentence.object.nominal-anaphor");
        let program = cursor.require_value(
            context.program,
            "a nominal anaphor of an effect program's proven antecedent",
        )?;
        ConditionObjectReference::Permanent(recognize_nominal_object_read(
            cursor,
            &BindingEnvironment::for_permanent_context(program.effects),
        )?)
    };
    let predicate = recognize_object_state_predicate(cursor, false)?;
    finish_referenced_object(cursor, context, object, predicate)
}

/// The verb phrase after a referenced object: a copula with a registered
/// status list or a shared object query, or `has` with counters, a keyword
/// or a numeric property comparison. `copula_read` is true after `it's`.
fn recognize_object_state_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    copula_read: bool,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("condition.sentence.object.verb");
    let mut negated = false;
    if !copula_read {
        let verb = cursor
            .peek()
            .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase())
            .ok_or_else(|| cursor.fail("`is`, `was`, `isn't`, `wasn't`, `has` or `had`"))?;
        match verb.as_str() {
            "is" | "was" => consume_current(cursor)?,
            "isn't" | "wasn't" => {
                consume_current(cursor)?;
                negated = true;
            }
            "has" | "had" => {
                consume_current(cursor)?;
                cursor.enter("condition.sentence.object.possession");
                // `has a +1/+1 counter on it`, `had no counters on it`,
                // `has three or more quest counters on it` (CR 122.1): the
                // shared counter qualifier of object queries.
                if super::counter_qualifier_starts(cursor, 0) {
                    return super::recognize_counter_qualifier(cursor);
                }
                if let Some(keyword) = cursor
                    .peek()
                    .and_then(parameterless_keyword_from_oracle_word)
                    && !word_is(cursor, 0, "mana")
                    && !word_is(cursor, 0, "power")
                    && !word_is(cursor, 0, "toughness")
                {
                    consume_current(cursor)?;
                    return Ok(ObjectPredicate::HasKeyword(keyword));
                }
                let property = recognize_selector_numeric_property(cursor)?;
                cursor.enter("condition.sentence.object.comparison");
                return recognize_numeric_property_comparison_tail(cursor, property);
            }
            _ => return Err(cursor.fail("`is`, `was`, `isn't`, `wasn't`, `has` or `had`")),
        }
    }
    if word_is(cursor, 0, "not") {
        cursor.consume("not")?;
        negated = !negated;
    }
    cursor.enter("condition.sentence.object.characteristic");
    let predicate = if let Some(statuses) = recognize_status_list(cursor)? {
        statuses
    } else {
        if word_is(cursor, 0, "a") || word_is(cursor, 0, "an") {
            consume_current(cursor)?;
        }
        let query = recognize_object_query(cursor)?;
        cursor.enter("condition.sentence.object.relation");
        cursor.require(
            query.controller.is_none(),
            "an object characteristic without a controller relation",
        )?;
        query.predicate
    };
    Ok(if negated {
        ObjectPredicate::Not(Box::new(predicate))
    } else {
        predicate
    })
}

/// `tapped`, `attacking`, `monstrous`, `enchanted or equipped`: registered
/// statuses and designations of the object, one or a coordinated list with
/// one connector (their union or intersection). `None` when no registered
/// status starts here, so that the shared object query reads the phrase.
fn recognize_status_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectPredicate>> {
    let status_at = |cursor: &OracleTokenCursor<'_>, offset: usize| {
        cursor
            .peek_ahead(offset)
            .and_then(super::lookup_status_predicate)
    };
    let Some(first) = status_at(cursor, 0) else {
        return Ok(None);
    };
    // A status adjective that modifies a following noun (`a tapped
    // creature`) belongs to the object query.
    let ends_here = |cursor: &OracleTokenCursor<'_>, offset: usize| {
        cursor.peek_ahead(offset).is_none_or(|raw| {
            matches!(
                raw.to_ascii_lowercase().as_str(),
                "," | "." | "and" | "or" | ";" | ":"
            )
        })
    };
    if !ends_here(cursor, 1) {
        return Ok(None);
    }
    cursor.enter("condition.sentence.object.status");
    consume_current(cursor)?;
    let mut members = vec![first];
    let mut connector: Option<String> = None;
    while (word_is(cursor, 0, "or") || word_is(cursor, 0, "and"))
        && status_at(cursor, 1).is_some()
        && ends_here(cursor, 2)
    {
        let word = cursor
            .peek()
            .expect("the connector was checked")
            .to_ascii_lowercase();
        cursor.require(
            connector.as_deref().is_none_or(|previous| previous == word),
            "one connector in a status list",
        )?;
        consume_current(cursor)?;
        let member = status_at(cursor, 0).expect("the status was checked");
        consume_current(cursor)?;
        members.push(member);
        connector = Some(word);
    }
    Ok(Some(match (members.len(), connector.as_deref()) {
        (1, _) => members.pop().expect("one status was read"),
        (_, Some("and")) => ObjectPredicate::Intersection(members),
        _ => ObjectPredicate::Union(members),
    }))
}

/// The width of `there is|are` / `there's` at the cursor, if present.
fn existential_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    if contraction_is(cursor, 0, "there", "s") {
        Some(1)
    } else if word_is(cursor, 0, "there") && (word_is(cursor, 1, "is") || word_is(cursor, 1, "are"))
    {
        Some(2)
    } else {
        None
    }
}

/// Lexical lookahead: `there is|are QUANTITY [KIND] counter(s) on ...`, the
/// existential form of a counter count on one object. A zone count (`there
/// are N cards in your graveyard`) is not this form.
fn existential_counter_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let Some(start) = existential_width(cursor) else {
        return false;
    };
    for offset in start..start + 7 {
        let Some(raw) = cursor.peek_ahead(offset) else {
            return false;
        };
        if raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters") {
            return word_is(cursor, offset + 1, "on");
        }
        if matches!(raw, "," | "." | ";") || raw.eq_ignore_ascii_case("in") {
            return false;
        }
    }
    false
}

/// `there are no KIND counters on OBJECT`, `there is a KIND counter on it`,
/// `there are N or more counters on this permanent`: the number of counters
/// on one referenced object (CR 122.1), read with the shared threshold and
/// counter name, and tested with the shared object predicate. The object is
/// resolved like the subject of any object condition; it is never guessed.
fn recognize_existential_counter_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.existential-counter.existential");
    let width = existential_width(cursor).ok_or_else(|| cursor.fail("`there is|are`"))?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.existential-counter.quantity");
    let threshold = recognize_count_threshold(cursor, true)?;
    let (minimum, maximum) = match threshold.operator {
        ThresholdOperator::AtLeast => (threshold.value, None),
        ThresholdOperator::AtMost => (0, Some(threshold.value)),
        ThresholdOperator::Exactly => (threshold.value, Some(threshold.value)),
    };
    cursor.enter("condition.existential-counter.kind");
    let counter = if word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters") {
        None
    } else {
        Some(recognize_counter_name(cursor)?)
    };
    cursor.enter("condition.existential-counter.noun");
    cursor.require(
        word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters"),
        "`counter` or `counters`",
    )?;
    consume_current(cursor)?;
    cursor.enter("condition.existential-counter.location");
    cursor.consume("on")?;
    cursor.enter("condition.existential-counter.object");
    let object = if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        resolve_object_anaphor(cursor, context)?
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        ConditionObjectReference::SOURCE
    };
    finish_referenced_object(
        cursor,
        context,
        object,
        ObjectPredicate::HasCounter(CounterPresence {
            counter,
            minimum,
            maximum,
        }),
    )
}

/// `it` denotes exactly one proven singular antecedent: a permanent subject,
/// the latest declared card target, or the current card-collection result.
/// Outside an effect program, `it` denotes the source when the parent's
/// subject is its source.
fn resolve_object_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.antecedent");
    let Some(program) = context.program else {
        if let Some(subject) = context.subject_object {
            return Ok(subject);
        }
        cursor.require(
            context.source_subject,
            "`it` that denotes the ability's own source",
        )?;
        return Ok(ConditionObjectReference::SOURCE);
    };
    let mut candidates = Vec::new();
    if let Some(reference) = super::permanent_subject::proven_read_antecedent(program.effects) {
        candidates.push(ConditionObjectReference::Permanent(reference));
    }
    if let Some(binding) = program.card_target {
        candidates.push(ConditionObjectReference::CardTarget(binding));
    }
    if program.card_focus {
        candidates.push(ConditionObjectReference::CardFocus);
    }
    match candidates.as_slice() {
        [reference] => Ok(*reference),
        [] => Err(cursor.fail("one proven singular antecedent for the pronoun")),
        _ => Err(cursor.fail("exactly one proven antecedent for the pronoun, not several")),
    }
}

fn resolve_card_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.card-antecedent");
    let program = cursor.require_value(
        context.program,
        "an effect program whose card antecedent `that card` names",
    )?;
    match (program.card_target, program.card_focus) {
        (Some(binding), false) => Ok(ConditionObjectReference::CardTarget(binding)),
        (None, true) => Ok(ConditionObjectReference::CardFocus),
        (None, false) => Err(cursor.fail("one proven singular card antecedent for `that card`")),
        (Some(_), true) => {
            Err(cursor.fail("exactly one proven card antecedent for `that card`, not several"))
        }
    }
}

fn finish_referenced_object(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
    object: ConditionObjectReference,
    predicate: ObjectPredicate,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.sentence.object.last-known-information");
    let object = match (object, context.program) {
        (
            ConditionObjectReference::Permanent(PermanentEffectReference::Target(binding)),
            Some(program),
        ) if program.departed_targets.contains(&binding) => {
            // CR 608.2h: an object moved out of the battlefield by an earlier
            // instruction is tested with its last known information: the
            // numeric properties and the type line its departure recorded.
            // Any other characteristic is not captured and is refused
            // instead of being read on the new object.
            cursor.require(
                last_known_information_answers(&predicate),
                "a mana value, power, toughness or type-line characteristic of a moved permanent (its other last-known information is not captured)",
            )?;
            ConditionObjectReference::DepartedTarget(binding)
        }
        _ => object,
    };
    cursor.enter("condition.sentence.object.domain");
    let domain = match object {
        ConditionObjectReference::Permanent(_) | ConditionObjectReference::DepartedTarget(_) => {
            ObjectKind::Permanent
        }
        ConditionObjectReference::CardTarget(_) | ConditionObjectReference::CardFocus => {
            ObjectKind::Card
        }
    };
    cursor.require(
        predicate.supports_domain(domain),
        "a characteristic evaluable in the referenced object's domain",
    )?;
    Ok(ReferencedObjectConditionNode { object, predicate })
}

/// Whether a departed permanent's recorded last known information answers
/// the predicate: one numeric property comparison (the captured mana value,
/// power or toughness), or a predicate over the recorded type line.
fn last_known_information_answers(predicate: &ObjectPredicate) -> bool {
    fn type_line(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::CardType(_)
            | ObjectPredicate::Subtype(_)
            | ObjectPredicate::Supertype(_) => true,
            ObjectPredicate::Not(operand) => type_line(operand),
            ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
                !operands.is_empty() && operands.iter().all(type_line)
            }
            _ => false,
        }
    }
    matches!(predicate, ObjectPredicate::NumericComparison { .. }) || type_line(predicate)
}

pub(in crate::oracle::canonical) fn sentence_condition_depends_on_cast_choice(
    condition: &SentenceConditionNode,
) -> bool {
    condition_any(condition, &ConditionAtom::depends_on_cast_choice)
}

#[cfg(test)]
#[path = "sentence_condition_tests.rs"]
mod tests;
