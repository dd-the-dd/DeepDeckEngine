//! The relative-clause slot of the shared object query (C4):
//! `NOUN that|which|who CLAUSE`.
//!
//! ```text
//! Relative := ("that" | "which" | "who") Clause
//! Clause   := ("has" | "have") CounterQualifier                 present
//!           | ("is" | "are" | "isn't" | "aren't") ["not"] Status ("or" Status)*
//!           | Past ("or" Past)* "this" "turn"                    history
//!           | "dealt" ["combat"] "damage" "to" Player "this" "turn"
//!           | "targets" Targeted ("or" Targeted)*                  spell targets
//! Past     := "entered" ["the" "battlefield"] | "attacked" | "blocked"
//!           | ("was" | "were") "dealt" "damage" | "dealt" "damage"
//!           | ("didn't" | "did" "not") Past
//! Player   := "you" | "an" "opponent" | "a" "player"
//! Targeted := Player | ("a" | "an") PermanentQuery
//!
//! Departure := ["that" | "which"] "died" ["under" Possessor "control"] "this" "turn"
//!            | [("that" | "which") ("was" | "were")] "put" "into" Graveyard
//!              "from" "the" "battlefield" "this" "turn"
//! Possessor := "your" | "an" "opponent's"
//! Graveyard := Possessor "graveyard" | "a" "graveyard" | "graveyards"
//! ```
//!
//! A status clause coordinates its statuses with one connector: `or` is a
//! union, `and` an intersection (`that are tapped and attacking`).
//!
//! A past-tense clause names a this-turn fact of the object (CR 608.2c
//! history); the engine reads it from its object history. Other relatives
//! (`that player controls`, `that targets only a single ...`) are not
//! claimed here and keep their explicit failure.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DamagedPlayer, DepartureHistory, DeparturePlayer, ObjectControllerScope, ObjectPredicate,
    ObjectQueryContext, ObjectRelation, ObjectStatus, TargetedObjects, TargetedPermanents,
};

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn is_relative_pronoun(cursor: &OracleTokenCursor<'_>) -> bool {
    ["that", "which", "who"]
        .iter()
        .any(|pronoun| word_at(cursor, 0, pronoun))
}

/// The first past verb of a history clause at `offset`, with its width.
fn past_fact_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(ObjectStatus, usize)> {
    if word_at(cursor, offset, "entered") {
        let width =
            if word_at(cursor, offset + 1, "the") && word_at(cursor, offset + 2, "battlefield") {
                3
            } else {
                1
            };
        return Some((ObjectStatus::EnteredThisTurn, width));
    }
    if word_at(cursor, offset, "attacked") {
        return Some((ObjectStatus::AttackedThisTurn, 1));
    }
    if word_at(cursor, offset, "blocked") {
        return Some((ObjectStatus::BlockedThisTurn, 1));
    }
    if (word_at(cursor, offset, "was") || word_at(cursor, offset, "were"))
        && word_at(cursor, offset + 1, "dealt")
        && word_at(cursor, offset + 2, "damage")
    {
        return Some((ObjectStatus::WasDealtDamageThisTurn, 3));
    }
    if word_at(cursor, offset, "dealt")
        && word_at(cursor, offset + 1, "damage")
        && word_at(cursor, offset + 2, "this")
    {
        return Some((ObjectStatus::DealtDamageThisTurn, 2));
    }
    None
}

/// `dealt [combat] damage to PLAYER this turn` at `offset`: the recipient
/// relation, whether only combat damage counts, and the phrase width.
fn dealt_damage_to_player_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(DamagedPlayer, bool, usize)> {
    if !word_at(cursor, offset, "dealt") {
        return None;
    }
    let combat = word_at(cursor, offset + 1, "combat");
    let damage = offset + 1 + usize::from(combat);
    if !word_at(cursor, damage, "damage") || !word_at(cursor, damage + 1, "to") {
        return None;
    }
    let player = damage + 2;
    let (recipient, player_width) = if word_at(cursor, player, "you") {
        (DamagedPlayer::AbilityController, 1)
    } else if word_at(cursor, player, "an") && word_at(cursor, player + 1, "opponent") {
        (DamagedPlayer::OpponentOfAbilityController, 2)
    } else if word_at(cursor, player, "a") && word_at(cursor, player + 1, "player") {
        (DamagedPlayer::AnyPlayer, 2)
    } else {
        return None;
    };
    let period = player + player_width;
    (word_at(cursor, period, "this") && word_at(cursor, period + 1, "turn")).then_some((
        recipient,
        combat,
        period + 2 - offset,
    ))
}

/// A negated past verb: `didn't attack` / `did not enter`.
fn negated_past_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(ObjectStatus, usize)> {
    let (skip, base) =
        if word_at(cursor, offset, "didn't") || word_at(cursor, offset, "didn\u{2019}t") {
            (1, offset + 1)
        } else if word_at(cursor, offset, "did") && word_at(cursor, offset + 1, "not") {
            (2, offset + 2)
        } else {
            return None;
        };
    let fact = if word_at(cursor, base, "enter") {
        ObjectStatus::EnteredThisTurn
    } else if word_at(cursor, base, "attack") {
        ObjectStatus::AttackedThisTurn
    } else if word_at(cursor, base, "block") {
        ObjectStatus::BlockedThisTurn
    } else {
        return None;
    };
    Some((fact, skip + 1))
}

/// True when a relative clause this slot reads starts at the cursor. The
/// `that targets ...` relation of a stack object is claimed only when the
/// parent states that its query describes declared target candidates
/// (`stack_targets`); another parent (a spell-cast event) reads the targets
/// of the cast spell with its own typed relation.
pub(in crate::oracle::canonical) fn relative_clause_starts_in(
    cursor: &OracleTokenCursor<'_>,
    stack_targets: bool,
) -> bool {
    relative_clause_starts(cursor)
        || (stack_targets && is_relative_pronoun(cursor) && targets_clause_starts(cursor, 1))
}

/// True when a relative clause of every query starts at the cursor.
pub(in crate::oracle::canonical) fn relative_clause_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    is_relative_pronoun(cursor)
        && (word_at(cursor, 1, "has") && super::counter_qualifier_starts(cursor, 2)
            || word_at(cursor, 1, "have") && super::counter_qualifier_starts(cursor, 2)
            || ["is", "are", "isn't", "aren't"]
                .iter()
                .any(|verb| word_at(cursor, 1, verb))
                && status_after_copula(cursor, 2)
            || past_fact_at(cursor, 1).is_some()
            || dealt_damage_to_player_at(cursor, 1).is_some()
            || negated_past_at(cursor, 1).is_some())
}

/// `targets PLAYER|a QUERY` at `offset`. `targets only a single ...` is a
/// count of the targets, not this relation, and is not claimed.
fn targets_clause_starts(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    word_at(cursor, offset, "targets")
        && (targeted_player_at(cursor, offset + 1).is_some()
            || word_at(cursor, offset + 1, "a")
            || word_at(cursor, offset + 1, "an"))
}

/// The player a spell targets at `offset`, with its width.
fn targeted_player_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(DamagedPlayer, usize)> {
    if word_at(cursor, offset, "you") {
        Some((DamagedPlayer::AbilityController, 1))
    } else if word_at(cursor, offset, "an") && word_at(cursor, offset + 1, "opponent") {
        Some((DamagedPlayer::OpponentOfAbilityController, 2))
    } else if word_at(cursor, offset, "a") && word_at(cursor, offset + 1, "player") {
        Some((DamagedPlayer::AnyPlayer, 2))
    } else {
        None
    }
}

/// `targets you or a permanent you control` (CR 115.1): every alternative is
/// a player relative to the ability controller or one permanent of the
/// shared object query, whose own controller relation is kept. At most one
/// player and one permanent alternative are stated.
fn recognize_targets_clause(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relative-clause.targets");
    cursor.consume("targets")?;
    let mut targeted = TargetedObjects {
        players: None,
        permanents: None,
    };
    loop {
        if let Some((player, width)) = targeted_player_at(cursor, 0) {
            cursor.enter("selector.relative-clause.targets.player");
            cursor.require(
                targeted.players.is_none(),
                "one targeted player alternative",
            )?;
            for _ in 0..width {
                let raw = cursor.peek().unwrap_or_default().to_string();
                cursor.consume(&raw)?;
            }
            targeted.players = Some(player);
        } else {
            cursor.enter("selector.relative-clause.targets.object");
            cursor.require(
                targeted.permanents.is_none(),
                "one targeted permanent alternative",
            )?;
            let article = cursor.peek().unwrap_or_default().to_string();
            cursor.require(
                article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an"),
                "`a` or `an` before the targeted object",
            )?;
            cursor.consume(&article)?;
            let query = super::recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)?
                .into_object_query(cursor)?;
            cursor.enter("selector.relative-clause.targets.object-domain");
            cursor.require(
                query.denotes_battlefield_permanents() && !query.exclude_source,
                "a targeted permanent of the shared battlefield query",
            )?;
            cursor.require(
                !matches!(
                    query.controller,
                    Some(ObjectControllerScope::EventPlayer | ObjectControllerScope::Player(_))
                ),
                "a targeted permanent's controller relative to the ability controller",
            )?;
            cursor.require(
                literal_numeric_comparisons(&query.predicate),
                "a targeted permanent compared with literal values only",
            )?;
            targeted.permanents = Some(TargetedPermanents {
                predicate: Box::new(query.predicate),
                controller: query.controller,
            });
        }
        let another = word_at(cursor, 0, "or")
            && (targeted_player_at(cursor, 1).is_some()
                || word_at(cursor, 1, "a")
                || word_at(cursor, 1, "an"));
        if !another {
            break;
        }
        cursor.consume("or")?;
    }
    Ok(ObjectPredicate::Relation(ObjectRelation::Targets(targeted)))
}

/// Whether every numeric comparison of `predicate` compares with a literal:
/// the targeted permanent is tested while the spell is chosen as a target,
/// with no declaration or resolution context for another operand.
fn literal_numeric_comparisons(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => matches!(
            right.as_ref(),
            super::NumericExpression::Atom(super::NumericAtom::Integer(_))
        ),
        ObjectPredicate::Not(operand) => literal_numeric_comparisons(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().all(literal_numeric_comparisons)
        }
        ObjectPredicate::Relation(ObjectRelation::Targets(_)) => false,
        _ => true,
    }
}

fn status_after_copula(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let offset = offset + usize::from(word_at(cursor, offset, "not"));
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| super::lookup_status_predicate(&raw).is_some())
}

/// Reads one relative clause into the object predicate it states.
pub(in crate::oracle::canonical) fn recognize_relative_clause(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relative-clause.pronoun");
    let pronoun = cursor.peek().unwrap_or_default().to_string();
    cursor.require(is_relative_pronoun(cursor), "`that`, `which` or `who`")?;
    cursor.consume(&pronoun)?;
    if word_at(cursor, 0, "has") || word_at(cursor, 0, "have") {
        cursor.enter("selector.relative-clause.possession");
        let verb = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&verb)?;
        return super::recognize_counter_qualifier(cursor);
    }
    if ["is", "are", "isn't", "aren't"]
        .iter()
        .any(|verb| word_at(cursor, 0, verb))
    {
        cursor.enter("selector.relative-clause.status");
        let verb = cursor.peek().unwrap_or_default().to_ascii_lowercase();
        cursor.consume(&verb)?;
        let mut negated = verb.ends_with("n't");
        if word_at(cursor, 0, "not") {
            cursor.consume("not")?;
            negated = !negated;
        }
        let mut members = Vec::new();
        // One connector coordinates the statuses: `tapped or attacking` is a
        // union, `tapped and attacking` an intersection. A mixed list has no
        // proved precedence and is refused.
        let mut connector: Option<&'static str> = None;
        loop {
            let raw = cursor.peek().unwrap_or_default().to_ascii_lowercase();
            let status = cursor
                .require_value(super::lookup_status_predicate(&raw), "a registered status")?;
            cursor.consume(&raw)?;
            members.push(status);
            let next = ["or", "and"]
                .into_iter()
                .find(|candidate| word_at(cursor, 0, candidate));
            let more = next.is_some()
                && cursor
                    .peek_ahead(1)
                    .is_some_and(|raw| super::lookup_status_predicate(raw).is_some());
            if !more {
                break;
            }
            let next = next.expect("checked status connector");
            cursor.enter("selector.relative-clause.status.connector");
            cursor.require(
                connector.is_none_or(|previous| previous == next),
                "one connector coordinating the statuses of a relative clause",
            )?;
            connector = Some(next);
            cursor.consume(next)?;
        }
        let predicate = match (members.len(), connector) {
            (1, _) => members.remove(0),
            (_, Some("and")) => ObjectPredicate::Intersection(members),
            _ => ObjectPredicate::Union(members),
        };
        return Ok(if negated {
            ObjectPredicate::Not(Box::new(predicate))
        } else {
            predicate
        });
    }
    if targets_clause_starts(cursor, 0) {
        return recognize_targets_clause(cursor);
    }
    if let Some((recipient, combat_only, width)) = dealt_damage_to_player_at(cursor, 0) {
        // `that dealt combat damage to you this turn`: a relation of the
        // permanent to the players its damage reached this turn.
        cursor.enter("selector.relative-clause.damage-recipient");
        for _ in 0..width {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
        }
        return Ok(ObjectPredicate::Relation(
            ObjectRelation::DealtDamageToPlayerThisTurn {
                recipient,
                combat_only,
            },
        ));
    }
    cursor.enter("selector.relative-clause.history");
    let mut members = Vec::new();
    loop {
        let (fact, width, negated) = match past_fact_at(cursor, 0) {
            Some((fact, width)) => (fact, width, false),
            None => {
                let (fact, width) = cursor.require_value(
                    negated_past_at(cursor, 0),
                    "a past-tense fact of the object this turn",
                )?;
                (fact, width, true)
            }
        };
        for _ in 0..width {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
        }
        let atom = ObjectPredicate::Status(fact);
        members.push(if negated {
            ObjectPredicate::Not(Box::new(atom))
        } else {
            atom
        });
        if word_at(cursor, 0, "or")
            && (past_fact_at(cursor, 1).is_some()
                || word_at(cursor, 1, "enter")
                || word_at(cursor, 1, "attack")
                || word_at(cursor, 1, "block"))
        {
            cursor.consume("or")?;
            // `didn't attack or enter`: the negation distributes.
            if let Some(ObjectPredicate::Not(_)) = members.last()
                && past_fact_at(cursor, 0).is_none()
            {
                let fact = if word_at(cursor, 0, "enter") {
                    ObjectStatus::EnteredThisTurn
                } else if word_at(cursor, 0, "attack") {
                    ObjectStatus::AttackedThisTurn
                } else {
                    ObjectStatus::BlockedThisTurn
                };
                let raw = cursor.peek().unwrap_or_default().to_string();
                cursor.consume(&raw)?;
                members.push(ObjectPredicate::Not(Box::new(ObjectPredicate::Status(
                    fact,
                ))));
                if word_at(cursor, 0, "or") {
                    cursor.consume("or")?;
                    continue;
                }
                break;
            }
            continue;
        }
        break;
    }
    cursor.enter("selector.relative-clause.period");
    cursor.consume_phrase(&["this", "turn"])?;
    // `didn't attack or enter` is `not attacked and not entered`.
    let all_negated = members
        .iter()
        .all(|member| matches!(member, ObjectPredicate::Not(_)));
    Ok(match members.len() {
        1 => members.remove(0),
        _ if all_negated => ObjectPredicate::Intersection(members),
        _ => ObjectPredicate::Union(members),
    })
}

/// One departure-history clause at the cursor: its token width, the offset
/// of its verb (`died` / `put`) and the history it states. A pure lookahead
/// over the complete clause, so a partial shape is never claimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DepartureClause {
    pub width: usize,
    pub verb_offset: usize,
    pub history: DepartureHistory,
}

/// `your` / `an opponent's`: a possessive player relative to the ability
/// controller, with its width.
fn possessive_player_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(DeparturePlayer, usize)> {
    if word_at(cursor, offset, "your") {
        Some((DeparturePlayer::AbilityController, 1))
    } else if word_at(cursor, offset, "an")
        && (word_at(cursor, offset + 1, "opponent's")
            || word_at(cursor, offset + 1, "opponent\u{2019}s"))
    {
        Some((DeparturePlayer::OpponentOfAbilityController, 2))
    } else {
        None
    }
}

/// The graveyard a departed object was put into: its owner relative to the
/// ability controller (CR 404.2: a card goes to its owner's graveyard), or
/// `None` for any graveyard, with the phrase width.
fn graveyard_holder_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(Option<DeparturePlayer>, usize)> {
    if word_at(cursor, offset, "graveyards") {
        return Some((None, 1));
    }
    if word_at(cursor, offset, "a") && word_at(cursor, offset + 1, "graveyard") {
        return Some((None, 2));
    }
    let (player, width) = possessive_player_at(cursor, offset)?;
    word_at(cursor, offset + width, "graveyard").then_some((Some(player), width + 1))
}

/// `that died [under PLAYER's control] this turn` and `[that was|were] put
/// into GRAVEYARD from the battlefield this turn` (CR 700.4: to die is to be
/// put into a graveyard from the battlefield). The objects are departures of
/// this turn; the query reports them as its own source set.
pub(in crate::oracle::canonical) fn departure_clause_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<DepartureClause> {
    let relative = word_at(cursor, 0, "that") || word_at(cursor, 0, "which");
    let mut at = usize::from(relative);
    if word_at(cursor, at, "died") {
        // A bare `died` after a noun is a finite verb of the parent clause.
        if !relative {
            return None;
        }
        let verb_offset = at;
        at += 1;
        let mut controller = None;
        if word_at(cursor, at, "under") {
            let (player, width) = possessive_player_at(cursor, at + 1)?;
            if !word_at(cursor, at + 1 + width, "control") {
                return None;
            }
            controller = Some(player);
            at += 2 + width;
        }
        if !(word_at(cursor, at, "this") && word_at(cursor, at + 1, "turn")) {
            return None;
        }
        return Some(DepartureClause {
            width: at + 2,
            verb_offset,
            history: DepartureHistory {
                controller,
                graveyard_owner: None,
            },
        });
    }
    if relative {
        if !(word_at(cursor, at, "was") || word_at(cursor, at, "were")) {
            return None;
        }
        at += 1;
    }
    if !(word_at(cursor, at, "put") && word_at(cursor, at + 1, "into")) {
        return None;
    }
    let verb_offset = at;
    at += 2;
    let (graveyard_owner, width) = graveyard_holder_at(cursor, at)?;
    at += width;
    let from_battlefield = ["from", "the", "battlefield", "this", "turn"]
        .iter()
        .enumerate()
        .all(|(index, expected)| word_at(cursor, at + index, expected));
    from_battlefield.then_some(DepartureClause {
        width: at + 5,
        verb_offset,
        history: DepartureHistory {
            controller: None,
            graveyard_owner,
        },
    })
}

/// A finite verb form that can only open a relative clause after a relative
/// pronoun (`that has`, `that died`, `which share`). Lexical only. The
/// modal `would` is left out: `damage that would be dealt` belongs to the
/// replacement frames that own it.
fn finite_relative_verb(raw: &str) -> bool {
    const VERBS: &[&str] = &[
        "is", "are", "was", "were", "isn't", "aren't", "wasn't", "weren't", "has", "have", "had",
        "hasn't", "haven't", "hadn't", "does", "did", "doesn't", "don't", "didn't", "can", "can't",
        "could", "couldn't", "shares", "share", "dies", "targets", "entered", "attacked",
        "blocked", "dealt", "died",
    ];
    let lower = raw.to_ascii_lowercase().replace('\u{2019}', "'");
    VERBS.contains(&lower.as_str())
}

/// The transactional end of a complete object operand. A relative pronoun
/// followed by a finite verb right after the operand can only qualify that
/// operand; when the shared query did not read it, the operand fails here
/// with the relative-clause diagnostic instead of handing the clause to the
/// parent's connector reader as if the operand had ended (AGENTS: a partial
/// read is not a commit).
pub(in crate::oracle::canonical) fn refuse_unread_relative_tail(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    if !is_relative_pronoun(cursor) || !cursor.peek_ahead(1).is_some_and(finite_relative_verb) {
        return Ok(());
    }
    cursor.enter("selector.relative-clause.unsupported");
    Err(cursor.fail("a relative clause the shared object query reads"))
}
