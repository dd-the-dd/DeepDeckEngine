//! Player-state propositions of conditions and their player subject.
//!
//! A condition such as `an opponent has eight or more cards in their
//! graveyard` composes three independent parts: a player subject (`you`, or a
//! quantified population such as `an opponent`, `each player`, `no opponent`;
//! CR 102.2-102.3), a verb frame owned by that subject (`controls`, `has`,
//! `has cast`, `has drawn`), and a typed amount over one reusable measure of
//! that player's state (controlled permanents through the shared object query,
//! life total, player counters, cards in one of their zones, spells cast or
//! cards drawn this turn). Every parent that reads a condition program
//! (`as long as`, sentence `if`, intervening `if`, activation restrictions,
//! entry exceptions) reaches these atoms through the shared static reader; no
//! parent recombines a subject with a measure on its own.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    ObjectKind, ObjectPredicate, recognize_counter_name, recognize_object_query,
    recognize_search_card_selector,
};
use crate::oracle::counter_name::CounterName;

/// How a fixed count threshold compares with the measured amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ThresholdOperator {
    AtLeast,
    AtMost,
}

/// `N or more`, `at least N`, `a`/`an` (at least one), `no` (at most zero),
/// `N or fewer|less`, `at most N`: one bounded comparison with a fixed value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CountThreshold {
    pub operator: ThresholdOperator,
    pub value: i64,
}

impl CountThreshold {
    pub(in crate::oracle::canonical) const fn at_least(value: i64) -> Self {
        Self {
            operator: ThresholdOperator::AtLeast,
            value,
        }
    }

    /// True when exactly one object satisfies the threshold's lower bound,
    /// so the following noun is singular (`a card`, `one card`).
    fn singular(self) -> bool {
        self.value == 1 && self.operator == ThresholdOperator::AtLeast
    }
}

/// The quantifier of a player population (CR 102.2 opponents, CR 102.1 players).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerQuantifier {
    /// `an opponent`, `a player`, `any player`: at least one member.
    Some,
    /// `each opponent`, `each player`: every member.
    Every,
    /// `no opponent`, `no player`: no member.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerPopulation {
    /// The opponents of the ability's controller.
    Opponents,
    /// Every player still in the game.
    Players,
}

/// The player a player-state proposition is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionPlayerSubject {
    /// `you`: the ability's controller.
    Controller,
    Quantified {
        quantifier: PlayerQuantifier,
        population: PlayerPopulation,
    },
    /// `your opponents` taken together (CR 102.3): one measure summed over
    /// every opponent of the controller, not a per-player quantifier.
    OpponentsTogether,
}

impl ConditionPlayerSubject {
    const fn is_controller(self) -> bool {
        matches!(self, Self::Controller)
    }
}

/// A zone holding the cards of one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerCardZone {
    Hand,
    Graveyard,
}

/// One measured fact about the subject player, compared with a threshold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerStateMeasure {
    /// Permanents the player controls matching the shared object query.
    ControlledPermanents(ObjectPredicate),
    /// The player's life total (CR 119.1).
    Life,
    /// Counters of one kind on the player (CR 122.1, e.g. poison).
    PlayerCounters(CounterName),
    /// Cards in one of the player's zones matching the shared card selector.
    CardsInZone {
        zone: PlayerCardZone,
        predicate: ObjectPredicate,
    },
    /// Spells the player cast this turn matching the shared spell query;
    /// `None` counts every spell.
    SpellsCastThisTurn(Option<ObjectPredicate>),
    /// Cards the player drew this turn.
    CardsDrawnThisTurn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerStateProposition {
    pub measure: PlayerStateMeasure,
    pub threshold: CountThreshold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerStateConditionNode {
    pub subject: ConditionPlayerSubject,
    pub proposition: PlayerStateProposition,
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a token"))?
        .to_string();
    cursor.consume(&raw)
}

fn quantified_subject_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(PlayerQuantifier, PlayerPopulation)> {
    let population = if word_is(cursor, 1, "opponent") {
        PlayerPopulation::Opponents
    } else if word_is(cursor, 1, "player") {
        PlayerPopulation::Players
    } else {
        return None;
    };
    let quantifier = match cursor.peek()?.to_ascii_lowercase().as_str() {
        "an" if population == PlayerPopulation::Opponents => PlayerQuantifier::Some,
        "a" if population == PlayerPopulation::Players => PlayerQuantifier::Some,
        "any" => PlayerQuantifier::Some,
        "each" => PlayerQuantifier::Every,
        "no" => PlayerQuantifier::None,
        _ => return None,
    };
    Some((quantifier, population))
}

/// True at `your opponents control`, the collective subject of a summed count.
pub(in crate::oracle::canonical) fn collective_opponents_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "your") && word_is(cursor, 1, "opponents") && word_is(cursor, 2, "control")
}

/// `your opponents control N QUERY`: permanents controlled by any opponent,
/// counted together.
pub(in crate::oracle::canonical) fn recognize_collective_opponents_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerStateConditionNode> {
    cursor.enter("condition.player.collective-subject");
    cursor.consume_phrase(&["your", "opponents", "control"])?;
    Ok(PlayerStateConditionNode {
        subject: ConditionPlayerSubject::OpponentsTogether,
        proposition: recognize_controlled_permanents_proposition(cursor)?,
    })
}

/// True at a quantified player subject followed by a verb this reader owns:
/// `an opponent controls`, `each player has`, `no opponent controls`. Other
/// verbs (`an opponent lost life`, `a player cast`) stay with their readers.
pub(in crate::oracle::canonical) fn quantified_player_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    quantified_subject_at(cursor).is_some()
        && (word_is(cursor, 2, "controls") || word_is(cursor, 2, "has"))
}

/// Reads a quantified player subject and its proposition.
pub(in crate::oracle::canonical) fn recognize_quantified_player_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerStateConditionNode> {
    cursor.enter("condition.player.subject");
    let (quantifier, population) =
        quantified_subject_at(cursor).ok_or_else(|| cursor.fail("a quantified player subject"))?;
    consume_current(cursor)?;
    consume_current(cursor)?;
    let subject = ConditionPlayerSubject::Quantified {
        quantifier,
        population,
    };
    cursor.enter("condition.player.verb");
    let proposition = if word_is(cursor, 0, "controls") {
        cursor.consume("controls")?;
        recognize_controlled_permanents_proposition(cursor)?
    } else {
        cursor.consume("has")?;
        recognize_player_possession_proposition(cursor, subject)?
    };
    Ok(PlayerStateConditionNode {
        subject,
        proposition,
    })
}

/// After `has`/`have` (and `you've`): a history verb (`cast`, `drawn`) or a
/// possessed amount (life, counters, cards in a zone). The subject decides
/// the possessive agreement of the zone (`your` or `their`).
pub(in crate::oracle::canonical) fn recognize_player_possession_proposition(
    cursor: &mut OracleTokenCursor<'_>,
    subject: ConditionPlayerSubject,
) -> MachineResult<PlayerStateProposition> {
    cursor.enter("condition.player.possession");
    if word_is(cursor, 0, "cast") {
        cursor.consume("cast")?;
        return recognize_spells_cast_proposition(cursor);
    }
    if word_is(cursor, 0, "drawn") {
        cursor.consume("drawn")?;
        cursor.enter("condition.player.drawn.amount");
        let threshold = recognize_count_threshold(cursor, true)?;
        cursor.enter("condition.player.drawn.noun");
        cursor.consume(if threshold.singular() {
            "card"
        } else {
            "cards"
        })?;
        cursor.enter("condition.player.drawn.duration");
        cursor.consume_phrase(&["this", "turn"])?;
        return Ok(PlayerStateProposition {
            measure: PlayerStateMeasure::CardsDrawnThisTurn,
            threshold,
        });
    }
    cursor.enter("condition.player.possession.amount");
    let numeric_amount =
        !(word_is(cursor, 0, "a") || word_is(cursor, 0, "an") || word_is(cursor, 0, "no"));
    let threshold = recognize_count_threshold(cursor, true)?;
    cursor.enter("condition.player.possession.noun");
    if word_is(cursor, 0, "life") {
        cursor.enter("condition.player.life.amount");
        cursor.require(numeric_amount, "a numeric life threshold")?;
        cursor.consume("life")?;
        return Ok(PlayerStateProposition {
            measure: PlayerStateMeasure::Life,
            threshold,
        });
    }
    let counter_noun =
        |offset| word_is(cursor, offset, "counter") || word_is(cursor, offset, "counters");
    let lexical_counter = cursor.peek_token().is_some_and(|token| {
        matches!(
            token.token_type.as_str(),
            "counterNamePart" | "counterValue"
        )
    });
    if lexical_counter || counter_noun(1) {
        cursor.enter("condition.player.counters.name");
        // A counter word the lexicon does not classify (`poison`) is still
        // an opaque one-word counter name when `counter(s)` follows it.
        let counter = if lexical_counter {
            recognize_counter_name(cursor)?
        } else {
            let raw = cursor.peek().expect("the counter noun follows").to_string();
            consume_current(cursor)?;
            CounterName::from_parts([raw.as_str()])
                .ok_or_else(|| cursor.fail("a non-empty counter name"))?
        };
        cursor.enter("condition.player.counters.noun");
        cursor.consume(if threshold.singular() {
            "counter"
        } else {
            "counters"
        })?;
        cursor.enter("condition.player.counters.relation");
        cursor.require(
            !word_is(cursor, 0, "on"),
            "counters on the player, not on an object",
        )?;
        return Ok(PlayerStateProposition {
            measure: PlayerStateMeasure::PlayerCounters(counter),
            threshold,
        });
    }
    let predicate = recognize_search_card_selector(cursor, !threshold.singular())?;
    cursor.enter("condition.player.cards.zone");
    cursor.consume("in")?;
    if word_is(cursor, 0, "your") || word_is(cursor, 0, "their") {
        cursor.enter("condition.player.cards.possessive");
        cursor.require(
            word_is(cursor, 0, "your") == subject.is_controller(),
            "the zone possessive that agrees with the condition's player subject",
        )?;
        consume_current(cursor)?;
    } else {
        cursor.enter("condition.player.cards.bare-zone");
        cursor.require(
            word_is(cursor, 0, "hand"),
            "`hand` without a possessive, or a possessive zone",
        )?;
    }
    cursor.enter("condition.player.cards.zone-noun");
    let zone = if word_is(cursor, 0, "hand") {
        PlayerCardZone::Hand
    } else if word_is(cursor, 0, "graveyard") {
        PlayerCardZone::Graveyard
    } else {
        return Err(cursor.fail("`hand` or `graveyard`"));
    };
    consume_current(cursor)?;
    Ok(PlayerStateProposition {
        measure: PlayerStateMeasure::CardsInZone { zone, predicate },
        threshold,
    })
}

/// `[N or more | a | no] QUERY` after `controls`: the shared permanent query.
fn recognize_controlled_permanents_proposition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerStateProposition> {
    cursor.enter("condition.player.controls.amount");
    let threshold = recognize_count_threshold(cursor, true)?;
    cursor.enter("condition.player.controls.query");
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query.controller.is_none(),
        "a permanent query without its own controller relation",
    )?;
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent),
        "a predicate valid for a battlefield permanent",
    )?;
    Ok(PlayerStateProposition {
        measure: PlayerStateMeasure::ControlledPermanents(query.predicate),
        threshold,
    })
}

/// `cast a QUERY spell this turn` / `cast N or more spells this turn`. A
/// filtered count has no evaluator yet; a filtered presence and an unfiltered
/// count are distinct supported propositions.
fn recognize_spells_cast_proposition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerStateProposition> {
    cursor.enter("condition.player.cast.amount");
    let threshold = recognize_count_threshold(cursor, true)?;
    cursor.enter("condition.player.cast.query");
    let query = super::recognize_object_noun_query(cursor, super::ObjectQueryContext::ORDINARY)?
        .into_spell_cast_query(cursor)?;
    cursor.require(
        query.caster.is_none(),
        "one caster relation, supplied by the condition's player subject",
    )?;
    cursor.require(
        query.predicate.is_in_domain(ObjectKind::Spell),
        "an explicit spell-domain query",
    )?;
    let predicate = (query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Spell))
        .then_some(query.predicate);
    cursor.enter("condition.player.cast.filtered-count");
    cursor.require(
        predicate.is_none() || threshold == CountThreshold::at_least(1),
        "an unfiltered spell count, or the presence of one filtered spell",
    )?;
    cursor.enter("condition.player.cast.duration");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(PlayerStateProposition {
        measure: PlayerStateMeasure::SpellsCastThisTurn(predicate),
        threshold,
    })
}

/// Reads a fixed count threshold. `allow_article` admits `a`/`an` as at
/// least one; `no` is at most zero. A declared variable is not a threshold.
pub(in crate::oracle::canonical) fn recognize_count_threshold(
    cursor: &mut OracleTokenCursor<'_>,
    allow_article: bool,
) -> MachineResult<CountThreshold> {
    cursor.enter("condition.threshold.start");
    if allow_article && (word_is(cursor, 0, "a") || word_is(cursor, 0, "an")) {
        consume_current(cursor)?;
        return Ok(CountThreshold::at_least(1));
    }
    if word_is(cursor, 0, "no") {
        cursor.consume("no")?;
        return Ok(CountThreshold {
            operator: ThresholdOperator::AtMost,
            value: 0,
        });
    }
    if word_is(cursor, 0, "at") {
        cursor.consume("at")?;
        cursor.enter("condition.threshold.bound");
        let operator = if word_is(cursor, 0, "least") {
            ThresholdOperator::AtLeast
        } else if word_is(cursor, 0, "most") {
            ThresholdOperator::AtMost
        } else {
            return Err(cursor.fail("`least` or `most` after `at`"));
        };
        consume_current(cursor)?;
        let value = recognize_threshold_value(cursor)?;
        return Ok(CountThreshold { operator, value });
    }
    let value = recognize_threshold_value(cursor)?;
    cursor.enter("condition.threshold.direction");
    cursor.consume("or")?;
    let operator = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("more" | "greater") => ThresholdOperator::AtLeast,
        Some("fewer" | "less") => ThresholdOperator::AtMost,
        _ => return Err(cursor.fail("`more`, `greater`, `fewer` or `less`")),
    };
    consume_current(cursor)?;
    Ok(CountThreshold { operator, value })
}

fn recognize_threshold_value(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    cursor.enter("condition.threshold.value");
    let value = cursor
        .peek()
        .filter(|raw| !raw.eq_ignore_ascii_case("a") && !raw.eq_ignore_ascii_case("an"))
        .and_then(|raw| raw.parse::<i64>().ok().or_else(|| parse_number_word(raw)))
        .ok_or_else(|| cursor.fail("a fixed nonnegative threshold"))?;
    cursor.require(value >= 0, "a fixed nonnegative threshold")?;
    consume_current(cursor)?;
    Ok(value)
}

#[cfg(test)]
#[path = "player_condition_tests.rs"]
mod tests;
