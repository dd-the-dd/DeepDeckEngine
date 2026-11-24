//! `as long as CONDITION` (CR 611.3a), and the readers of the condition
//! language's player and history atoms.
//!
//! The static parent owns `as long as` and the clause boundary (a comma
//! before the body, or the sentence end after it); it reads the Boolean
//! program through the one atom reader (`recognize_condition_atom`) with the
//! static allow-list. The readers below are the shared children of that one
//! grammar for the subjects `you`, a player population, an object quantity,
//! and `there`. Every atom is a reusable proposition that the engine
//! evaluates generically, not a card recipe.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    ClauseAction, ClauseAmount, ClauseAmountPolicy, ClauseFrame, ClauseModality, ClauseSubject,
    ClauseSubjectPolicy, ClauseTense, ConditionAtom, ConditionComparison, ConditionContext,
    ConditionNode, ConditionNumber, ConditionPlayerSubject, ConditionScope,
    ControlledCountDirection, CountThreshold, EventActorScope, EventHistoryNode,
    EventHistoryPattern, HistoryAggregate, HistoryCastMode, HistoryDestination, HistoryEvent,
    HistoryMeasure, HistoryPlayer, HistoryQualifiers, HistoryScope, LIFE_LEMMAS, NumericAtom,
    NumericComparisonOperator, ObjectControllerScope, ObjectKind, ObjectPredicate, PlayerCardZone,
    PlayerDesignation, PlayerDesignationNode, PlayerStateConditionNode, PlayerStateMeasure,
    TargetPlayerLifeAction, ThresholdOperator, recognize_boolean_condition_list_before_separator,
    recognize_clause, recognize_condition_atom, recognize_count_threshold, recognize_numeric_atom,
    recognize_object_query, recognize_player_possession_proposition,
    recognize_search_card_selector, threshold_comparison,
};
use super::{ClauseArguments, VerbForm, VerbLemma, Voice, is_verb_form, lookup_verb};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

pub(in crate::oracle::canonical) type StaticConditionNode = ConditionNode;

/// What the ability's subject allows `it` to mean.
#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) struct StaticConditionContext<'a> {
    /// True when the static ability's subject is its own source, so that `it`
    /// and `this CARD` denote the permanent the condition is evaluated on.
    pub source_subject: bool,
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
}

/// What a static ability's source and subject let a condition name.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::oracle::canonical) struct StaticConditionSubject {
    /// How the source is attached (`enchanted creature`, `equipped
    /// creature`, CR 301.5a, 303.4a), from the source face.
    pub source_attachment: Option<super::AttachedObjectRole>,
    /// The ability's subject is the permanent the source is attached to, so
    /// `it` / `its` denote that permanent (`Enchanted creature gets +2/+2 as
    /// long as it's black`).
    pub attached_subject: bool,
}

/// Reads the Boolean program after `as long as`. The clause terminator (`,` or
/// the sentence end) stays with the parent.
pub(in crate::oracle::canonical) fn recognize_static_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: StaticConditionContext<'_>,
) -> MachineResult<StaticConditionNode> {
    recognize_static_condition_for(cursor, context, StaticConditionSubject::default())
}

/// `recognize_static_condition` for a parent that knows its source's
/// attachment and whether its subject is the attached permanent.
pub(in crate::oracle::canonical) fn recognize_static_condition_for(
    cursor: &mut OracleTokenCursor<'_>,
    context: StaticConditionContext<'_>,
    subject: StaticConditionSubject,
) -> MachineResult<StaticConditionNode> {
    cursor.enter("condition.static.boolean-program");
    let mut context = ConditionContext::standalone(
        ConditionScope::Static,
        context.name_links,
        context.name_failure,
        context.source_subject,
    );
    context.source_attachment = subject.source_attachment;
    context.subject_object = match (subject.attached_subject, subject.source_attachment) {
        (true, Some(role)) => Some(super::ConditionObjectReference::Permanent(
            super::PermanentEffectReference::Attached(role),
        )),
        _ => None,
    };
    recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_condition_atom(cursor, &context)
    })
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

/// `N or more` / `N or fewer` / `a` / `an`: a fixed bounded count comparison.
pub(in crate::oracle::canonical) fn recognize_count_comparison(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(i64, ControlledCountDirection, bool)> {
    cursor.enter("condition.static.count.threshold");
    if word_is(cursor, 0, "a") || word_is(cursor, 0, "an") {
        let article = cursor.peek().expect("article was checked").to_string();
        cursor.consume(&article)?;
        return Ok((1, ControlledCountDirection::OrMore, true));
    }
    let threshold = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) => value,
        NumericAtom::VariableX => {
            return Err(cursor.fail("a fixed nonnegative count threshold"));
        }
    };
    cursor.require(threshold >= 0, "a fixed nonnegative count threshold")?;
    cursor.enter("condition.static.count.direction");
    cursor.consume("or")?;
    let direction = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("more") => ControlledCountDirection::OrMore,
        Some("fewer") => ControlledCountDirection::OrFewer,
        _ => return Err(cursor.fail("`more` or `fewer`")),
    };
    let word = cursor.peek().expect("direction was checked").to_string();
    cursor.consume(&word)?;
    Ok((threshold, direction, false))
}

/// `there are N or more CARDS in your graveyard` / `there's a CARD in your
/// graveyard`: a count over the controller's graveyard through the shared
/// card selector (a bare `cards` counts every card).
pub(in crate::oracle::canonical) fn recognize_graveyard_count_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.static.graveyard-count.existential");
    if contraction_is(cursor, 0, "there", "s") {
        consume_current(cursor)?;
    } else {
        cursor.consume("there")?;
        cursor.enter("condition.static.graveyard-count.copula");
        let copula = cursor
            .peek()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| cursor.fail("`is` or `are`"))?;
        cursor.require(
            copula == "is" || copula == "are",
            "`is` or `are` after `there`",
        )?;
        cursor.consume(&copula)?;
    }
    // `no` (none, CR 609.3-style absence) and `exactly N` read through the
    // shared threshold; `a`, `N or more|fewer` keep the original reader.
    let (threshold, singular) = if word_is(cursor, 0, "no") || word_is(cursor, 0, "exactly") {
        let threshold = recognize_count_threshold(cursor, false)?;
        let singular = threshold.value == 1 && threshold.operator == ThresholdOperator::Exactly;
        (threshold, singular)
    } else {
        let (value, direction, singular) = recognize_count_comparison(cursor)?;
        let operator = match direction {
            ControlledCountDirection::OrMore => ThresholdOperator::AtLeast,
            ControlledCountDirection::OrFewer => ThresholdOperator::AtMost,
        };
        (CountThreshold { operator, value }, singular)
    };
    cursor.enter("condition.static.graveyard-count.cards");
    let predicate = recognize_search_card_selector(cursor, !singular)?;
    cursor.enter("condition.static.graveyard-count.zone");
    cursor.consume("in")?;
    cursor.consume("your")?;
    let zone = if word_is(cursor, 0, "hand") {
        PlayerCardZone::Hand
    } else if word_is(cursor, 0, "graveyard") {
        PlayerCardZone::Graveyard
    } else {
        return Err(cursor.fail("`graveyard` or `hand`"));
    };
    consume_current(cursor)?;
    Ok(ConditionAtom::Compare(ConditionComparison::measured(
        ConditionPlayerSubject::Controller,
        PlayerStateMeasure::CardsInZone { zone, predicate },
        threshold,
    )))
}

/// `you've drawn N or more cards this turn`, `you've cast N or more spells
/// this turn`, `you have N or more life`, `you have an enduring story`,
/// `you've completed a dungeon`, `you haven't completed NAME`.
pub(in crate::oracle::canonical) fn recognize_controller_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.static.controller-history.subject");
    if contraction_is(cursor, 0, "you", "ve") {
        consume_current(cursor)?;
    } else if word_is(cursor, 0, "you") && contraction_is(cursor, 1, "haven", "t") {
        cursor.consume("you")?;
        consume_current(cursor)?;
        cursor.enter("condition.static.controller-history.negated-verb");
        // `you haven't VERBED ... this turn`: the negated perfect of a
        // recorded action, read like `you didn't VERB`.
        if history_verb_at(cursor, 0, VerbForm::PastParticiple).is_some() {
            let verb = history_verb_at(cursor, 0, VerbForm::PastParticiple)
                .expect("the participle was checked");
            consume_current(cursor)?;
            let (pattern, threshold, _) =
                recognize_history_object(cursor, HistoryPlayer::Controller, verb)?;
            let scope = recognize_history_scope(cursor)?;
            return negated_history_node(cursor, pattern, scope, threshold);
        }
        return recognize_completed_dungeon_atom(cursor, false);
    } else {
        cursor.consume("you")?;
        cursor.consume("have")?;
    }
    cursor.enter("condition.static.controller-history.verb");
    if word_is(cursor, 0, "drawn")
        || word_is(cursor, 0, "cast")
        || word_is(cursor, 0, "no")
        || word_is(cursor, 0, "at")
        || word_is(cursor, 0, "exactly")
        || ((word_is(cursor, 0, "fewer") || word_is(cursor, 0, "more"))
            && word_is(cursor, 1, "than"))
        || cursor.peek().is_some_and(|raw| {
            !raw.eq_ignore_ascii_case("an")
                && !raw.eq_ignore_ascii_case("a")
                && (raw.parse::<i64>().is_ok() || parse_number_word(raw).is_some())
        })
    {
        let subject = ConditionPlayerSubject::Controller;
        return recognize_player_possession_proposition(cursor, subject).map(|proposition| {
            super::player_state_atom(PlayerStateConditionNode {
                subject,
                proposition,
            })
        });
    }
    if word_is(cursor, 0, "completed") {
        return recognize_completed_dungeon_atom(cursor, true);
    }
    // `you've attacked this turn`, `you've searched your library this turn`:
    // the perfect of a recorded action is the same player history.
    if history_verb_at(cursor, 0, VerbForm::PastParticiple).is_some() {
        return recognize_history_predicate(
            cursor,
            HistoryPlayer::Controller,
            VerbForm::PastParticiple,
        );
    }
    cursor.enter("condition.static.controller-history.designation");
    // CR 702.131c: `you have the city's blessing`.
    if word_is(cursor, 0, "the") && contraction_is(cursor, 1, "city", "s") {
        cursor.consume("the")?;
        consume_current(cursor)?;
        cursor.consume("blessing")?;
        return Ok(ConditionAtom::PlayerDesignation(PlayerDesignationNode {
            designation: PlayerDesignation::CityBlessing,
            held: true,
        }));
    }
    cursor.consume("an")?;
    cursor.consume("enduring")?;
    cursor.consume("story")?;
    Ok(ConditionAtom::PlayerDesignation(PlayerDesignationNode {
        designation: PlayerDesignation::EnduringStory,
        held: true,
    }))
}

/// `completed a dungeon` or `completed NAME` (CR 309.7). A named Dungeon is
/// read up to the condition's end; it is compared with completed Dungeons.
fn recognize_completed_dungeon_atom(
    cursor: &mut OracleTokenCursor<'_>,
    completed: bool,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.static.completed-dungeon");
    cursor.consume("completed")?;
    let designation = |dungeon| {
        ConditionAtom::PlayerDesignation(PlayerDesignationNode {
            designation: PlayerDesignation::CompletedDungeon { dungeon },
            held: completed,
        })
    };
    if word_is(cursor, 0, "a") && word_is(cursor, 1, "dungeon") {
        cursor.consume("a")?;
        cursor.consume("dungeon")?;
        return Ok(designation(None));
    }
    cursor.enter("condition.static.completed-dungeon.name");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|raw| raw.starts_with(|character: char| character.is_ascii_uppercase())),
        "`a dungeon` or a capitalized Dungeon name",
    )?;
    let mut words = Vec::new();
    while let Some(raw) = cursor.peek() {
        let name_word = raw.chars().all(|character| {
            character.is_alphanumeric() || character == '\'' || character == '\u{2019}'
        }) && (raw.starts_with(|character: char| character.is_ascii_uppercase())
            || matches!(raw, "of" | "the"));
        if !name_word {
            break;
        }
        words.push(raw.to_string());
        cursor.advance_to(cursor.position() + 1)?;
    }
    Ok(designation(Some(words.join(" "))))
}

/// The player subject of a comparative possession and its token length.
fn comparative_subject_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(usize, ConditionPlayerSubject)> {
    if word_is(cursor, 0, "you") {
        Some((1, ConditionPlayerSubject::Controller))
    } else if word_is(cursor, 0, "an") && word_is(cursor, 1, "opponent") {
        Some((
            2,
            ConditionPlayerSubject::Quantified {
                quantifier: super::PlayerQuantifier::Some,
                population: super::PlayerPopulation::Opponents,
            },
        ))
    } else if super::condition_player_reference_starts(cursor) {
        // `that player` / `defending player`: resolved by the binding
        // environment when the atom is read.
        Some((2, ConditionPlayerSubject::EventPlayer))
    } else {
        None
    }
}

/// True at `PLAYER has|have more|fewer|less MEASURE than PLAYER`.
pub(in crate::oracle::canonical) fn player_comparative_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let Some((length, _)) = comparative_subject_at(cursor) else {
        return false;
    };
    (word_is(cursor, length, "has") || word_is(cursor, length, "have"))
        && (word_is(cursor, length + 1, "more")
            || word_is(cursor, length + 1, "fewer")
            || word_is(cursor, length + 1, "less"))
        && !word_is(cursor, length + 2, "than")
}

/// `life`, `cards in hand`, `cards in your|their hand|graveyard`: a measure
/// of each compared player.
fn recognize_comparative_measure(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerStateMeasure> {
    cursor.enter("condition.player.comparative.measure");
    if word_is(cursor, 0, "life") {
        cursor.consume("life")?;
        return Ok(PlayerStateMeasure::Life);
    }
    cursor.consume("cards")?;
    cursor.consume("in")?;
    if word_is(cursor, 0, "your") || word_is(cursor, 0, "their") {
        consume_current(cursor)?;
    }
    let zone = if word_is(cursor, 0, "hand") {
        PlayerCardZone::Hand
    } else if word_is(cursor, 0, "graveyard") {
        PlayerCardZone::Graveyard
    } else {
        return Err(cursor.fail("`hand` or `graveyard`"));
    };
    consume_current(cursor)?;
    Ok(PlayerStateMeasure::CardsInZone {
        zone,
        predicate: ObjectPredicate::ObjectKind(ObjectKind::Card),
    })
}

/// `an opponent has more cards in hand than you`, `you have more life than
/// each opponent`, `you have less life than an opponent`, `that player has
/// more cards in hand than you`: one measure of two players compared. The
/// quantified player is the comparison's subject; the controller's own
/// measure is read whatever that subject (`ControllerMeasure`).
pub(in crate::oracle::canonical) fn recognize_player_comparative_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.player.comparative.subject");
    let (length, subject) =
        comparative_subject_at(cursor).ok_or_else(|| cursor.fail("a player subject"))?;
    let subject = if subject == ConditionPlayerSubject::EventPlayer {
        let environment = super::installed_binding_environment(cursor);
        match super::recognize_condition_player_reference(cursor, &environment)? {
            super::BoundPlayerReference::EventPlayer => ConditionPlayerSubject::EventPlayer,
            super::BoundPlayerReference::DefendingPlayer => ConditionPlayerSubject::DefendingPlayer,
            _ => return Err(cursor.fail("the player the triggering event records")),
        }
    } else {
        for _ in 0..length {
            consume_current(cursor)?;
        }
        subject
    };
    cursor.enter("condition.player.comparative.verb");
    consume_current(cursor)?;
    let operator = if word_is(cursor, 0, "more") {
        NumericComparisonOperator::GreaterThan
    } else {
        NumericComparisonOperator::LessThan
    };
    consume_current(cursor)?;
    let measure = recognize_comparative_measure(cursor)?;
    cursor.enter("condition.player.comparative.than");
    cursor.consume("than")?;
    cursor.enter("condition.player.comparative.other");
    let more = operator == NumericComparisonOperator::GreaterThan;
    let controller = || ConditionNumber::ControllerMeasure(measure.clone());
    // Some opponent exceeds a value iff the greatest opponent value does;
    // every opponent does iff the least one does (CR 102.2-102.3).
    let greatest = || ConditionNumber::GreatestOpponent(measure.clone());
    let least = || ConditionNumber::LeastOpponent(measure.clone());
    let (subject, left, right) = match subject {
        ConditionPlayerSubject::Controller if word_is(cursor, 0, "each") => {
            cursor.consume_phrase(&["each", "opponent"])?;
            let others = if more { greatest() } else { least() };
            (ConditionPlayerSubject::Controller, controller(), others)
        }
        ConditionPlayerSubject::Controller => {
            cursor.consume_phrase(&["an", "opponent"])?;
            let others = if more { least() } else { greatest() };
            (ConditionPlayerSubject::Controller, controller(), others)
        }
        bound @ (ConditionPlayerSubject::EventPlayer | ConditionPlayerSubject::DefendingPlayer) => {
            cursor.consume("you")?;
            (
                bound,
                ConditionNumber::Player(measure.clone()),
                controller(),
            )
        }
        _ => {
            cursor.consume("you")?;
            let opponent = if more { greatest() } else { least() };
            (ConditionPlayerSubject::Controller, opponent, controller())
        }
    };
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject,
        left,
        operator,
        right,
    }))
}

/// `an opponent controls more|fewer ...` or `you control more|fewer ...`: a
/// comparison of two controlled-permanent counts rather than a fixed bound.
pub(in crate::oracle::canonical) fn opponent_count_comparison_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let comparative = |offset| word_is(cursor, offset, "more") || word_is(cursor, offset, "fewer");
    (word_is(cursor, 0, "an") && word_is(cursor, 1, "opponent") && word_is(cursor, 2, "controls"))
        && comparative(3)
        || (word_is(cursor, 0, "you") && word_is(cursor, 1, "control") && comparative(2))
}

/// `an opponent controls more QUERY than you` (the greatest opponent count
/// exceeds the controller's), `you control fewer QUERY than an opponent`
/// (the same comparison), and `you control more QUERY than each opponent`
/// (the controller's count exceeds the greatest opponent count). The
/// permanent query is the shared object query; the comparison is the shared
/// `Compare` over two player measures.
pub(in crate::oracle::canonical) fn recognize_opponent_count_comparison_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.static.count-comparison.subject");
    let opponent_subject = word_is(cursor, 0, "an");
    if opponent_subject {
        cursor.consume("an")?;
        cursor.consume("opponent")?;
        cursor.consume("controls")?;
    } else {
        cursor.consume("you")?;
        cursor.consume("control")?;
    }
    cursor.enter("condition.static.count-comparison.direction");
    let more = word_is(cursor, 0, "more");
    consume_current(cursor)?;
    cursor.enter("condition.static.count-comparison.query");
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller.is_none(),
        "a permanent query without its own controller relation",
    )?;
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent),
        "a predicate valid for a battlefield permanent",
    )?;
    cursor.enter("condition.static.count-comparison.than");
    cursor.consume("than")?;
    cursor.enter("condition.static.count-comparison.other-player");
    let measure = || PlayerStateMeasure::ControlledPermanents {
        predicate: query.predicate.clone(),
        exclude_source: false,
    };
    let controller = ConditionNumber::Player(measure());
    let greatest_opponent = ConditionNumber::GreatestOpponent(measure());
    // Some opponent exceeds a count iff the greatest opponent count does.
    let (left, right) = if opponent_subject {
        cursor.consume("you")?;
        cursor.require(
            more,
            "`more ... than you`; an opponent with fewer permanents needs the least opponent count",
        )?;
        (greatest_opponent, controller)
    } else if word_is(cursor, 0, "an") {
        cursor.consume("an")?;
        cursor.consume("opponent")?;
        cursor.require(
            !more,
            "`fewer ... than an opponent`; more than some opponent needs the least opponent count",
        )?;
        (greatest_opponent, controller)
    } else {
        cursor.consume("each")?;
        cursor.consume("opponent")?;
        cursor.require(
            more,
            "`more ... than each opponent`; fewer than every opponent needs the least opponent count",
        )?;
        (controller, greatest_opponent)
    };
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left,
        operator: NumericComparisonOperator::GreaterThan,
        right,
    }))
}

fn life_history_frame() -> ClauseFrame<'static> {
    ClauseFrame {
        subject: ClauseSubjectPolicy::PlayerScope,
        modality: ClauseModality::Indicative,
        tense: ClauseTense::PastThisTurn,
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::Threshold)],
        lemmas: LIFE_LEMMAS,
    }
}

/// `PLAYER gained|lost [N or more] life this turn`: the past interpretation
/// of the shared life clause, as an event history over the life events of
/// that player relation.
pub(in crate::oracle::canonical) fn try_recognize_life_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ConditionAtom>> {
    let Some(clause) = recognize_clause(cursor, &life_history_frame()).into_result()? else {
        return Ok(None);
    };
    cursor.enter("condition.static.life-history");
    let ClauseAction::LifeTotalChange { direction, amount } = clause.action else {
        return Err(cursor.fail("a life-total change"));
    };
    let ClauseSubject::Scope {
        actor,
        one_or_more: false,
    } = clause.subject
    else {
        return Err(cursor.fail("a singular player scope for a life history"));
    };
    let player = match actor {
        EventActorScope::Controller => HistoryPlayer::Controller,
        EventActorScope::Opponent => HistoryPlayer::Opponent,
        EventActorScope::AnyPlayer => HistoryPlayer::Any,
    };
    let threshold = match amount {
        ClauseAmount::Unspecified => 1,
        ClauseAmount::Count {
            value,
            direction: ControlledCountDirection::OrMore,
        } if value >= 1 => value,
        _ => {
            return Err(cursor.fail("life, or `N or more life` with N of at least one"));
        }
    };
    cursor.enter("condition.static.life-history.per-player");
    // A quantified other player who gained N life: one player's sum, not
    // the sum over the population.
    cursor.require(
        threshold == 1 || player == HistoryPlayer::Controller,
        "`N or more life` of the controller; a population's per-player sum is unsupported",
    )?;
    Ok(Some(ConditionAtom::EventHistory(EventHistoryNode {
        pattern: EventHistoryPattern {
            event: match direction {
                TargetPlayerLifeAction::Gain => HistoryEvent::LifeGained,
                TargetPlayerLifeAction::Lose => HistoryEvent::LifeLost,
            },
            player,
            object: None,
            measure: HistoryMeasure::Amount,
            qualifiers: HistoryQualifiers::default(),
        },
        scope: HistoryScope::ThisTurn,
        aggregate: HistoryAggregate::Total,
        operator: NumericComparisonOperator::GreaterThanOrEqual,
        threshold,
    })))
}

/// The player subject of a history clause and its token length.
fn history_actor_at(cursor: &OracleTokenCursor<'_>) -> Option<(usize, HistoryPlayer)> {
    if word_is(cursor, 0, "you") {
        Some((1, HistoryPlayer::Controller))
    } else if word_is(cursor, 0, "an") && word_is(cursor, 1, "opponent") {
        Some((2, HistoryPlayer::Opponent))
    } else if word_is(cursor, 0, "a") && word_is(cursor, 1, "player") {
        Some((2, HistoryPlayer::Any))
    } else {
        None
    }
}

/// A recorded player action of the history lexicon. The verb only selects
/// the recorded event and the argument frame of its object; the subject,
/// the amount, the shared object query and the turn are shared children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActorHistoryVerb {
    Cast,
    Search,
    Attack,
    Sacrifice,
    Discard,
    Draw,
    Cycle,
    Create,
    Put,
    Play,
    Descend,
    Gain,
    Lose,
}

/// The lemmas of the shared verb lexicon that name a recorded player action.
const ACTOR_HISTORY_LEMMAS: &[(VerbLemma, ActorHistoryVerb)] = &[
    (VerbLemma::Cast, ActorHistoryVerb::Cast),
    (VerbLemma::Search, ActorHistoryVerb::Search),
    (VerbLemma::Attack, ActorHistoryVerb::Attack),
    (VerbLemma::Sacrifice, ActorHistoryVerb::Sacrifice),
    (VerbLemma::Discard, ActorHistoryVerb::Discard),
    (VerbLemma::Draw, ActorHistoryVerb::Draw),
    (VerbLemma::Cycle, ActorHistoryVerb::Cycle),
    (VerbLemma::Create, ActorHistoryVerb::Create),
    (VerbLemma::Put, ActorHistoryVerb::Put),
    (VerbLemma::Gain, ActorHistoryVerb::Gain),
    (VerbLemma::Lose, ActorHistoryVerb::Lose),
    // Event-only lexicon rows: playing a land (CR 305.1) and descending (a
    // permanent card put into that player's graveyard from anywhere).
    (VerbLemma::Play, ActorHistoryVerb::Play),
    (VerbLemma::Descend, ActorHistoryVerb::Descend),
];

/// The history verb at `offset` in `form`: the simple past after a subject,
/// the past participle after a perfect `have`, the base after `didn't`.
fn history_verb_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    form: VerbForm,
) -> Option<ActorHistoryVerb> {
    let raw = cursor.peek_ahead(offset)?;
    let lemmas = ACTOR_HISTORY_LEMMAS
        .iter()
        .map(|(lemma, _)| *lemma)
        .collect::<Vec<_>>();
    let (entry, forms) = lookup_verb(raw, &lemmas)?;
    if !forms.contains(&form) {
        return None;
    }
    ACTOR_HISTORY_LEMMAS
        .iter()
        .find(|(lemma, _)| *lemma == entry.lemma)
        .map(|(_, verb)| *verb)
}

/// The width of a past negation (`didn't`, `did not`) at `offset`.
fn past_negation_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> usize {
    if contraction_is(cursor, offset, "didn", "t") {
        1
    } else if word_is(cursor, offset, "did") && word_is(cursor, offset + 1, "not") {
        2
    } else {
        0
    }
}

/// The event a causative `had OBJECTS VERB` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CausativeHistory {
    /// `had OBJECTS enter the battlefield` (CR 603.6a).
    Entered,
    /// `had CARDS put into their graveyard from anywhere`.
    PutIntoGraveyard,
}

/// Locates the infinitive of a causative history after `had` without reading
/// the object phrase between them: its offset and the reported event.
fn causative_history_at(
    cursor: &OracleTokenCursor<'_>,
    from: usize,
) -> Option<(usize, CausativeHistory)> {
    (from + 1..from + 14).find_map(|offset| {
        let raw = cursor.peek_ahead(offset)?;
        if matches!(raw, "," | "." | ";") {
            return None;
        }
        if is_verb_form(raw, VerbLemma::Enter, VerbForm::Base)
            && word_is(cursor, offset + 1, "the")
            && word_is(cursor, offset + 2, "battlefield")
        {
            return Some((offset, CausativeHistory::Entered));
        }
        if is_verb_form(raw, VerbLemma::Put, VerbForm::PastParticiple)
            && word_is(cursor, offset + 1, "into")
        {
            return Some((offset, CausativeHistory::PutIntoGraveyard));
        }
        None
    })
}

/// True at `PLAYER VERBED ...` (simple past of a recorded action), `PLAYER
/// didn't VERB ...`, `PLAYER had OBJECTS enter|put ...` or `PLAYER was|were
/// dealt damage`.
pub(in crate::oracle::canonical) fn actor_event_history_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let Some((length, _)) = history_actor_at(cursor) else {
        return false;
    };
    let negation = past_negation_width(cursor, length);
    if negation > 0 {
        return history_verb_at(cursor, length + negation, VerbForm::Base).is_some();
    }
    let past = history_verb_at(cursor, length, VerbForm::Past).is_some();
    let causative =
        word_is(cursor, length, "had") && causative_history_at(cursor, length + 1).is_some();
    let dealt = (word_is(cursor, length, "was") || word_is(cursor, length, "were"))
        && cursor
            .peek_ahead(length + 1)
            .is_some_and(|raw| is_verb_form(raw, VerbLemma::Deal, VerbForm::PastParticiple));
    past || causative || dealt
}

/// `this turn` or `last turn`. A result of this program (`this way`) is an
/// action result, never a turn history: mixing both scopes fails here.
fn recognize_history_scope(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<HistoryScope> {
    cursor.enter("condition.history.scope");
    let scope = if word_is(cursor, 0, "this") && word_is(cursor, 1, "turn") {
        HistoryScope::ThisTurn
    } else if word_is(cursor, 0, "last") {
        HistoryScope::LastTurn
    } else {
        return Err(cursor.fail("`this turn` or `last turn`"));
    };
    consume_current(cursor)?;
    cursor.consume("turn")?;
    Ok(scope)
}

/// How a threshold over a player relation aggregates: `a player cast two or
/// more spells` is one player's count; `an opponent ...` likewise. An upper
/// bound over a population (`an opponent cast fewer ...`) needs the least
/// per-player count and is refused.
fn history_aggregate(
    cursor: &mut OracleTokenCursor<'_>,
    player: HistoryPlayer,
    threshold: CountThreshold,
) -> MachineResult<HistoryAggregate> {
    if player == HistoryPlayer::Controller {
        return Ok(HistoryAggregate::Total);
    }
    cursor.enter("condition.history.aggregate");
    match threshold.operator {
        ThresholdOperator::AtLeast if threshold.value <= 1 => Ok(HistoryAggregate::Total),
        ThresholdOperator::AtLeast => Ok(HistoryAggregate::GreatestPerPlayer),
        ThresholdOperator::AtMost | ThresholdOperator::Exactly => Err(cursor.fail(
            "a lower bound for a player population's history (an upper or exact bound needs every per-player count)",
        )),
    }
}

fn history_node(
    pattern: EventHistoryPattern,
    scope: HistoryScope,
    aggregate: HistoryAggregate,
    threshold: CountThreshold,
) -> ConditionAtom {
    let (operator, threshold) = match threshold_comparison(threshold) {
        // No event happened: the count is zero.
        (NumericComparisonOperator::LessThanOrEqual, 0) => (NumericComparisonOperator::Equal, 0),
        comparison => comparison,
    };
    ConditionAtom::EventHistory(EventHistoryNode {
        pattern,
        scope,
        aggregate,
        operator,
        threshold,
    })
}

/// `PLAYER didn't VERB ...`: the complement of the stated lower bound over
/// the subject's own total (`didn't cast a spell` is zero spells). A
/// population's negation would need every per-player count; it is refused.
fn negated_history_node(
    cursor: &OracleTokenCursor<'_>,
    pattern: EventHistoryPattern,
    scope: HistoryScope,
    threshold: CountThreshold,
) -> MachineResult<ConditionAtom> {
    cursor.require(
        pattern.player == HistoryPlayer::Controller,
        "a negated history of the controller (a population's negation needs every per-player count)",
    )?;
    let (operator, value) = match threshold.operator {
        ThresholdOperator::AtLeast if threshold.value == 1 => (NumericComparisonOperator::Equal, 0),
        ThresholdOperator::AtLeast => (NumericComparisonOperator::LessThan, threshold.value),
        ThresholdOperator::AtMost => (NumericComparisonOperator::GreaterThan, threshold.value),
        ThresholdOperator::Exactly => {
            return Err(cursor.fail("a lower or upper bound under `didn't`, not an exact count"));
        }
    };
    Ok(ConditionAtom::EventHistory(EventHistoryNode {
        pattern,
        scope,
        aggregate: HistoryAggregate::Total,
        operator,
        threshold: value,
    }))
}

/// The predicate, unless it only restates the event object's domain.
fn object_filter(predicate: ObjectPredicate, domain: ObjectKind) -> Option<ObjectPredicate> {
    (predicate != ObjectPredicate::ObjectKind(domain)).then_some(predicate)
}

/// The predicate without the atom every object of the event already has
/// (`a token` of a token creation, `a land` of a land play). A predicate
/// that does not state the atom is refused by the caller.
fn without_restated_atom(
    predicate: ObjectPredicate,
    restated: &ObjectPredicate,
) -> Option<Option<ObjectPredicate>> {
    match predicate {
        predicate if &predicate == restated => Some(None),
        ObjectPredicate::Intersection(operands) if operands.contains(restated) => {
            let mut rest = operands
                .into_iter()
                .filter(|operand| operand != restated)
                .collect::<Vec<_>>();
            Some(Some(match rest.len() {
                1 => rest.remove(0),
                _ => ObjectPredicate::Intersection(rest),
            }))
        }
        _ => None,
    }
}

/// A history pattern without qualifiers.
fn plain_pattern(
    event: HistoryEvent,
    player: HistoryPlayer,
    object: Option<ObjectPredicate>,
    measure: HistoryMeasure,
) -> EventHistoryPattern {
    EventHistoryPattern {
        event,
        player,
        object,
        measure,
        qualifiers: HistoryQualifiers::default(),
    }
}

/// The amount of a counted object phrase: the shared threshold, or
/// `another` (at least one object other than the ability's source).
fn recognize_history_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(CountThreshold, bool)> {
    if word_is(cursor, 0, "another") {
        cursor.consume("another")?;
        return Ok((CountThreshold::at_least(1), true));
    }
    Ok((recognize_count_threshold(cursor, true)?, false))
}

/// `a card`, `two or more nonland cards`: the shared card selector, whose
/// noun agrees with the amount in number.
fn recognize_counted_cards(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    let mut singular = cursor.clone();
    if let Ok(predicate) = recognize_search_card_selector(&mut singular, false) {
        *cursor = singular;
        return Ok(predicate);
    }
    recognize_search_card_selector(cursor, true)
}

/// A counted permanent query of a recorded departure or entry: no control
/// relation of its own, valid for a battlefield permanent.
fn recognize_counted_permanents(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ObjectQueryNode> {
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller.is_none() && !query.exclude_source,
        "a counted permanent without its own controller relation or `other`",
    )?;
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent),
        "a predicate valid for a battlefield permanent",
    )?;
    Ok(query)
}

/// `you attacked [with N CREATURES] this turn`, `an opponent searched their
/// library this turn`, `a player cast two or more spells last turn`, `you
/// sacrificed a Food this turn`, `you didn't play a land this turn`, `you had
/// a land enter the battlefield under your control this turn`, `an opponent
/// was dealt damage this turn`: the player relation, a recorded action, an
/// optional counted object and the turn.
pub(in crate::oracle::canonical) fn recognize_actor_event_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.history.actor");
    let (length, player) =
        history_actor_at(cursor).ok_or_else(|| cursor.fail("a player subject"))?;
    for _ in 0..length {
        consume_current(cursor)?;
    }
    let negation = past_negation_width(cursor, 0);
    if negation > 0 {
        cursor.enter("condition.history.negation");
        for _ in 0..negation {
            consume_current(cursor)?;
        }
        return recognize_history_predicate(cursor, player, VerbForm::Base);
    }
    if word_is(cursor, 0, "had") && causative_history_at(cursor, 1).is_some() {
        return recognize_causative_history(cursor, player);
    }
    recognize_history_predicate(cursor, player, VerbForm::Past)
}

/// `PLAYER had OBJECTS enter the battlefield [under their control]` and
/// `PLAYER had CARDS put into their graveyard from anywhere` (the causative
/// `have` of a recorded zone change, CR 603.6a, 400.7): the player is the
/// controller of the entering permanents, or the owner of the graveyard.
fn recognize_causative_history(
    cursor: &mut OracleTokenCursor<'_>,
    player: HistoryPlayer,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.history.causative");
    let (offset, event) =
        causative_history_at(cursor, 1).ok_or_else(|| cursor.fail("`had OBJECTS VERB`"))?;
    let verb_position = cursor.position() + offset;
    cursor.consume("had")?;
    cursor.enter("condition.history.causative.amount");
    let (threshold, exclude_source) = recognize_history_amount(cursor)?;
    cursor.require(
        cursor.position() < verb_position,
        "a counted object phrase before the verb",
    )?;
    cursor.enter("condition.history.causative.objects");
    let possessive = if player == HistoryPlayer::Controller {
        "your"
    } else {
        "their"
    };
    let pattern = match event {
        CausativeHistory::Entered => {
            let query = cursor.bounded(verb_position, recognize_counted_permanents)?;
            if predicate_reads_object_state(&query.predicate) {
                return Err(cursor.engine_gap(
                    crate::oracle::canonical::engine_gap::EngineGap::EventHistoryState,
                    "a characteristic the entry record keeps (a face-down, tapped, designation or counter state is not recorded)",
                ));
            }
            cursor.enter("condition.history.causative.enter");
            consume_current(cursor)?;
            cursor.consume_phrase(&["the", "battlefield"])?;
            if word_is(cursor, 0, "under") {
                cursor.enter("condition.history.causative.control");
                cursor.consume("under")?;
                cursor.require(
                    word_is(cursor, 0, possessive),
                    "the control possessive that agrees with the history's player subject",
                )?;
                consume_current(cursor)?;
                cursor.consume("control")?;
            }
            EventHistoryPattern {
                event: HistoryEvent::PermanentEntered,
                player,
                object: object_filter(query.predicate, ObjectKind::Permanent),
                measure: HistoryMeasure::Occurrences,
                qualifiers: HistoryQualifiers {
                    exclude_source,
                    ..HistoryQualifiers::default()
                },
            }
        }
        CausativeHistory::PutIntoGraveyard => {
            cursor.require(!exclude_source, "counted cards without `another`")?;
            let predicate = cursor.bounded(verb_position, recognize_counted_cards)?;
            cursor.enter("condition.history.causative.put");
            consume_current(cursor)?;
            cursor.consume("into")?;
            cursor.require(
                word_is(cursor, 0, possessive),
                "the graveyard possessive that agrees with the history's player subject",
            )?;
            consume_current(cursor)?;
            cursor.consume("graveyard")?;
            cursor.enter("condition.history.causative.origin");
            cursor.consume_phrase(&["from", "anywhere"])?;
            plain_pattern(
                HistoryEvent::CardsPutIntoGraveyard,
                player,
                object_filter(predicate, ObjectKind::Card),
                HistoryMeasure::Cards,
            )
        }
    };
    let aggregate = history_aggregate(cursor, player, threshold)?;
    let scope = recognize_history_scope(cursor)?;
    Ok(history_node(pattern, scope, aggregate, threshold))
}

/// The verb phrase of a player-subject history after its subject (and after
/// a perfect `have`, whose participle has the same surface, or `didn't`,
/// whose verb is the base form and negates the history).
fn recognize_history_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    player: HistoryPlayer,
    form: VerbForm,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.history.verb");
    let negated = form == VerbForm::Base;
    if !negated && (word_is(cursor, 0, "was") || word_is(cursor, 0, "were")) {
        consume_current(cursor)?;
        consume_current(cursor)?;
        cursor.enter("condition.history.damage");
        cursor.consume("damage")?;
        let scope = recognize_history_scope(cursor)?;
        let threshold = CountThreshold::at_least(1);
        return Ok(history_node(
            plain_pattern(
                HistoryEvent::DamageDealtToPlayer,
                player,
                None,
                HistoryMeasure::Amount,
            ),
            scope,
            HistoryAggregate::Total,
            threshold,
        ));
    }
    let verb = history_verb_at(cursor, 0, form)
        .ok_or_else(|| cursor.fail("a recorded action in the simple past"))?;
    consume_current(cursor)?;
    let (pattern, threshold, aggregate_allowed) = recognize_history_object(cursor, player, verb)?;
    let scope = recognize_history_scope(cursor)?;
    if negated {
        return negated_history_node(cursor, pattern, scope, threshold);
    }
    let aggregate = if aggregate_allowed {
        history_aggregate(cursor, player, threshold)?
    } else {
        HistoryAggregate::Total
    };
    Ok(history_node(pattern, scope, aggregate, threshold))
}

/// The argument frame of each recorded action: its counted object (a
/// spell, a permanent, a card, a token, counters on a permanent), and
/// whether a population's lower bound reads one player's count.
fn recognize_history_object(
    cursor: &mut OracleTokenCursor<'_>,
    player: HistoryPlayer,
    verb: ActorHistoryVerb,
) -> MachineResult<(EventHistoryPattern, CountThreshold, bool)> {
    match verb {
        ActorHistoryVerb::Cast => {
            cursor.enter("condition.history.cast.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.enter("condition.history.cast.query");
            let query =
                super::recognize_object_noun_query(cursor, super::ObjectQueryContext::ORDINARY)?
                    .into_spell_cast_query(cursor)?;
            cursor.require(
                query.caster.is_none(),
                "one caster relation, supplied by the history's player subject",
            )?;
            cursor.require(
                query.predicate.has_domain(ObjectKind::Spell)
                    && query.predicate.supports_domain(ObjectKind::Spell),
                "an explicit spell-domain query",
            )?;
            Ok((
                plain_pattern(
                    HistoryEvent::SpellCast,
                    player,
                    object_filter(query.predicate, ObjectKind::Spell),
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Search => {
            cursor.enter("condition.history.search.zone");
            let possessive_agrees = if player == HistoryPlayer::Controller {
                word_is(cursor, 0, "your")
            } else {
                word_is(cursor, 0, "their")
            };
            cursor.require(
                possessive_agrees,
                "the library possessive that agrees with the history's player subject",
            )?;
            consume_current(cursor)?;
            cursor.consume("library")?;
            Ok((
                plain_pattern(
                    HistoryEvent::LibrarySearched,
                    player,
                    None,
                    HistoryMeasure::Occurrences,
                ),
                CountThreshold::at_least(1),
                false,
            ))
        }
        ActorHistoryVerb::Attack => {
            cursor.enter("condition.history.attack.with");
            let (object, threshold) = if word_is(cursor, 0, "with") {
                cursor.consume("with")?;
                let threshold = recognize_count_threshold(cursor, true)?;
                cursor.enter("condition.history.attack.attackers");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller.is_none() && !query.exclude_source,
                    "attackers without their own controller relation",
                )?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent),
                    "a predicate valid for an attacking permanent",
                )?;
                (
                    object_filter(query.predicate, ObjectKind::Permanent),
                    threshold,
                )
            } else {
                (None, CountThreshold::at_least(1))
            };
            Ok((
                plain_pattern(
                    HistoryEvent::AttackerDeclared,
                    player,
                    object,
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Sacrifice => {
            // CR 701.21: the sacrifice record keeps the permanent's type line.
            cursor.enter("condition.history.sacrifice.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.enter("condition.history.sacrifice.objects");
            let query = recognize_counted_permanents(cursor)?;
            cursor.require(
                type_line_predicate(&query.predicate),
                "a type-line characteristic of a sacrificed permanent (other last-known characteristics are not recorded)",
            )?;
            Ok((
                plain_pattern(
                    HistoryEvent::PermanentSacrificed,
                    player,
                    object_filter(query.predicate, ObjectKind::Permanent),
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Discard | ActorHistoryVerb::Cycle => {
            // CR 701.9, 702.29: one record per card, read on the card.
            cursor.enter("condition.history.card.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.enter("condition.history.card.objects");
            let predicate = recognize_counted_cards(cursor)?;
            let event = if verb == ActorHistoryVerb::Discard {
                HistoryEvent::CardDiscarded
            } else {
                HistoryEvent::CardCycled
            };
            Ok((
                plain_pattern(
                    event,
                    player,
                    object_filter(predicate, ObjectKind::Card),
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Draw => {
            // CR 121.1: one draw record per card.
            cursor.enter("condition.history.draw.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.enter("condition.history.draw.noun");
            cursor.require(
                word_is(cursor, 0, "card") || word_is(cursor, 0, "cards"),
                "`card` or `cards` after a draw amount",
            )?;
            consume_current(cursor)?;
            Ok((
                plain_pattern(
                    HistoryEvent::CardDrawn,
                    player,
                    None,
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Create => {
            // CR 111.1: the creation record keeps the token's type line.
            cursor.enter("condition.history.create.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.enter("condition.history.create.tokens");
            let query = recognize_object_query(cursor)?;
            cursor.require(
                query.controller.is_none() && !query.exclude_source,
                "created tokens without a controller relation",
            )?;
            let object = without_restated_atom(query.predicate, &ObjectPredicate::IsToken)
                .ok_or_else(|| cursor.fail("a token noun after a creation"))?
                .and_then(|predicate| object_filter(predicate, ObjectKind::Permanent));
            cursor.require(
                object.as_ref().is_none_or(type_line_predicate),
                "a type-line characteristic of a created token",
            )?;
            Ok((
                plain_pattern(
                    HistoryEvent::TokenCreated,
                    player,
                    object,
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Play => {
            // CR 305.1: playing a land is its own recorded action.
            cursor.enter("condition.history.play.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.enter("condition.history.play.lands");
            let predicate = recognize_counted_cards_or_permanents(cursor)?;
            let land = ObjectPredicate::CardType(super::ObjectCardType::Land);
            let object = without_restated_atom(predicate, &land)
                .ok_or_else(|| cursor.fail("a land noun after `played`"))?
                .and_then(|predicate| object_filter(predicate, ObjectKind::Permanent))
                .and_then(|predicate| object_filter(predicate, ObjectKind::Card));
            Ok((
                plain_pattern(
                    HistoryEvent::LandPlayed,
                    player,
                    object,
                    HistoryMeasure::Occurrences,
                ),
                threshold,
                true,
            ))
        }
        ActorHistoryVerb::Put => {
            // CR 122.1: `put N [KIND] counters on OBJECT`; the actor is the
            // player who put them, the permanent's controller a filter.
            cursor.enter("condition.history.counters.amount");
            let threshold = recognize_count_threshold(cursor, true)?;
            cursor.require(
                threshold == CountThreshold::at_least(1),
                "`a`, `one or more` counters (an amount of counters is not a count of placements)",
            )?;
            cursor.enter("condition.history.counters.kind");
            let counter = if word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters") {
                None
            } else {
                Some(super::recognize_counter_name(cursor)?)
            };
            cursor.enter("condition.history.counters.noun");
            cursor.require(
                word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters"),
                "`counter` or `counters`",
            )?;
            consume_current(cursor)?;
            cursor.consume("on")?;
            cursor.enter("condition.history.counters.recipient");
            cursor.require(
                word_is(cursor, 0, "a") || word_is(cursor, 0, "an"),
                "`a`/`an` before the permanent that received counters",
            )?;
            consume_current(cursor)?;
            let query = recognize_object_query(cursor)?;
            cursor.require(
                !query.exclude_source,
                "a permanent receiving counters without `other`",
            )?;
            cursor.require(
                query.predicate.supports_domain(ObjectKind::Permanent)
                    && type_line_predicate_or_domain(&query.predicate),
                "a type-line characteristic of the permanent that received counters",
            )?;
            let recipient_controller = match query.controller {
                None => None,
                Some(ObjectControllerScope::AbilityController) => Some(HistoryPlayer::Controller),
                Some(ObjectControllerScope::OpponentOfAbilityController) => {
                    Some(HistoryPlayer::Opponent)
                }
                Some(_) => {
                    return Err(cursor.fail("a controller relation of the permanent with counters"));
                }
            };
            Ok((
                EventHistoryPattern {
                    event: HistoryEvent::CountersPlaced,
                    player,
                    object: object_filter(query.predicate, ObjectKind::Permanent),
                    measure: HistoryMeasure::Occurrences,
                    qualifiers: HistoryQualifiers {
                        counter,
                        recipient_controller,
                        ..HistoryQualifiers::default()
                    },
                },
                threshold,
                false,
            ))
        }
        ActorHistoryVerb::Descend => {
            // A player descends when a permanent card is put into their
            // graveyard from anywhere: one graveyard arrival of such a card.
            Ok((
                plain_pattern(
                    HistoryEvent::CardsPutIntoGraveyard,
                    player,
                    Some(ObjectPredicate::PermanentCard),
                    HistoryMeasure::Cards,
                ),
                CountThreshold::at_least(1),
                false,
            ))
        }
        ActorHistoryVerb::Gain | ActorHistoryVerb::Lose => {
            // `gained life` alone is the shared life clause's; this reader
            // owns only the coordinated `gained or lost life` (CR 119.3).
            cursor.enter("condition.history.life.coordination");
            cursor.consume("or")?;
            let other = if verb == ActorHistoryVerb::Gain {
                ActorHistoryVerb::Lose
            } else {
                ActorHistoryVerb::Gain
            };
            cursor.require(
                history_verb_at(cursor, 0, VerbForm::Past) == Some(other),
                "`gained or lost` life",
            )?;
            consume_current(cursor)?;
            cursor.consume("life")?;
            Ok((
                plain_pattern(
                    HistoryEvent::LifeChanged,
                    player,
                    None,
                    HistoryMeasure::Amount,
                ),
                CountThreshold::at_least(1),
                false,
            ))
        }
    }
}

/// `a land`, `two or more lands`: a land is played from a card zone, so the
/// noun reads as the shared permanent query (`land`) or card selector.
fn recognize_counted_cards_or_permanents(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller.is_none() && !query.exclude_source,
        "a played land without a controller relation",
    )?;
    Ok(query.predicate)
}

/// The verb of an object-quantity history and its position after the
/// counted object phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectHistoryVerb {
    Died,
    LeftBattlefield,
    EnteredBattlefield,
    WasCast,
    /// `was cast for its MODE cost`: a cast mode participle (`warped`).
    WasCastWithMode(HistoryCastMode),
    /// `attacked` (CR 508.1a): declared as an attacker.
    Attacked,
    /// `was put into a graveyard from the battlefield|anywhere`.
    PutIntoGraveyard,
    /// `was put into a hand from the battlefield`.
    PutIntoHand,
}

/// Casting-mode participles: the decision the cast record keeps for a
/// spell cast with that mode.
const CAST_MODE_PARTICIPLES: &[(&str, HistoryCastMode)] = &[("warped", HistoryCastMode::Warp)];

/// Finds the history verb that follows the counted object phrase, without
/// reading the phrase.
fn object_history_verb_at(cursor: &OracleTokenCursor<'_>) -> Option<(usize, ObjectHistoryVerb)> {
    (1..12).find_map(|offset| {
        let raw = cursor.peek_ahead(offset)?;
        if matches!(raw, "," | "." | ";") {
            return None;
        }
        if is_verb_form(raw, VerbLemma::Die, VerbForm::Past) {
            return Some((offset, ObjectHistoryVerb::Died));
        }
        if is_verb_form(raw, VerbLemma::Leave, VerbForm::Past)
            && word_is(cursor, offset + 1, "the")
            && word_is(cursor, offset + 2, "battlefield")
        {
            return Some((offset, ObjectHistoryVerb::LeftBattlefield));
        }
        if is_verb_form(raw, VerbLemma::Enter, VerbForm::Past)
            && word_is(cursor, offset + 1, "the")
            && word_is(cursor, offset + 2, "battlefield")
        {
            return Some((offset, ObjectHistoryVerb::EnteredBattlefield));
        }
        if is_verb_form(raw, VerbLemma::Attack, VerbForm::Past)
            && (word_is(cursor, offset + 1, "this") || word_is(cursor, offset + 1, "last"))
            && word_is(cursor, offset + 2, "turn")
        {
            return Some((offset, ObjectHistoryVerb::Attacked));
        }
        if raw.eq_ignore_ascii_case("was") || raw.eq_ignore_ascii_case("were") {
            let next = cursor.peek_ahead(offset + 1)?;
            if is_verb_form(next, VerbLemma::Cast, VerbForm::PastParticiple) {
                return Some((offset, ObjectHistoryVerb::WasCast));
            }
            if let Some((_, mode)) = CAST_MODE_PARTICIPLES
                .iter()
                .find(|(participle, _)| next.eq_ignore_ascii_case(participle))
            {
                return Some((offset, ObjectHistoryVerb::WasCastWithMode(*mode)));
            }
            if is_verb_form(next, VerbLemma::Put, VerbForm::PastParticiple)
                && word_is(cursor, offset + 2, "into")
            {
                let zone = (offset + 3..offset + 6).find_map(|index| {
                    let word = cursor.peek_ahead(index)?;
                    ["graveyard", "hand"]
                        .iter()
                        .find(|zone| word.eq_ignore_ascii_case(zone))
                        .copied()
                })?;
                return Some((
                    offset,
                    if zone == "hand" {
                        ObjectHistoryVerb::PutIntoHand
                    } else {
                        ObjectHistoryVerb::PutIntoGraveyard
                    },
                ));
            }
        }
        None
    })
}

/// True at `COUNT OBJECT died|left the battlefield|entered the battlefield|
/// attacked|was cast|was put into a zone`.
pub(in crate::oracle::canonical) fn object_event_history_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let count_word = word_is(cursor, 0, "no")
        || word_is(cursor, 0, "a")
        || word_is(cursor, 0, "an")
        || word_is(cursor, 0, "at")
        || word_is(cursor, 0, "another")
        || cursor
            .peek()
            .is_some_and(|raw| raw.parse::<i64>().is_ok() || parse_number_word(raw).is_some());
    count_word && object_history_verb_at(cursor).is_some()
}

/// Whether the predicate reads a permanent state rather than a definition.
fn predicate_reads_object_state(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::Status(_)
        | ObjectPredicate::Designation(_)
        | ObjectPredicate::HasCounter(_)
        | ObjectPredicate::ChosenCreatureType
        | ObjectPredicate::ChosenColor => true,
        ObjectPredicate::Not(operand) => predicate_reads_object_state(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().any(predicate_reads_object_state)
        }
        _ => false,
    }
}

/// Whether a predicate is evaluable from the recorded type line of an object
/// that left the battlefield (the event keeps its type line, not its other
/// characteristics).
fn type_line_predicate(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::ObjectKind(ObjectKind::Permanent) => true,
        ObjectPredicate::Not(operand) => type_line_predicate(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().all(type_line_predicate)
        }
        _ => false,
    }
}

fn type_line_predicate_or_domain(predicate: &ObjectPredicate) -> bool {
    type_line_predicate(predicate)
        || *predicate == ObjectPredicate::ObjectKind(ObjectKind::Permanent)
}

/// `your graveyard` / `a graveyard` / `an opponent's graveyard` after `into`:
/// the owner relation of the zone, if any.
fn recognize_owned_zone(
    cursor: &mut OracleTokenCursor<'_>,
    zone: &str,
) -> MachineResult<Option<HistoryPlayer>> {
    cursor.enter("condition.history.object.zone-owner");
    let owner = if word_is(cursor, 0, "your") {
        consume_current(cursor)?;
        Some(HistoryPlayer::Controller)
    } else if word_is(cursor, 0, "an") && contraction_is(cursor, 1, "opponent", "s") {
        consume_current(cursor)?;
        consume_current(cursor)?;
        Some(HistoryPlayer::Opponent)
    } else if word_is(cursor, 0, "a") {
        consume_current(cursor)?;
        None
    } else {
        return Err(cursor.fail("`your`, `a` or `an opponent's` zone"));
    };
    cursor.enter("condition.history.object.zone");
    cursor.consume(zone)?;
    Ok(owner)
}

/// `a creature died this turn`, `two or more Elves died this turn`, `a
/// permanent left the battlefield under your control this turn`, `no spells
/// were cast last turn`, `a spell was warped this turn`, `no creatures
/// attacked this turn`, `an enchantment was put into your graveyard from the
/// battlefield this turn`: a counted object phrase of the shared object
/// query, a recorded zone change or cast, an optional control relation and
/// the turn.
pub(in crate::oracle::canonical) fn recognize_object_event_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.history.object.verb-position");
    // The verb is located before the count is read, so that the object
    // phrase between them is read by one bounded child.
    let (offset, verb) = object_history_verb_at(cursor).ok_or_else(|| {
        cursor.fail("`died`, `left the battlefield`, `entered the battlefield` or `was cast`")
    })?;
    let verb_position = cursor.position() + offset;
    // `from anywhere` names the graveyard arrival of cards, `from the
    // battlefield` a permanent's departure.
    let from_anywhere = verb == ObjectHistoryVerb::PutIntoGraveyard
        && (offset..offset + 10)
            .take_while(|index| {
                cursor
                    .peek_ahead(*index)
                    .is_some_and(|raw| !matches!(raw, "," | "." | ";"))
            })
            .any(|index| word_is(cursor, index, "anywhere"));
    cursor.enter("condition.history.object.amount");
    let (threshold, exclude_source) = recognize_history_amount(cursor)?;
    cursor.require(
        cursor.position() < verb_position,
        "a counted object phrase before the verb",
    )?;
    cursor.enter("condition.history.object.query");
    let spell_verb = matches!(
        verb,
        ObjectHistoryVerb::WasCast | ObjectHistoryVerb::WasCastWithMode(_)
    );
    let (predicate, player, domain) = if spell_verb {
        cursor.require(!exclude_source, "a counted spell without `another`")?;
        let query = cursor.bounded(verb_position, |cursor| {
            super::recognize_object_noun_query(cursor, super::ObjectQueryContext::ORDINARY)?
                .into_spell_cast_query(cursor)
        })?;
        cursor.require(
            query.caster.is_none(),
            "a spell query without a caster relation (the actor is a separate subject)",
        )?;
        cursor.require(
            query.predicate.has_domain(ObjectKind::Spell)
                && query.predicate.supports_domain(ObjectKind::Spell),
            "an explicit spell-domain query",
        )?;
        (query.predicate, HistoryPlayer::Any, ObjectKind::Spell)
    } else if from_anywhere {
        // Cards put into a graveyard from anywhere: the arrival record.
        cursor.require(!exclude_source, "counted cards without `another`")?;
        let predicate = cursor.bounded(verb_position, recognize_counted_cards)?;
        (predicate, HistoryPlayer::Any, ObjectKind::Card)
    } else {
        let query = cursor.bounded(verb_position, recognize_object_query)?;
        cursor.require(
            !query.exclude_source,
            "a counted object with `another`, not `other`",
        )?;
        cursor.require(
            !exclude_source || verb == ObjectHistoryVerb::EnteredBattlefield,
            "`another` object that entered the battlefield (the source is excluded from an entry record only)",
        )?;
        let player = match query.controller {
            None => HistoryPlayer::Any,
            Some(ObjectControllerScope::AbilityController) => HistoryPlayer::Controller,
            Some(ObjectControllerScope::OpponentOfAbilityController) => HistoryPlayer::Opponent,
            Some(_) => {
                return Err(cursor.fail("a controller relation of the counted permanents"));
            }
        };
        cursor.require(
            query.predicate.supports_domain(ObjectKind::Permanent),
            "a predicate valid for a battlefield permanent",
        )?;
        if matches!(
            verb,
            ObjectHistoryVerb::EnteredBattlefield | ObjectHistoryVerb::Attacked
        ) && predicate_reads_object_state(&query.predicate)
        {
            return Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::EventHistoryState,
                "a characteristic the entry record keeps (a face-down, tapped, designation or counter state is not recorded)",
            ));
        }
        if !matches!(
            verb,
            ObjectHistoryVerb::EnteredBattlefield | ObjectHistoryVerb::Attacked
        ) {
            cursor.require(
                type_line_predicate(&query.predicate),
                "a type-line characteristic of a permanent that left the battlefield (other last-known characteristics are not recorded)",
            )?;
        }
        (query.predicate, player, ObjectKind::Permanent)
    };
    cursor.enter("condition.history.object.verb");
    let mut qualifiers = HistoryQualifiers {
        exclude_source,
        ..HistoryQualifiers::default()
    };
    let mut measure = HistoryMeasure::Occurrences;
    let mut player = player;
    let event = match verb {
        ObjectHistoryVerb::Died => {
            consume_current(cursor)?;
            HistoryEvent::PermanentDied
        }
        ObjectHistoryVerb::LeftBattlefield | ObjectHistoryVerb::EnteredBattlefield => {
            consume_current(cursor)?;
            cursor.consume("the")?;
            cursor.consume("battlefield")?;
            if verb == ObjectHistoryVerb::LeftBattlefield {
                HistoryEvent::PermanentLeftBattlefield
            } else {
                HistoryEvent::PermanentEntered
            }
        }
        ObjectHistoryVerb::Attacked => {
            consume_current(cursor)?;
            HistoryEvent::AttackerDeclared
        }
        ObjectHistoryVerb::WasCast => {
            consume_current(cursor)?;
            consume_current(cursor)?;
            HistoryEvent::SpellCast
        }
        ObjectHistoryVerb::WasCastWithMode(mode) => {
            consume_current(cursor)?;
            consume_current(cursor)?;
            qualifiers.cast_mode = Some(mode);
            HistoryEvent::SpellCast
        }
        ObjectHistoryVerb::PutIntoGraveyard | ObjectHistoryVerb::PutIntoHand => {
            // CR 700.4: a permanent put into a graveyard from the
            // battlefield died; a card put there from anywhere arrived.
            consume_current(cursor)?;
            consume_current(cursor)?;
            cursor.consume("into")?;
            let zone = if verb == ObjectHistoryVerb::PutIntoHand {
                "hand"
            } else {
                "graveyard"
            };
            let owner = recognize_owned_zone(cursor, zone)?;
            cursor.enter("condition.history.object.origin");
            cursor.consume("from")?;
            if from_anywhere {
                cursor.consume("anywhere")?;
                // The arrival record's player is the graveyard's owner.
                player = owner.unwrap_or(HistoryPlayer::Any);
                measure = HistoryMeasure::Cards;
                HistoryEvent::CardsPutIntoGraveyard
            } else {
                cursor.consume_phrase(&["the", "battlefield"])?;
                cursor.require(
                    player == HistoryPlayer::Any || owner.is_none(),
                    "one player relation: the counted permanents' controller or the zone's owner",
                )?;
                if let Some(owner) = owner {
                    // CR 404.2: a card goes to its owner's zone.
                    player = owner;
                    qualifiers.zone_owner = true;
                }
                if verb == ObjectHistoryVerb::PutIntoHand {
                    qualifiers.destination = Some(HistoryDestination::Hand);
                    HistoryEvent::PermanentLeftBattlefield
                } else {
                    HistoryEvent::PermanentDied
                }
            }
        }
    };
    cursor.enter("condition.history.object.control");
    let player = if word_is(cursor, 0, "under") {
        cursor.require(
            player == HistoryPlayer::Any
                && matches!(
                    event,
                    HistoryEvent::PermanentLeftBattlefield | HistoryEvent::PermanentEntered
                ),
            "one control relation of a permanent that moved",
        )?;
        cursor.consume("under")?;
        let relation = if word_is(cursor, 0, "your") {
            cursor.consume("your")?;
            HistoryPlayer::Controller
        } else {
            cursor.consume("an")?;
            cursor.require(
                contraction_is(cursor, 0, "opponent", "s"),
                "`your control` or `an opponent's control`",
            )?;
            consume_current(cursor)?;
            HistoryPlayer::Opponent
        };
        cursor.consume("control")?;
        relation
    } else {
        player
    };
    // The control relation filters the counted objects; it quantifies no
    // player, so every bound counts the events in total.
    let scope = recognize_history_scope(cursor)?;
    Ok(history_node(
        EventHistoryPattern {
            event,
            player,
            object: object_filter(predicate, domain),
            measure,
            qualifiers,
        },
        scope,
        HistoryAggregate::Total,
        threshold,
    ))
}
