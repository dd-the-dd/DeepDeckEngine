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
    EventHistoryPattern, HistoryAggregate, HistoryEvent, HistoryMeasure, HistoryPlayer,
    HistoryScope, LIFE_LEMMAS, NumericAtom, NumericComparisonOperator, ObjectControllerScope,
    ObjectKind, ObjectPredicate, PlayerCardZone, PlayerDesignation, PlayerDesignationNode,
    PlayerStateConditionNode, PlayerStateMeasure, TargetPlayerLifeAction, ThresholdOperator,
    recognize_boolean_condition_list_before_separator, recognize_clause, recognize_condition_atom,
    recognize_count_threshold, recognize_numeric_atom, recognize_object_query,
    recognize_player_possession_proposition, recognize_search_card_selector, threshold_comparison,
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

/// Reads the Boolean program after `as long as`. The clause terminator (`,` or
/// the sentence end) stays with the parent.
pub(in crate::oracle::canonical) fn recognize_static_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: StaticConditionContext<'_>,
) -> MachineResult<StaticConditionNode> {
    cursor.enter("condition.static.boolean-program");
    let context = ConditionContext::standalone(
        ConditionScope::Static,
        context.name_links,
        context.name_failure,
        context.source_subject,
    );
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
    let (threshold, direction, singular) = recognize_count_comparison(cursor)?;
    cursor.enter("condition.static.graveyard-count.cards");
    let predicate = recognize_search_card_selector(cursor, !singular)?;
    cursor.enter("condition.static.graveyard-count.zone");
    cursor.consume("in")?;
    cursor.consume("your")?;
    cursor.consume("graveyard")?;
    Ok(ConditionAtom::Compare(ConditionComparison::measured(
        ConditionPlayerSubject::Controller,
        PlayerStateMeasure::CardsInZone {
            zone: PlayerCardZone::Graveyard,
            predicate,
        },
        CountThreshold {
            operator: match direction {
                ControlledCountDirection::OrMore => ThresholdOperator::AtLeast,
                ControlledCountDirection::OrFewer => ThresholdOperator::AtMost,
            },
            value: threshold,
        },
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
    if cursor.peek().is_some_and(|raw| {
        lookup_verb(raw, &[VerbLemma::Search, VerbLemma::Attack])
            .is_some_and(|(_, forms)| forms.contains(&VerbForm::PastParticiple))
    }) {
        return recognize_history_predicate(cursor, HistoryPlayer::Controller);
    }
    cursor.enter("condition.static.controller-history.designation");
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

/// The past verbs a player-subject history reads.
const ACTOR_HISTORY_LEMMAS: &[VerbLemma] = &[VerbLemma::Cast, VerbLemma::Search, VerbLemma::Attack];

/// True at `PLAYER VERBED ...` (simple past of a recorded action) or
/// `PLAYER was|were dealt damage`.
pub(in crate::oracle::canonical) fn actor_event_history_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let Some((length, _)) = history_actor_at(cursor) else {
        return false;
    };
    let past = cursor.peek_ahead(length).is_some_and(|raw| {
        lookup_verb(raw, ACTOR_HISTORY_LEMMAS)
            .is_some_and(|(_, forms)| forms.contains(&VerbForm::Past))
    });
    let dealt = (word_is(cursor, length, "was") || word_is(cursor, length, "were"))
        && cursor
            .peek_ahead(length + 1)
            .is_some_and(|raw| is_verb_form(raw, VerbLemma::Deal, VerbForm::PastParticiple));
    past || dealt
}

/// `this turn` or `last turn`.
fn recognize_history_scope(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<HistoryScope> {
    cursor.enter("condition.history.scope");
    let scope = if word_is(cursor, 0, "this") {
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
    cursor: &OracleTokenCursor<'_>,
    player: HistoryPlayer,
    threshold: CountThreshold,
) -> MachineResult<HistoryAggregate> {
    if player == HistoryPlayer::Controller {
        return Ok(HistoryAggregate::Total);
    }
    match threshold.operator {
        ThresholdOperator::AtLeast if threshold.value <= 1 => Ok(HistoryAggregate::Total),
        ThresholdOperator::AtLeast => Ok(HistoryAggregate::GreatestPerPlayer),
        ThresholdOperator::AtMost => Err(cursor.fail(
            "a lower bound for a player population's history (an upper bound needs the least per-player count)",
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

/// The predicate, unless it only restates the event object's domain.
fn object_filter(predicate: ObjectPredicate, domain: ObjectKind) -> Option<ObjectPredicate> {
    (predicate != ObjectPredicate::ObjectKind(domain)).then_some(predicate)
}

/// `you attacked [with N CREATURES] this turn`, `an opponent searched their
/// library this turn`, `a player cast two or more spells last turn`, `an
/// opponent was dealt damage this turn`: the player relation, a recorded
/// action, an optional counted object and the turn.
pub(in crate::oracle::canonical) fn recognize_actor_event_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.history.actor");
    let (length, player) =
        history_actor_at(cursor).ok_or_else(|| cursor.fail("a player subject"))?;
    for _ in 0..length {
        consume_current(cursor)?;
    }
    recognize_history_predicate(cursor, player)
}

/// The verb phrase of a player-subject history after its subject (and after
/// a perfect `have`, whose participle has the same surface).
fn recognize_history_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    player: HistoryPlayer,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.history.verb");
    if word_is(cursor, 0, "was") || word_is(cursor, 0, "were") {
        consume_current(cursor)?;
        consume_current(cursor)?;
        cursor.enter("condition.history.damage");
        cursor.consume("damage")?;
        let scope = recognize_history_scope(cursor)?;
        let threshold = CountThreshold::at_least(1);
        return Ok(history_node(
            EventHistoryPattern {
                event: HistoryEvent::DamageDealtToPlayer,
                player,
                object: None,
                measure: HistoryMeasure::Amount,
            },
            scope,
            HistoryAggregate::Total,
            threshold,
        ));
    }
    let raw = cursor.peek().ok_or_else(|| cursor.fail("a past verb"))?;
    let (entry, _) = lookup_verb(raw, ACTOR_HISTORY_LEMMAS)
        .ok_or_else(|| cursor.fail("a recorded action in the simple past"))?;
    let lemma = entry.lemma;
    consume_current(cursor)?;
    match lemma {
        VerbLemma::Cast => {
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
            let aggregate = history_aggregate(cursor, player, threshold)?;
            let scope = recognize_history_scope(cursor)?;
            Ok(history_node(
                EventHistoryPattern {
                    event: HistoryEvent::SpellCast,
                    player,
                    object: object_filter(query.predicate, ObjectKind::Spell),
                    measure: HistoryMeasure::Occurrences,
                },
                scope,
                aggregate,
                threshold,
            ))
        }
        VerbLemma::Search => {
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
            let scope = recognize_history_scope(cursor)?;
            Ok(history_node(
                EventHistoryPattern {
                    event: HistoryEvent::LibrarySearched,
                    player,
                    object: None,
                    measure: HistoryMeasure::Occurrences,
                },
                scope,
                HistoryAggregate::Total,
                CountThreshold::at_least(1),
            ))
        }
        VerbLemma::Attack => {
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
            let aggregate = history_aggregate(cursor, player, threshold)?;
            let scope = recognize_history_scope(cursor)?;
            Ok(history_node(
                EventHistoryPattern {
                    event: HistoryEvent::AttackerDeclared,
                    player,
                    object,
                    measure: HistoryMeasure::Occurrences,
                },
                scope,
                aggregate,
                threshold,
            ))
        }
        _ => Err(cursor.fail("a recorded action in the simple past")),
    }
}

/// The verb of an object-quantity history and its position after the
/// counted object phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectHistoryVerb {
    Died,
    LeftBattlefield,
    EnteredBattlefield,
    WasCast,
}

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
        if (raw.eq_ignore_ascii_case("was") || raw.eq_ignore_ascii_case("were"))
            && cursor
                .peek_ahead(offset + 1)
                .is_some_and(|next| is_verb_form(next, VerbLemma::Cast, VerbForm::PastParticiple))
        {
            return Some((offset, ObjectHistoryVerb::WasCast));
        }
        None
    })
}

/// True at `COUNT OBJECT died|left the battlefield|entered the battlefield|
/// was cast`.
pub(in crate::oracle::canonical) fn object_event_history_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let count_word = word_is(cursor, 0, "no")
        || word_is(cursor, 0, "a")
        || word_is(cursor, 0, "an")
        || word_is(cursor, 0, "at")
        || cursor
            .peek()
            .is_some_and(|raw| raw.parse::<i64>().is_ok() || parse_number_word(raw).is_some());
    count_word && object_history_verb_at(cursor).is_some()
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

/// `a creature died this turn`, `two or more Elves died this turn`, `a
/// permanent left the battlefield under your control this turn`, `no spells
/// were cast last turn`: a counted object phrase of the shared object query,
/// a recorded zone change or cast, an optional control relation and the turn.
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
    cursor.enter("condition.history.object.amount");
    let threshold = recognize_count_threshold(cursor, true)?;
    cursor.require(
        cursor.position() < verb_position,
        "a counted object phrase before the verb",
    )?;
    cursor.enter("condition.history.object.query");
    let (predicate, player, domain) = if verb == ObjectHistoryVerb::WasCast {
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
    } else {
        let query = cursor.bounded(verb_position, recognize_object_query)?;
        cursor.require(
            !query.exclude_source,
            "a counted object without `other` (the source is not excluded from a history)",
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
        if verb != ObjectHistoryVerb::EnteredBattlefield {
            cursor.require(
                type_line_predicate(&query.predicate),
                "a type-line characteristic of a permanent that left the battlefield (other last-known characteristics are not recorded)",
            )?;
        }
        (query.predicate, player, ObjectKind::Permanent)
    };
    cursor.enter("condition.history.object.verb");
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
        ObjectHistoryVerb::WasCast => {
            consume_current(cursor)?;
            consume_current(cursor)?;
            HistoryEvent::SpellCast
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
            measure: HistoryMeasure::Occurrences,
        },
        scope,
        HistoryAggregate::Total,
        threshold,
    ))
}
