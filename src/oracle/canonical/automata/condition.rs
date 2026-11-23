//! The one condition language (CR 603.4, 608.2c, 611.3a, 118.12).
//!
//! Every condition parent (`as long as`, a sentence `If`, an intervening
//! `if`, `unless`, and the `if ..., ` envelopes of family parents) reads its
//! Boolean program through the same atom reader
//! (`recognize_condition_atom`), subject first. Atoms belong to a small set
//! of families:
//!
//! ```text
//! Compare(Num, op, Num, player subject)   counts, life, X, power, ...
//! ObjectSatisfies(BindingRef, Pred)       the source, a target, a bound card
//! EventHistory(Pattern, scope)            this-turn / last-turn event counts
//! CastFact                                kicked, cast, cast from a zone
//! ActionOutcome                           `if you can't`, `if you don't`
//! TurnOwnership                           `it's [not] your turn`
//! PlayerDesignation                       enduring story, completed dungeon
//! ```
//!
//! A parent declares which families its evaluation moment can resolve
//! (`ConditionScope`); an atom outside that allow-list is an explicit
//! failure, never an approximation.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BooleanConditionNode, ChoiceBinding, ConditionContext, ConditionObjectReference,
    ConditionPlayerSubject, ControlledCountDirection, ControlledPermanentCountConditionNode,
    CountThreshold, LocalObjectReference, LocalPlayerReference, NumericAtom,
    NumericComparisonOperator, NumericExpression, ObjectKind, ObjectPredicate, PlayerStateMeasure,
    ThresholdOperator, recognize_card_type_union_selector, recognize_condition_atom,
    recognize_numeric_atom, recognize_object_query,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// A Boolean program over the one atom grammar.
pub(in crate::oracle::canonical) type ConditionNode = BooleanConditionNode<ConditionAtom>;

/// The families of the condition language. A parent's scope is an
/// allow-list over these families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionFamily {
    Compare,
    ObjectSatisfies,
    EventHistory,
    CastFact,
    ActionOutcome,
    TurnOwnership,
    PlayerDesignation,
}

/// The evaluation moment of a condition parent, which decides the atom
/// families it can resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionScope {
    /// `as long as`, activation and entry restrictions (CR 611.3a): a state
    /// evaluated continuously, without a cast record or a previous action.
    Static,
    /// A sentence `If` of an effect program (CR 608.2c): every family.
    Program,
    /// The intervening `if` of a triggered ability (CR 603.4): no earlier
    /// action of the program exists yet.
    TriggerGuard,
    /// `unless CONDITION`: a state condition (a payment is another branch).
    Unless,
    /// A family parent's `if ..., ` envelope that composes exactly one family.
    Only(ConditionFamily),
}

impl ConditionScope {
    pub(in crate::oracle::canonical) fn allows(self, family: ConditionFamily) -> bool {
        match self {
            Self::Program => true,
            Self::TriggerGuard => family != ConditionFamily::ActionOutcome,
            Self::Static | Self::Unless => !matches!(
                family,
                ConditionFamily::ActionOutcome | ConditionFamily::CastFact
            ),
            Self::Only(only) => family == only,
        }
    }

    pub(in crate::oracle::canonical) fn refusal(self) -> &'static str {
        match self {
            Self::Program => "a condition of the effect program",
            Self::TriggerGuard => {
                "an intervening-if condition (no earlier action of the program exists yet)"
            }
            Self::Static => "a continuously evaluated state condition",
            Self::Unless => "an `unless` state condition",
            Self::Only(_) => "the one condition family this parent composes",
        }
    }
}

/// One atom of the condition language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionAtom {
    /// `Compare(Num, op, Num)` read for one player subject.
    Compare(ConditionComparison),
    /// `ObjectSatisfies(BindingRef, Pred)`: a referenced object tested with
    /// the shared object predicate.
    ObjectSatisfies(ReferencedObjectConditionNode),
    /// `EventHistory(Pattern, scope)` compared with a count.
    EventHistory(EventHistoryNode),
    /// A fact recorded when the source spell was cast.
    CastFact(CastFactNode),
    /// The outcome of an earlier action or optional payment of the program.
    ActionOutcome(ActionOutcomeNode),
    /// `it's [not] your turn`.
    TurnOwnership(ControllerTurnPropositionNode),
    /// A designation the controller holds (CR 309.7, enduring story).
    PlayerDesignation(PlayerDesignationNode),
}

impl ConditionAtom {
    pub(in crate::oracle::canonical) fn family(&self) -> ConditionFamily {
        match self {
            Self::Compare(_) => ConditionFamily::Compare,
            Self::ObjectSatisfies(_) => ConditionFamily::ObjectSatisfies,
            Self::EventHistory(_) => ConditionFamily::EventHistory,
            Self::CastFact(_) => ConditionFamily::CastFact,
            Self::ActionOutcome(_) => ConditionFamily::ActionOutcome,
            Self::TurnOwnership(_) => ConditionFamily::TurnOwnership,
            Self::PlayerDesignation(_) => ConditionFamily::PlayerDesignation,
        }
    }

    /// Kicker status is chosen as the spell is cast (CR 702.33d); targets in
    /// a branch it governs are required only if the spell was kicked.
    pub(in crate::oracle::canonical) fn depends_on_cast_choice(&self) -> bool {
        matches!(
            self,
            Self::CastFact(CastFactNode {
                fact: CastFact::Kicked,
                ..
            })
        )
    }

    /// Whether the atom counts the controller's permanents other than the
    /// ability's own source (`another`, `other`).
    pub(in crate::oracle::canonical) fn excludes_source(&self) -> bool {
        let excludes = |number: &ConditionNumber| {
            matches!(
                number,
                ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                    exclude_source: true,
                    ..
                })
            )
        };
        match self {
            Self::Compare(comparison) => excludes(&comparison.left) || excludes(&comparison.right),
            _ => false,
        }
    }

    /// Whether the atom reads the result of the instruction before it.
    pub(in crate::oracle::canonical) fn reads_previous_action(&self) -> bool {
        matches!(
            self,
            Self::ActionOutcome(ActionOutcomeNode::PreviousActionImpossible)
        )
    }
}

/// Whether any atom of the program satisfies `test`.
pub(in crate::oracle::canonical) fn condition_any(
    condition: &ConditionNode,
    test: &impl Fn(&ConditionAtom) -> bool,
) -> bool {
    match condition {
        BooleanConditionNode::Atom(atom) => test(atom),
        BooleanConditionNode::And(operands) | BooleanConditionNode::Or(operands) => {
            operands.iter().any(|operand| condition_any(operand, test))
        }
        BooleanConditionNode::Not(operand) => condition_any(operand, test),
    }
}

/// `Compare(left, operator, right)` where every player measure is read for
/// `subject`: the controller, or each member of a quantified population.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionComparison {
    pub subject: ConditionPlayerSubject,
    pub left: ConditionNumber,
    pub operator: NumericComparisonOperator,
    pub right: ConditionNumber,
}

impl ConditionComparison {
    /// `subject` has `measure` within `threshold`.
    pub(in crate::oracle::canonical) fn measured(
        subject: ConditionPlayerSubject,
        measure: PlayerStateMeasure,
        threshold: CountThreshold,
    ) -> Self {
        let (operator, value) = threshold_comparison(threshold);
        Self {
            subject,
            left: ConditionNumber::Player(measure),
            operator,
            right: ConditionNumber::Integer(value),
        }
    }
}

/// The comparison a fixed threshold states.
pub(in crate::oracle::canonical) fn threshold_comparison(
    threshold: CountThreshold,
) -> (NumericComparisonOperator, i64) {
    match threshold.operator {
        ThresholdOperator::AtLeast => (
            NumericComparisonOperator::GreaterThanOrEqual,
            threshold.value,
        ),
        ThresholdOperator::AtMost => (NumericComparisonOperator::LessThanOrEqual, threshold.value),
    }
}

/// One operand of a comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionNumber {
    Integer(i64),
    /// The shared numeric expression grammar (`X`, `that creature's power`).
    Expression(NumericExpression),
    /// A measure of the comparison's player subject.
    Player(PlayerStateMeasure),
    /// The greatest value of a measure among the opponents of the subject.
    GreatestOpponent(PlayerStateMeasure),
}

/// The object an object condition tests, with its predicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReferencedObjectConditionNode {
    pub object: ConditionObjectReference,
    pub predicate: ObjectPredicate,
}

/// The typed event of a history pattern. Each kind is one recorded game
/// event; the pattern's fields say whose and which object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryEvent {
    /// CR 601.2i: a spell became cast.
    SpellCast,
    /// CR 121.1.
    CardDrawn,
    /// CR 119.3.
    LifeGained,
    LifeLost,
    /// CR 120.3: damage dealt to the player (the event's player).
    DamageDealtToPlayer,
    /// CR 508.1a: a creature was declared as an attacker.
    AttackerDeclared,
    /// CR 700.4.
    PermanentDied,
    /// CR 603.6c.
    PermanentLeftBattlefield,
    /// CR 603.6a.
    PermanentEntered,
    /// CR 701.23.
    LibrarySearched,
}

/// The player relation of the event's player (the actor, the recipient of
/// damage, or the controller of a permanent that moved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryPlayer {
    Any,
    Controller,
    Opponent,
}

/// What a history pattern counts per matching event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryMeasure {
    /// One per event.
    Occurrences,
    /// The event's amount (life, damage).
    Amount,
}

/// The turn a history pattern reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryScope {
    ThisTurn,
    /// The turn immediately before the current one (`last turn`).
    LastTurn,
}

/// How the per-event measure is aggregated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum HistoryAggregate {
    /// Summed over every matching event.
    Total,
    /// Summed per player, then the greatest per-player sum (`a player cast
    /// two or more spells`: one player did).
    GreatestPerPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventHistoryPattern {
    pub event: HistoryEvent,
    pub player: HistoryPlayer,
    /// The event object's predicate; `None` for every object of the event.
    pub object: Option<ObjectPredicate>,
    pub measure: HistoryMeasure,
}

/// `EventHistory(Pattern, scope)` compared with a fixed count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventHistoryNode {
    pub pattern: EventHistoryPattern,
    pub scope: HistoryScope,
    pub aggregate: HistoryAggregate,
    pub operator: NumericComparisonOperator,
    pub threshold: i64,
}

/// The zone a spell was cast from (CR 601.2a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceCastZone {
    Hand,
    Graveyard,
    Exile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastFact {
    /// CR 702.33d.
    Kicked,
    /// The source was cast; from `zone` when present. `negated` reads
    /// `anywhere other than ZONE`.
    Cast {
        zone: Option<SourceCastZone>,
        negated: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastFactNode {
    pub spell: LocalObjectReference,
    pub fact: CastFact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionOutcomeNode {
    /// `If you can't` (CR 609.3): the instruction just before the condition
    /// could not be performed.
    PreviousActionImpossible,
    /// `if you don't`: the optional payment the parent bound was not paid.
    CostNotPaid { payment: ChoiceBinding },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerDesignation {
    EnduringStory,
    /// CR 309.7: `dungeon` names one Dungeon, or any Dungeon when absent.
    CompletedDungeon {
        dungeon: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerDesignationNode {
    pub designation: PlayerDesignation,
    /// False for `you haven't completed ...`.
    pub held: bool,
}

/// Typed turn-ownership proposition `it's [not] your turn`. The enclosing
/// `if` / `unless` introducer owns polarity; this child owns only the clause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControllerTurnPropositionNode {
    /// True when the proposition asserts that it is the controller's turn.
    pub is_controller_turn: bool,
}

impl ControlledPermanentCountConditionNode {
    /// The count query as a comparison atom of the controller.
    pub(in crate::oracle::canonical) fn into_atom(self) -> ConditionAtom {
        debug_assert_eq!(self.player, LocalPlayerReference::Controller);
        ConditionAtom::Compare(ConditionComparison {
            subject: ConditionPlayerSubject::Controller,
            left: ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                predicate: self.predicate,
                exclude_source: self.exclude_source,
            }),
            operator: match self.direction {
                ControlledCountDirection::OrMore => NumericComparisonOperator::GreaterThanOrEqual,
                ControlledCountDirection::OrFewer => NumericComparisonOperator::LessThanOrEqual,
            },
            right: ConditionNumber::Integer(self.threshold),
        })
    }
}

/// Reads `if ATOM,` for a family parent that composes exactly one family.
/// The atom is read by the shared reader; the family is the allow-list.
fn recognize_family_envelope(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionNode> {
    cursor.enter(state);
    cursor.consume("if")?;
    let atom = recognize_condition_atom(cursor, context)?;
    cursor.enter("condition.envelope.separator");
    cursor.consume(",")?;
    Ok(BooleanConditionNode::Atom(atom))
}

fn family_context(family: ConditionFamily) -> ConditionContext<'static> {
    ConditionContext::standalone(ConditionScope::Only(family), &[], None, false)
}

/// Recognizes `if you control a/an <permanent predicate>,` as the
/// controlled-permanent count comparison shared by every condition parent.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_presence_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let node = recognize_family_envelope(
        cursor,
        "condition.controlled-permanent-presence.if",
        &family_context(ConditionFamily::Compare),
    )?;
    cursor.enter("condition.controlled-permanent-presence.measure");
    cursor.require(
        controlled_permanent_count(&node).is_some(),
        "a controlled permanent presence condition",
    )?;
    Ok(node)
}

/// The controlled-permanent count of a single comparison atom, if it is one.
pub(in crate::oracle::canonical) fn controlled_permanent_count(
    condition: &ConditionNode,
) -> Option<(&ObjectPredicate, NumericComparisonOperator, i64, bool)> {
    let BooleanConditionNode::Atom(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left:
            ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                predicate,
                exclude_source,
            }),
        operator,
        right: ConditionNumber::Integer(threshold),
    })) = condition
    else {
        return None;
    };
    Some((predicate, *operator, *threshold, *exclude_source))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentPresenceState {
    Player,
    Control,
    Determiner,
    PermanentPredicate,
    Complete,
}

/// Reads `you control a/an/another OBJECT` as a count query (at least one).
/// This query owns neither `if` nor the trailing clause comma: the shared
/// object query returns a comma that does not continue its own list.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_presence_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledPermanentCountConditionNode> {
    let mut state = ControlledPermanentPresenceState::Player;
    let mut predicate = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            ControlledPermanentPresenceState::Player => {
                cursor.enter("condition.controlled-permanent-presence.player");
                cursor.consume("you")?;
                ControlledPermanentPresenceState::Control
            }
            ControlledPermanentPresenceState::Control => {
                cursor.enter("condition.controlled-permanent-presence.control");
                cursor.consume("control")?;
                ControlledPermanentPresenceState::Determiner
            }
            ControlledPermanentPresenceState::Determiner => {
                cursor.enter("condition.controlled-permanent-presence.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    determiner.raw.eq_ignore_ascii_case("a")
                        || determiner.raw.eq_ignore_ascii_case("an")
                        || determiner.raw.eq_ignore_ascii_case("another"),
                    "`a`, `an` or `another` before a controlled permanent predicate",
                )?;
                // `another` is `a` that is not the ability's own source (CR 201.5).
                exclude_source = determiner.raw.eq_ignore_ascii_case("another");
                ControlledPermanentPresenceState::PermanentPredicate
            }
            ControlledPermanentPresenceState::PermanentPredicate => {
                cursor.enter("condition.controlled-permanent-presence.permanent-predicate");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped permanent selector after `you control`",
                )?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent),
                    "a predicate valid for a battlefield permanent",
                )?;
                predicate = Some(query.predicate);
                ControlledPermanentPresenceState::Complete
            }
            ControlledPermanentPresenceState::Complete => {
                cursor.enter("condition.controlled-permanent-presence.complete");
                return Ok(ControlledPermanentCountConditionNode {
                    player: LocalPlayerReference::Controller,
                    threshold: 1,
                    direction: ControlledCountDirection::OrMore,
                    predicate: cursor
                        .require_value(predicate, "the controlled permanent predicate")?,
                    exclude_source,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledPermanentCountState {
    Player,
    Control,
    ComparatorPrefix,
    Threshold,
    ComparatorConnector,
    ComparatorDirection,
    OptionalOther,
    PermanentPredicate,
    Complete,
}

/// Recognizes the reusable count query `you control N or more/fewer [other]
/// <permanents>`. The surrounding condition owns `if`/`unless` and punctuation.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_count_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControlledPermanentCountConditionNode> {
    let mut state = ControlledPermanentCountState::Player;
    let mut threshold = None;
    let mut direction = None;
    let mut predicate = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            ControlledPermanentCountState::Player => {
                cursor.enter("condition.controlled-permanent-count.player");
                cursor.consume("you")?;
                ControlledPermanentCountState::Control
            }
            ControlledPermanentCountState::Control => {
                cursor.enter("condition.controlled-permanent-count.control");
                cursor.consume("control")?;
                ControlledPermanentCountState::ComparatorPrefix
            }
            ControlledPermanentCountState::ComparatorPrefix => {
                cursor.enter("condition.controlled-permanent-count.comparator-prefix");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("at"))
                {
                    cursor.consume("at")?;
                    direction = Some(match cursor.peek() {
                        Some(raw) if raw.eq_ignore_ascii_case("least") => {
                            cursor.consume("least")?;
                            ControlledCountDirection::OrMore
                        }
                        Some(raw) if raw.eq_ignore_ascii_case("most") => {
                            cursor.consume("most")?;
                            ControlledCountDirection::OrFewer
                        }
                        _ => return Err(cursor.fail("`least` or `most` after `at`")),
                    });
                }
                ControlledPermanentCountState::Threshold
            }
            ControlledPermanentCountState::Threshold => {
                cursor.enter("condition.controlled-permanent-count.threshold");
                let value = match recognize_numeric_atom(cursor)? {
                    NumericAtom::Integer(value) => value,
                    NumericAtom::VariableX => {
                        return Err(cursor.fail("a fixed nonnegative permanent-count threshold"));
                    }
                };
                cursor.require(value >= 0, "a fixed nonnegative permanent-count threshold")?;
                threshold = Some(value);
                if direction.is_some() {
                    ControlledPermanentCountState::OptionalOther
                } else {
                    ControlledPermanentCountState::ComparatorConnector
                }
            }
            ControlledPermanentCountState::ComparatorConnector => {
                cursor.enter("condition.controlled-permanent-count.comparator-connector");
                cursor.consume("or")?;
                ControlledPermanentCountState::ComparatorDirection
            }
            ControlledPermanentCountState::ComparatorDirection => {
                cursor.enter("condition.controlled-permanent-count.comparator-direction");
                direction = Some(match cursor.peek() {
                    Some(raw) if raw.eq_ignore_ascii_case("more") => {
                        cursor.consume("more")?;
                        ControlledCountDirection::OrMore
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("fewer") => {
                        cursor.consume("fewer")?;
                        ControlledCountDirection::OrFewer
                    }
                    _ => return Err(cursor.fail("`more` or `fewer`")),
                });
                ControlledPermanentCountState::OptionalOther
            }
            ControlledPermanentCountState::OptionalOther => {
                // `other` is read with the query, as its own field.
                cursor.enter("condition.controlled-permanent-count.optional-other");
                ControlledPermanentCountState::PermanentPredicate
            }
            ControlledPermanentCountState::PermanentPredicate => {
                cursor.enter("condition.controlled-permanent-count.permanent-predicate");
                let query =
                    super::recognize_query_excluding_source(cursor, recognize_object_query)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped permanent selector after `you control`",
                )?;
                exclude_source = query.exclude_source;
                // The leading `you control` is this query's control relation.
                let controlled = super::ObjectQueryNode {
                    controller: Some(super::ObjectControllerScope::AbilityController),
                    ..query
                };
                cursor.require(
                    controlled.denotes_battlefield_permanents(),
                    "a selector that establishes the permanent domain",
                )?;
                predicate = Some(controlled.predicate);
                ControlledPermanentCountState::Complete
            }
            ControlledPermanentCountState::Complete => {
                cursor.enter("condition.controlled-permanent-count.complete");
                return Ok(ControlledPermanentCountConditionNode {
                    player: LocalPlayerReference::Controller,
                    threshold: cursor.require_value(threshold, "the count threshold")?,
                    direction: cursor.require_value(direction, "the comparison direction")?,
                    predicate: cursor
                        .require_value(predicate, "the counted permanent predicate")?,
                    exclude_source,
                });
            }
        };
    }
}

/// Player-count query `you have N or more/fewer opponents` (CR 102.3: the
/// opponents of the reference player). The surrounding condition owns
/// `if`/`unless` and punctuation; threshold and direction stay typed values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OpponentCountConditionNode {
    pub player: LocalPlayerReference,
    pub threshold: i64,
    pub direction: ControlledCountDirection,
}

pub(in crate::oracle::canonical) fn recognize_opponent_count_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OpponentCountConditionNode> {
    cursor.enter("condition.opponent-count.player");
    cursor.consume("you")?;
    cursor.enter("condition.opponent-count.have");
    cursor.consume("have")?;
    cursor.enter("condition.opponent-count.threshold");
    let threshold = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value >= 0 => value,
        _ => return Err(cursor.fail("a fixed nonnegative opponent-count threshold")),
    };
    cursor.enter("condition.opponent-count.comparator-connector");
    cursor.consume("or")?;
    cursor.enter("condition.opponent-count.comparator-direction");
    let direction = match cursor.peek() {
        Some(raw) if raw.eq_ignore_ascii_case("more") => ControlledCountDirection::OrMore,
        Some(raw) if raw.eq_ignore_ascii_case("fewer") => ControlledCountDirection::OrFewer,
        _ => return Err(cursor.fail("`more` or `fewer`")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("condition.opponent-count.noun");
    let noun = cursor.peek().ok_or_else(|| cursor.fail("`opponents`"))?;
    cursor.require(
        noun.eq_ignore_ascii_case("opponents")
            || (threshold == 1 && noun.eq_ignore_ascii_case("opponent")),
        "`opponents`",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("condition.opponent-count.complete");
    Ok(OpponentCountConditionNode {
        player: LocalPlayerReference::Controller,
        threshold,
        direction,
    })
}

/// Recognizes `if you control N or more/fewer <permanents>,` through the
/// shared atom reader.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_count_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let node = recognize_family_envelope(
        cursor,
        "condition.controlled-permanent-count.if",
        &family_context(ConditionFamily::Compare),
    )?;
    cursor.enter("condition.controlled-permanent-count.measure");
    cursor.require(
        controlled_permanent_count(&node).is_some(),
        "a controlled permanent count condition",
    )?;
    Ok(node)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardsExistInZoneConditionState {
    If,
    Existential,
    Copula,
    Determiner,
    Predicate,
    ConnectorOrZone,
    ZoneRelation,
    ZonePossessor,
    Zone,
    Separator,
    Complete,
}

/// Recognizes an existential conjunction over typed card predicates in one
/// player zone, for example `if there is an instant card and a sorcery card
/// in your graveyard,`. Each object phrase is delegated to the shared card
/// selector, while this parent owns Boolean `and` and the zone scope: the
/// result is a conjunction of the shared card-count comparisons.
pub(in crate::oracle::canonical) fn recognize_cards_exist_in_controller_graveyard_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let mut state = CardsExistInZoneConditionState::If;
    let mut predicates = Vec::new();
    loop {
        state = match state {
            CardsExistInZoneConditionState::If => {
                cursor.enter("condition.zone-card-presence.if");
                cursor.consume("if")?;
                CardsExistInZoneConditionState::Existential
            }
            CardsExistInZoneConditionState::Existential => {
                cursor.enter("condition.zone-card-presence.existential");
                cursor.consume("there")?;
                CardsExistInZoneConditionState::Copula
            }
            CardsExistInZoneConditionState::Copula => {
                cursor.enter("condition.zone-card-presence.copula");
                cursor.consume("is")?;
                CardsExistInZoneConditionState::Determiner
            }
            CardsExistInZoneConditionState::Determiner => {
                cursor.enter("condition.zone-card-presence.determiner");
                let determiner = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`a` or `an` before a card predicate"))?
                    .to_string();
                cursor.require(
                    determiner.eq_ignore_ascii_case("a") || determiner.eq_ignore_ascii_case("an"),
                    "`a` or `an` before a card predicate",
                )?;
                cursor.consume(&determiner)?;
                CardsExistInZoneConditionState::Predicate
            }
            CardsExistInZoneConditionState::Predicate => {
                cursor.enter("condition.zone-card-presence.predicate");
                let predicate = recognize_card_type_union_selector(cursor)?;
                cursor.require(
                    predicate.supports_domain(ObjectKind::Card)
                        && predicate.has_domain(ObjectKind::Card),
                    "a selector in the card domain",
                )?;
                predicates.push(predicate);
                CardsExistInZoneConditionState::ConnectorOrZone
            }
            CardsExistInZoneConditionState::ConnectorOrZone => {
                cursor.enter("condition.zone-card-presence.connector-or-zone");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    CardsExistInZoneConditionState::Determiner
                } else {
                    CardsExistInZoneConditionState::ZoneRelation
                }
            }
            CardsExistInZoneConditionState::ZoneRelation => {
                cursor.enter("condition.zone-card-presence.zone-relation");
                cursor.consume("in")?;
                CardsExistInZoneConditionState::ZonePossessor
            }
            CardsExistInZoneConditionState::ZonePossessor => {
                cursor.enter("condition.zone-card-presence.zone-possessor");
                cursor.consume("your")?;
                CardsExistInZoneConditionState::Zone
            }
            CardsExistInZoneConditionState::Zone => {
                cursor.enter("condition.zone-card-presence.zone");
                cursor.consume("graveyard")?;
                CardsExistInZoneConditionState::Separator
            }
            CardsExistInZoneConditionState::Separator => {
                cursor.enter("condition.zone-card-presence.separator");
                cursor.consume(",")?;
                CardsExistInZoneConditionState::Complete
            }
            CardsExistInZoneConditionState::Complete => {
                cursor.enter("condition.zone-card-presence.complete");
                cursor.require(!predicates.is_empty(), "one or more card predicates")?;
                // One conjunction per existential list, also of one member,
                // so the lowered program keeps a single shape.
                let atoms = predicates
                    .into_iter()
                    .map(|predicate| {
                        BooleanConditionNode::Atom(ConditionAtom::Compare(
                            ConditionComparison::measured(
                                ConditionPlayerSubject::Controller,
                                PlayerStateMeasure::CardsInZone {
                                    zone: super::PlayerCardZone::Graveyard,
                                    predicate,
                                },
                                CountThreshold::at_least(1),
                            ),
                        ))
                    })
                    .collect::<Vec<_>>();
                return Ok(BooleanConditionNode::And(atoms));
            }
        };
    }
}

/// Recognizes the event-history condition shared by revolt-style text
/// (`if a permanent left the battlefield under your control this turn`)
/// through the shared history reader, without assigning any meaning to a
/// decorative ability-word label.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_left_battlefield_this_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    cursor.enter("condition.permanent-left-battlefield.if");
    cursor.consume("if")?;
    let atom = recognize_condition_atom(cursor, &family_context(ConditionFamily::EventHistory))?;
    cursor.enter("condition.permanent-left-battlefield.event");
    cursor.require(
        matches!(
            &atom,
            ConditionAtom::EventHistory(EventHistoryNode {
                pattern: EventHistoryPattern {
                    event: HistoryEvent::PermanentLeftBattlefield,
                    ..
                },
                ..
            })
        ),
        "a permanent that left the battlefield",
    )?;
    Ok(BooleanConditionNode::Atom(atom))
}

/// Recognizes `if you don't,` while resolving the omitted payment through the
/// typed binding supplied by its parent sequence.
pub(in crate::oracle::canonical) fn recognize_cost_not_paid_condition(
    cursor: &mut OracleTokenCursor<'_>,
    payment: ChoiceBinding,
) -> MachineResult<ConditionNode> {
    let mut context = family_context(ConditionFamily::ActionOutcome);
    context.payment = Some(payment);
    recognize_family_envelope(cursor, "condition.cost-not-paid.if", &context)
}

/// Recognizes the reusable proposition `it's [not] your turn` without an
/// introducer or separator, so entry, cost and effect parents can compose it.
pub(in crate::oracle::canonical) fn recognize_controller_turn_proposition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ControllerTurnPropositionNode> {
    cursor.enter("condition.controller-turn.subject");
    let subject = cursor.consume_type("referenceWord")?;
    cursor.require(
        subject.raw.eq_ignore_ascii_case("it's") || subject.raw.eq_ignore_ascii_case("it’s"),
        "the current-turn reference `it's`",
    )?;
    cursor.enter("condition.controller-turn.negation");
    let negated = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("not"));
    if negated {
        cursor.consume("not")?;
    }
    cursor.enter("condition.controller-turn.possessor");
    cursor.consume("your")?;
    cursor.enter("condition.controller-turn.turn");
    cursor.consume("turn")?;
    Ok(ControllerTurnPropositionNode {
        is_controller_turn: !negated,
    })
}

/// Recognizes the turn condition `if it's not your turn,` through the shared
/// atom reader.
pub(in crate::oracle::canonical) fn recognize_not_your_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionNode> {
    let node = recognize_family_envelope(
        cursor,
        "condition.not-your-turn.if",
        &family_context(ConditionFamily::TurnOwnership),
    )?;
    cursor.enter("condition.not-your-turn.polarity");
    cursor.require(
        node == BooleanConditionNode::Atom(ConditionAtom::TurnOwnership(
            ControllerTurnPropositionNode {
                is_controller_turn: false,
            },
        )),
        "`it's not your turn`",
    )?;
    Ok(node)
}

/// Recognizes the reusable condition envelope `if this spell was kicked,`.
///
/// The spell reference is read by the shared atom reader with the shared
/// reference automaton, so the condition retains the same binding semantics
/// as an effect source.
pub(in crate::oracle::canonical) fn recognize_kicked_condition(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ConditionNode> {
    let context = ConditionContext::standalone(
        ConditionScope::Only(ConditionFamily::CastFact),
        name_links,
        name_failure,
        false,
    );
    let node = recognize_family_envelope(cursor, "condition.kicked.if", &context)?;
    cursor.enter("condition.kicked.fact");
    cursor.require(
        matches!(
            node,
            BooleanConditionNode::Atom(ConditionAtom::CastFact(CastFactNode {
                fact: CastFact::Kicked,
                ..
            }))
        ),
        "`was kicked`",
    )?;
    Ok(node)
}

#[cfg(test)]
#[path = "condition_controlled_presence_tests.rs"]
mod controlled_presence_tests;

#[cfg(test)]
#[path = "condition_count_comparative_tests.rs"]
mod count_comparative_tests;
