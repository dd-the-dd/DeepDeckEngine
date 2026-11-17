//! `as long as CONDITION` (CR 611.3a): the Boolean condition that gates a
//! static ability. The parent owns `as long as` and the clause boundary (a
//! comma before the body, or the sentence end after it); this child owns only
//! the Boolean program over typed state propositions. Every atom is a reusable
//! proposition that the engine evaluates continuously, not a card recipe.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    BooleanConditionNode, ControlledCountDirection, LocalConditionNode, NumericAtom, ObjectKind,
    ObjectPredicate, recognize_boolean_condition_list_before_separator, recognize_numeric_atom,
    recognize_object_query_before_condition_connector, recognize_search_card_selector,
    recognize_source_reference, recognize_trigger_guard_atom, source_reference_starts_at,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// A state of the ability's own source that a static condition can test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceStateProposition {
    Tapped,
    Untapped,
    Modified,
}

/// Counted events of the ability's controller during the current turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ControllerTurnEvent {
    CardDrawn,
    SpellCast,
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
    ControllerEventCount {
        event: ControllerTurnEvent,
        threshold: i64,
    },
    /// `you gained life this turn` (threshold 1) / `gained N or more life`.
    ControllerGainedLifeThisTurn {
        threshold: i64,
    },
    /// `you attacked this turn`: the controller declared an attacker.
    ControllerAttackedThisTurn,
    ControllerLostLifeThisTurn,
    /// `an opponent lost life this turn` (any opponent of the controller).
    OpponentLostLifeThisTurn,
    /// `an opponent controls more lands than you`, `you control fewer creatures
    /// than an opponent`, `you control more QUERY than each opponent`: the
    /// shared permanent query counted for the controller and for opponents.
    OpponentControlledCountComparison {
        predicate: ObjectPredicate,
        relation: OpponentCountRelation,
    },
    /// `you have N or more life` / `N or less life`: the controller's life total.
    ControllerLifeTotal {
        threshold: i64,
        direction: ControlledCountDirection,
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
    if opponent_count_comparison_starts(cursor) {
        return recognize_opponent_count_comparison_atom(cursor);
    }
    if word_is(cursor, 0, "you") && word_is(cursor, 1, "control") {
        return recognize_trigger_guard_atom(cursor).map(StaticConditionAtom::Local);
    }
    if contraction_is(cursor, 0, "you", "ve")
        || (word_is(cursor, 0, "you")
            && (word_is(cursor, 1, "have")
                || contraction_is(cursor, 1, "haven", "t")
                || word_is(cursor, 1, "gained")
                || word_is(cursor, 1, "attacked")
                || word_is(cursor, 1, "lost")))
    {
        return recognize_controller_history_atom(cursor);
    }
    if word_is(cursor, 0, "no") || (word_is(cursor, 0, "a") && word_is(cursor, 1, "player")) {
        return recognize_last_turn_spell_atom(cursor);
    }
    if word_is(cursor, 0, "an") && word_is(cursor, 1, "opponent") {
        return recognize_opponent_life_loss_atom(cursor);
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
    let source_subject = contraction_is(cursor, 0, "it", "s")
        || (word_is(cursor, 0, "it") && word_is(cursor, 1, "is"))
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

/// `it's [un]tapped`, `it's modified`, `this CARD is tapped`: a state of the
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
        if word_is(cursor, 0, "it") {
            cursor.consume("it")?;
            cursor.consume("is")?;
        } else {
            let raw = cursor.peek().expect("contraction was checked").to_string();
            cursor.consume(&raw)?;
        }
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        cursor.enter("condition.static.source-state.copula");
        cursor.consume("is")?;
    }
    cursor.enter("condition.static.source-state.property");
    let property = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`tapped`, `untapped` or `modified`"))?;
    let proposition = match property.as_str() {
        "tapped" => SourceStateProposition::Tapped,
        "untapped" => SourceStateProposition::Untapped,
        "modified" => SourceStateProposition::Modified,
        _ => return Err(cursor.fail("`tapped`, `untapped` or `modified`")),
    };
    cursor.consume(&property)?;
    Ok(StaticConditionAtom::SourceState(proposition))
}

/// `N or more` / `N or fewer` / `a` / `an`: a fixed bounded count comparison.
fn recognize_count_comparison(
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
        // `you gained life this turn` is the simple-past form of the same history.
        if !(word_is(cursor, 0, "gained")
            || word_is(cursor, 0, "lost")
            || word_is(cursor, 0, "attacked"))
        {
            cursor.consume("have")?;
        }
    }
    cursor.enter("condition.static.controller-history.verb");
    if cursor.peek().is_some_and(|raw| {
        !raw.eq_ignore_ascii_case("an")
            && (raw.parse::<i64>().is_ok() || parse_number_word(raw).is_some())
    }) {
        return recognize_life_total_atom(cursor);
    }
    let verb = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`drawn`, `cast`, `gained` or an enduring story"))?;
    match verb.as_str() {
        "drawn" | "cast" => {
            cursor.consume(&verb)?;
            let (threshold, direction, _) = recognize_count_comparison(cursor)?;
            cursor.require(
                direction == ControlledCountDirection::OrMore,
                "an `or more` this-turn event count",
            )?;
            cursor.enter("condition.static.controller-history.noun");
            let (noun, event) = if verb == "drawn" {
                ("cards", ControllerTurnEvent::CardDrawn)
            } else {
                ("spells", ControllerTurnEvent::SpellCast)
            };
            cursor.consume(noun)?;
            cursor.enter("condition.static.controller-history.duration");
            cursor.consume_phrase(&["this", "turn"])?;
            Ok(StaticConditionAtom::ControllerEventCount { event, threshold })
        }
        "gained" => {
            cursor.consume("gained")?;
            cursor.enter("condition.static.controller-history.life");
            let threshold = if word_is(cursor, 0, "life") {
                1
            } else {
                let (threshold, direction, _) = recognize_count_comparison(cursor)?;
                cursor.require(
                    direction == ControlledCountDirection::OrMore && threshold >= 1,
                    "an `or more` life-gain count of at least one",
                )?;
                threshold
            };
            cursor.consume("life")?;
            cursor.consume_phrase(&["this", "turn"])?;
            Ok(StaticConditionAtom::ControllerGainedLifeThisTurn { threshold })
        }
        "attacked" => {
            cursor.consume("attacked")?;
            cursor.enter("condition.static.controller-history.attack");
            cursor.consume_phrase(&["this", "turn"])?;
            Ok(StaticConditionAtom::ControllerAttackedThisTurn)
        }
        "lost" => {
            cursor.consume("lost")?;
            cursor.enter("condition.static.controller-history.life");
            cursor.consume("life")?;
            cursor.consume_phrase(&["this", "turn"])?;
            Ok(StaticConditionAtom::ControllerLostLifeThisTurn)
        }
        "an" => {
            cursor.consume_phrase(&["an", "enduring", "story"])?;
            Ok(StaticConditionAtom::ControllerHasEnduringStory)
        }
        "completed" => recognize_completed_dungeon_atom(cursor, true),
        _ => {
            Err(cursor.fail("`drawn`, `cast`, `gained`, `lost`, a life total or an enduring story"))
        }
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

/// `N or more life` / `N or less life` after `you have`: the controller's life
/// total compared with a fixed bound.
fn recognize_life_total_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.life-total.threshold");
    let threshold = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) => value,
        NumericAtom::VariableX => return Err(cursor.fail("a fixed life threshold")),
    };
    cursor.consume("or")?;
    cursor.enter("condition.static.life-total.direction");
    let direction = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("more" | "greater") => ControlledCountDirection::OrMore,
        Some("less" | "fewer") => ControlledCountDirection::OrFewer,
        _ => return Err(cursor.fail("`more`, `greater`, `less` or `fewer`")),
    };
    let word = cursor.peek().expect("direction was checked").to_string();
    cursor.consume(&word)?;
    cursor.enter("condition.static.life-total.noun");
    cursor.consume("life")?;
    Ok(StaticConditionAtom::ControllerLifeTotal {
        threshold,
        direction,
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

/// `an opponent lost life this turn`.
fn recognize_opponent_life_loss_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticConditionAtom> {
    cursor.enter("condition.static.opponent-life.subject");
    cursor.consume_phrase(&["an", "opponent"])?;
    cursor.enter("condition.static.opponent-life.verb");
    cursor.consume_phrase(&["lost", "life", "this", "turn"])?;
    Ok(StaticConditionAtom::OpponentLostLifeThisTurn)
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
