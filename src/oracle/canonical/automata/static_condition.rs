//! `as long as CONDITION` (CR 611.3a): the Boolean condition that gates a
//! static ability. The parent owns `as long as` and the clause boundary (a
//! comma before the body, or the sentence end after it); this child owns only
//! the Boolean program over typed state propositions. Every atom is a reusable
//! proposition that the engine evaluates continuously, not a card recipe.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    BooleanConditionNode, ClauseAction, ClauseAmount, ClauseAmountPolicy, ClauseFrame,
    ClauseModality, ClauseSubject, ClauseSubjectPolicy, ClauseTense, ConditionPlayerSubject,
    ControlledCountDirection, ControllerTurnPropositionNode, EventActorScope, LIFE_LEMMAS,
    LocalConditionNode, NumericAtom, ObjectKind, ObjectPredicate, PlayerStateConditionNode,
    TargetPlayerLifeAction, clause_starts, collective_opponents_condition_starts,
    quantified_player_condition_starts, recognize_boolean_condition_list_before_separator,
    recognize_clause, recognize_collective_opponents_condition,
    recognize_controller_turn_proposition, recognize_numeric_atom,
    recognize_object_query_before_condition_connector, recognize_player_possession_proposition,
    recognize_quantified_player_condition, recognize_search_card_selector,
    recognize_source_reference, recognize_trigger_guard_atom, source_reference_starts_at,
};
use super::{ClauseArguments, VerbForm, VerbLemma, Voice, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// A state of the ability's own source that a static condition can test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceStateProposition {
    Tapped,
    Untapped,
    /// CR 506.4: an attacking or blocking creature until removed from combat.
    Attacking,
    Blocking,
    Modified,
    /// `this creature is monstrous`: a designation of the source.
    Designated(super::PermanentDesignation),
}

/// The quantified player relation of a controlled-permanent count comparison.
/// Each variant is one comparison between the controller's count and the
/// counts of its opponents; forms that need the least opponent count (`an
/// opponent controls fewer ...`) have no evaluator and stay explicit failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum OpponentCountRelation {
    /// There is an opponent whose count exceeds the controller's count.
    SomeOpponentExceedsController,
    /// The controller's count exceeds the count of every opponent.
    ControllerExceedsEveryOpponent,
}

/// What a `last turn` spell-cast condition asserts about the previous turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LastTurnSpellCasts {
    /// No player cast a spell.
    None,
    /// One player cast at least this many spells.
    SomePlayerAtLeast(i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StaticConditionAtom {
    /// Controller permanent presence / fixed-count atoms shared with guards.
    Local(LocalConditionNode),
    SourceState(SourceStateProposition),
    /// `it has a +1/+1 counter on it`: the source's counters, read by the
    /// shared counter qualifier of object queries (CR 122.1).
    SourceCounters(super::CounterPresence),
    /// `this creature is equipped`: a status of the source from the shared
    /// status registry that has no source-state proposition of its own.
    SourceStatus(super::ObjectPredicate),
    /// `it's your turn` / `it's not your turn`: the shared turn proposition.
    ControllerTurn(ControllerTurnPropositionNode),
    /// A player-state proposition about the controller (`you have ...`,
    /// `you've cast ...`) or a quantified player population (`an opponent
    /// controls ...`, `each player has ...`).
    PlayerState(PlayerStateConditionNode),
    /// `you gained life this turn` (threshold 1) / `gained N or more life`.
    ControllerGainedLifeThisTurn {
        threshold: i64,
    },
    /// `you attacked this turn`: the controller declared an attacker.
    ControllerAttackedThisTurn,
    ControllerLostLifeThisTurn,
    /// `an opponent lost life this turn` (any opponent of the controller).
    OpponentLostLifeThisTurn,
    /// `an opponent was dealt damage this turn` (CR 120.3: damage dealt to a
    /// player, any source; any opponent of the controller).
    OpponentDealtDamageThisTurn,
    /// `an opponent controls more lands than you`, `you control fewer creatures
    /// than an opponent`, `you control more QUERY than each opponent`: the
    /// shared permanent query counted for the controller and for opponents.
    OpponentControlledCountComparison {
        predicate: ObjectPredicate,
        relation: OpponentCountRelation,
    },
    /// `a creature died this turn`, `three or more Elves died this turn`:
    /// permanents of every player put into a graveyard from the battlefield.
    /// `subject` is a card type or subtype.
    PermanentsDiedThisTurn {
        threshold: i64,
        subject: ObjectPredicate,
    },
    /// `no spells were cast last turn` / `a player cast N or more spells last
    /// turn`: spell casts during the turn before the current one.
    SpellsCastLastTurn(LastTurnSpellCasts),
    CardsInControllerGraveyard {
        threshold: i64,
        direction: ControlledCountDirection,
        predicate: ObjectPredicate,
    },
    ControllerHasEnduringStory,
    /// `you've completed a dungeon` / `you haven't completed NAME` (CR
    /// 309.7): `dungeon` names one Dungeon, or any Dungeon when absent.
    ControllerCompletedDungeon {
        dungeon: Option<String>,
        completed: bool,
    },
}

pub(in crate::oracle::canonical) type StaticConditionNode =
    BooleanConditionNode<StaticConditionAtom>;

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
    recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_static_condition_atom(cursor, context)
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

pub(in crate::oracle::canonical) fn recognize_static_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: StaticConditionContext<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.atom-kind");
    if let Some(atom) = try_recognize_life_history_atom(cursor)? {
        return Ok(atom);
    }
    if opponent_count_comparison_starts(cursor) {
        return recognize_opponent_count_comparison_atom(cursor);
    }
    if word_is(cursor, 0, "you") && word_is(cursor, 1, "control") {
        return recognize_trigger_guard_atom(cursor).map(StaticConditionAtom::Local);
    }
    if collective_opponents_condition_starts(cursor) {
        return recognize_collective_opponents_condition(cursor)
            .map(StaticConditionAtom::PlayerState);
    }
    if quantified_player_condition_starts(cursor) {
        return recognize_quantified_player_condition(cursor).map(StaticConditionAtom::PlayerState);
    }
    if contraction_is(cursor, 0, "you", "ve")
        || (word_is(cursor, 0, "you")
            && (word_is(cursor, 1, "have")
                || contraction_is(cursor, 1, "haven", "t")
                || word_is(cursor, 1, "attacked")))
    {
        return recognize_controller_history_atom(cursor);
    }
    if word_is(cursor, 0, "an")
        && word_is(cursor, 1, "opponent")
        && word_is(cursor, 2, "was")
        && cursor
            .peek_ahead(3)
            .is_some_and(|raw| is_verb_form(raw, VerbLemma::Deal, VerbForm::PastParticiple))
    {
        return recognize_opponent_damage_history_atom(cursor);
    }
    if word_is(cursor, 0, "no") || (word_is(cursor, 0, "a") && word_is(cursor, 1, "player")) {
        return recognize_last_turn_spell_atom(cursor);
    }
    if word_is(cursor, 0, "a")
        || word_is(cursor, 0, "an")
        || cursor
            .peek()
            .is_some_and(|raw| raw.parse::<i64>().is_ok() || parse_number_word(raw).is_some())
    {
        return recognize_permanents_died_atom(cursor);
    }
    if word_is(cursor, 0, "there") || contraction_is(cursor, 0, "there", "s") {
        return recognize_graveyard_count_atom(cursor);
    }
    if contraction_is(cursor, 0, "it", "s")
        && (word_is(cursor, 1, "your") || (word_is(cursor, 1, "not") && word_is(cursor, 2, "your")))
    {
        return recognize_controller_turn_proposition(cursor)
            .map(StaticConditionAtom::ControllerTurn);
    }
    let source_subject = contraction_is(cursor, 0, "it", "s")
        || (word_is(cursor, 0, "it") && (word_is(cursor, 1, "is") || word_is(cursor, 1, "has")))
        || source_reference_starts_at(
            cursor.position(),
            cursor.peek(),
            context.name_links,
            context.name_failure,
        );
    if source_subject {
        return recognize_source_state_atom(cursor, context);
    }
    Err(cursor.fail(
        "a supported static condition: controller permanent presence, source state, this-turn history, graveyard count, or enduring story",
    ))
}

/// `it's [un]tapped`, `it's attacking`, `it's modified`, `this CARD is
/// blocking`: a state of the
/// ability's own source. `it` is the source only when the subject is.
fn recognize_source_state_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: StaticConditionContext<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.source-state.subject");
    if contraction_is(cursor, 0, "it", "s") || word_is(cursor, 0, "it") {
        cursor.require(
            context.source_subject,
            "`it` that denotes the static ability's own source",
        )?;
        if word_is(cursor, 0, "it") && word_is(cursor, 1, "has") {
            cursor.consume("it")?;
        } else if word_is(cursor, 0, "it") {
            cursor.consume("it")?;
            cursor.consume("is")?;
        } else {
            let raw = cursor.peek().expect("contraction was checked").to_string();
            cursor.consume(&raw)?;
        }
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        cursor.enter("condition.static.source-state.copula");
        if !word_is(cursor, 0, "has") {
            cursor.consume("is")?;
        }
    }
    if word_is(cursor, 0, "has") {
        cursor.enter("condition.static.source-state.counters");
        cursor.consume("has")?;
        cursor.require(
            super::counter_qualifier_starts(cursor, 0),
            "a counter possession of the source",
        )?;
        return match super::recognize_counter_qualifier(cursor)? {
            super::ObjectPredicate::HasCounter(presence) => {
                Ok(StaticConditionAtom::SourceCounters(presence))
            }
            _ => Err(cursor.fail("a counter possession of the source")),
        };
    }
    cursor.enter("condition.static.source-state.property");
    let property = cursor.peek().map(str::to_ascii_lowercase).ok_or_else(|| {
        cursor.fail("`tapped`, `untapped`, `attacking`, `blocking` or `modified`")
    })?;
    // `enchanted or equipped`: a coordinated list of registered statuses is
    // one predicate of the source (its union or intersection).
    let coordinated = |cursor: &OracleTokenCursor<'_>| {
        (word_is(cursor, 1, "or") || word_is(cursor, 1, "and"))
            && cursor
                .peek_ahead(2)
                .is_some_and(|raw| super::lookup_status_predicate(&raw).is_some())
    };
    if super::lookup_status_predicate(&property).is_some() && coordinated(cursor) {
        let mut members = Vec::new();
        let mut connector = None;
        loop {
            let raw = cursor
                .peek()
                .map(str::to_ascii_lowercase)
                .ok_or_else(|| cursor.fail("a registered status"))?;
            let predicate = cursor
                .require_value(super::lookup_status_predicate(&raw), "a registered status")?;
            cursor.consume(&raw)?;
            members.push(predicate);
            if !coordinated_next(cursor) {
                break;
            }
            let word = cursor
                .peek()
                .expect("the connector was checked")
                .to_ascii_lowercase();
            cursor.require(
                connector.is_none_or(|previous: &str| previous == word),
                "one connector in a status list",
            )?;
            connector = Some(if word == "or" { "or" } else { "and" });
            cursor.consume(&word)?;
        }
        return Ok(StaticConditionAtom::SourceStatus(
            if connector == Some("and") {
                super::ObjectPredicate::Intersection(members)
            } else {
                super::ObjectPredicate::Union(members)
            },
        ));
    }
    let designation = super::PermanentDesignation::from_adjective(&property);
    if designation.is_none()
        && let Some(predicate) = super::lookup_status_predicate(&property)
        && !matches!(
            property.as_str(),
            "tapped" | "untapped" | "attacking" | "blocking" | "modified"
        )
    {
        cursor.consume(&property)?;
        return Ok(StaticConditionAtom::SourceStatus(predicate));
    }
    let proposition = match property.as_str() {
        _ if designation.is_some() => {
            SourceStateProposition::Designated(designation.expect("the designation was read"))
        }
        "tapped" => SourceStateProposition::Tapped,
        "untapped" => SourceStateProposition::Untapped,
        "attacking" => SourceStateProposition::Attacking,
        "blocking" => SourceStateProposition::Blocking,
        "modified" => SourceStateProposition::Modified,
        _ => {
            return Err(cursor.fail("`tapped`, `untapped`, `attacking`, `blocking` or `modified`"));
        }
    };
    cursor.consume(&property)?;
    Ok(StaticConditionAtom::SourceState(proposition))
}

/// The cursor is at `or|and STATUS` inside a status list.
fn coordinated_next(cursor: &OracleTokenCursor<'_>) -> bool {
    (word_is(cursor, 0, "or") || word_is(cursor, 0, "and"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| super::lookup_status_predicate(&raw).is_some())
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
fn recognize_graveyard_count_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.graveyard-count.existential");
    if contraction_is(cursor, 0, "there", "s") {
        let raw = cursor.peek().expect("contraction was checked").to_string();
        cursor.consume(&raw)?;
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
    Ok(StaticConditionAtom::CardsInControllerGraveyard {
        threshold,
        direction,
        predicate,
    })
}

/// `you've drawn N or more cards this turn`, `you've cast N or more spells
/// this turn`, `you've gained life this turn`, `you have an enduring story`.
fn recognize_controller_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.controller-history.subject");
    if contraction_is(cursor, 0, "you", "ve") {
        let raw = cursor.peek().expect("contraction was checked").to_string();
        cursor.consume(&raw)?;
    } else if word_is(cursor, 0, "you") && contraction_is(cursor, 1, "haven", "t") {
        cursor.consume("you")?;
        let raw = cursor.peek().expect("contraction was checked").to_string();
        cursor.consume(&raw)?;
        cursor.enter("condition.static.controller-history.negated-verb");
        return recognize_completed_dungeon_atom(cursor, false);
    } else {
        cursor.consume("you")?;
        // `you attacked this turn` is the simple-past form of the same history.
        if !word_is(cursor, 0, "attacked") {
            cursor.consume("have")?;
        }
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
            StaticConditionAtom::PlayerState(PlayerStateConditionNode {
                subject,
                proposition,
            })
        });
    }
    let verb = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`drawn`, `cast`, `attacked` or an enduring story"))?;
    match verb.as_str() {
        "attacked" => {
            cursor.consume("attacked")?;
            cursor.enter("condition.static.controller-history.attack");
            cursor.consume_phrase(&["this", "turn"])?;
            Ok(StaticConditionAtom::ControllerAttackedThisTurn)
        }
        "an" => {
            cursor.consume_phrase(&["an", "enduring", "story"])?;
            Ok(StaticConditionAtom::ControllerHasEnduringStory)
        }
        "completed" => recognize_completed_dungeon_atom(cursor, true),
        _ => Err(cursor.fail("`drawn`, `cast`, `attacked`, a life total or an enduring story")),
    }
}

/// `completed a dungeon` or `completed NAME` (CR 309.7). A named Dungeon is
/// read up to the condition's end; it is compared with completed Dungeons.
fn recognize_completed_dungeon_atom(
    cursor: &mut OracleTokenCursor<'_>,
    completed: bool,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.completed-dungeon");
    cursor.consume("completed")?;
    if word_is(cursor, 0, "a") && word_is(cursor, 1, "dungeon") {
        cursor.consume_phrase(&["a", "dungeon"])?;
        return Ok(StaticConditionAtom::ControllerCompletedDungeon {
            dungeon: None,
            completed,
        });
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
    Ok(StaticConditionAtom::ControllerCompletedDungeon {
        dungeon: Some(words.join(" ")),
        completed,
    })
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

/// `an opponent controls more QUERY than you` (an opponent's count exceeds
/// the controller's), `you control fewer QUERY than an opponent` (the same
/// relation), and `you control more QUERY than each opponent`. The permanent
/// query is the shared object query; only the quantified relation is local.
fn recognize_opponent_count_comparison_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.count-comparison.subject");
    let opponent_subject = word_is(cursor, 0, "an");
    if opponent_subject {
        cursor.consume_phrase(&["an", "opponent", "controls"])?;
    } else {
        cursor.consume_phrase(&["you", "control"])?;
    }
    cursor.enter("condition.static.count-comparison.direction");
    let more = word_is(cursor, 0, "more");
    let direction = if more { "more" } else { "fewer" };
    cursor.consume(direction)?;
    cursor.enter("condition.static.count-comparison.query");
    let query = recognize_object_query_before_condition_connector(cursor)?;
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
    let relation = if opponent_subject {
        cursor.consume("you")?;
        cursor.require(
            more,
            "`more ... than you`; an opponent with fewer permanents has no evaluator",
        )?;
        OpponentCountRelation::SomeOpponentExceedsController
    } else if word_is(cursor, 0, "an") {
        cursor.consume_phrase(&["an", "opponent"])?;
        cursor.require(
            !more,
            "`fewer ... than an opponent`; more than some opponent has no evaluator",
        )?;
        OpponentCountRelation::SomeOpponentExceedsController
    } else {
        cursor.consume_phrase(&["each", "opponent"])?;
        cursor.require(
            more,
            "`more ... than each opponent`; fewer than every opponent has no evaluator",
        )?;
        OpponentCountRelation::ControllerExceedsEveryOpponent
    };
    Ok(StaticConditionAtom::OpponentControlledCountComparison {
        predicate: query.predicate,
        relation,
    })
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

/// True when a past life clause (`you gained life this turn`) starts here.
pub(in crate::oracle::canonical) fn life_history_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    clause_starts(cursor, &life_history_frame())
}

/// `PLAYER gained|lost [N or more] life this turn`: the past interpretation
/// of the shared life clause, as a this-turn history query. Only histories
/// the engine records are atoms; any other claimed clause fails here.
fn try_recognize_life_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<StaticConditionAtom>> {
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
    Ok(Some(match (actor, direction, amount) {
        (EventActorScope::Controller, TargetPlayerLifeAction::Gain, ClauseAmount::Unspecified) => {
            StaticConditionAtom::ControllerGainedLifeThisTurn { threshold: 1 }
        }
        (
            EventActorScope::Controller,
            TargetPlayerLifeAction::Gain,
            ClauseAmount::Count { value, direction },
        ) => {
            cursor.require(
                direction == ControlledCountDirection::OrMore && value >= 1,
                "an `or more` life-gain count of at least one",
            )?;
            StaticConditionAtom::ControllerGainedLifeThisTurn { threshold: value }
        }
        (EventActorScope::Controller, TargetPlayerLifeAction::Lose, ClauseAmount::Unspecified) => {
            StaticConditionAtom::ControllerLostLifeThisTurn
        }
        (EventActorScope::Opponent, TargetPlayerLifeAction::Lose, ClauseAmount::Unspecified) => {
            StaticConditionAtom::OpponentLostLifeThisTurn
        }
        _ => {
            return Err(cursor.fail(
                "a recorded life history (`you gained [N or more] life`, `you lost life`, or `an opponent lost life` this turn)",
            ));
        }
    }))
}

/// `an opponent was dealt damage this turn`: the passive of the `deal` lemma
/// (CR 120.1) with an opponent as recipient, as a this-turn history.
fn recognize_opponent_damage_history_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.damage-history.recipient");
    cursor.consume_phrase(&["an", "opponent"])?;
    cursor.enter("condition.static.damage-history.verb");
    cursor.consume("was")?;
    let participle = cursor
        .peek()
        .filter(|raw| is_verb_form(raw, VerbLemma::Deal, VerbForm::PastParticiple))
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("the passive participle of `deal`"))?;
    cursor.consume(&participle)?;
    cursor.enter("condition.static.damage-history.object");
    cursor.consume("damage")?;
    cursor.enter("condition.static.damage-history.time");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(StaticConditionAtom::OpponentDealtDamageThisTurn)
}

/// `a creature died this turn`, `two or more Elves died this turn`: the
/// count of this-turn deaths of permanents described by one card type or
/// subtype, owned by any player.
fn recognize_permanents_died_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    let (threshold, direction, _) = recognize_count_comparison(cursor)?;
    cursor.require(
        direction == ControlledCountDirection::OrMore,
        "an `or more` this-turn death count",
    )?;
    cursor.enter("condition.static.died.subject");
    let query = recognize_object_query_before_condition_connector(cursor)?;
    cursor.require(
        query.controller.is_none(),
        "a death count without a controller relation",
    )?;
    cursor.require(
        matches!(
            query.predicate,
            ObjectPredicate::CardType(_) | ObjectPredicate::Subtype(_)
        ),
        "one card type or subtype of the permanents that died",
    )?;
    cursor.enter("condition.static.died.verb");
    cursor.consume_phrase(&["died", "this", "turn"])?;
    Ok(StaticConditionAtom::PermanentsDiedThisTurn {
        threshold,
        subject: query.predicate,
    })
}

/// `no spells were cast last turn`, `a player cast two or more spells last turn`.
fn recognize_last_turn_spell_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.last-turn.subject");
    if word_is(cursor, 0, "no") {
        cursor.consume_phrase(&["no", "spells", "were", "cast", "last", "turn"])?;
        return Ok(StaticConditionAtom::SpellsCastLastTurn(
            LastTurnSpellCasts::None,
        ));
    }
    cursor.consume_phrase(&["a", "player", "cast"])?;
    let (threshold, direction, singular) = recognize_count_comparison(cursor)?;
    cursor.require(
        direction == ControlledCountDirection::OrMore,
        "an `or more` last-turn spell count",
    )?;
    cursor.enter("condition.static.last-turn.noun");
    cursor.consume(if singular { "spell" } else { "spells" })?;
    cursor.consume_phrase(&["last", "turn"])?;
    Ok(StaticConditionAtom::SpellsCastLastTurn(
        LastTurnSpellCasts::SomePlayerAtLeast(threshold),
    ))
}
