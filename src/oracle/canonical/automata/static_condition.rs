//! `as long as CONDITION` (CR 611.3a): the Boolean condition that gates a
//! static ability. The parent owns `as long as` and the clause boundary (a
//! comma before the body, or the sentence end after it); this child owns only
//! the Boolean program over typed state propositions. Every atom is a reusable
//! proposition that the engine evaluates continuously, not a card recipe.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BooleanConditionNode, ControlledCountDirection, LocalConditionNode, NumericAtom,
    ObjectPredicate, recognize_boolean_condition_list_before_separator, recognize_numeric_atom,
    recognize_search_card_selector, recognize_source_reference, recognize_trigger_guard_atom,
    source_reference_starts_at,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StaticConditionAtom {
    /// Controller permanent presence / fixed-count atoms shared with guards.
    Local(LocalConditionNode),
    SourceState(SourceStateProposition),
    ControllerEventCount {
        event: ControllerTurnEvent,
        threshold: i64,
    },
    ControllerGainedLifeThisTurn,
    CardsInControllerGraveyard {
        threshold: i64,
        direction: ControlledCountDirection,
        predicate: ObjectPredicate,
    },
    ControllerHasEnduringStory,
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
    if word_is(cursor, 0, "you") && word_is(cursor, 1, "control") {
        return recognize_trigger_guard_atom(cursor).map(StaticConditionAtom::Local);
    }
    if contraction_is(cursor, 0, "you", "ve")
        || (word_is(cursor, 0, "you")
            && (word_is(cursor, 1, "have") || word_is(cursor, 1, "gained")))
    {
        return recognize_controller_history_atom(cursor);
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
    } else {
        cursor.consume("you")?;
        // `you gained life this turn` is the simple-past form of the same history.
        if !word_is(cursor, 0, "gained") {
            cursor.consume("have")?;
        }
    }
    cursor.enter("condition.static.controller-history.verb");
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
            cursor.consume("life")?;
            cursor.consume_phrase(&["this", "turn"])?;
            Ok(StaticConditionAtom::ControllerGainedLifeThisTurn)
        }
        "an" => {
            cursor.consume_phrase(&["an", "enduring", "story"])?;
            Ok(StaticConditionAtom::ControllerHasEnduringStory)
        }
        _ => Err(cursor.fail("`drawn`, `cast`, `gained` or an enduring story")),
    }
}
