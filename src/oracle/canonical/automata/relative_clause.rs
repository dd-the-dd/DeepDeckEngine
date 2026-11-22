//! The relative-clause slot of the shared object query (C4):
//! `NOUN that|which|who CLAUSE`.
//!
//! ```text
//! Relative := ("that" | "which" | "who") Clause
//! Clause   := ("has" | "have") CounterQualifier                 present
//!           | ("is" | "are" | "isn't" | "aren't") ["not"] Status ("or" Status)*
//!           | Past ("or" Past)* "this" "turn"                    history
//! Past     := "entered" ["the" "battlefield"] | "attacked" | "blocked"
//!           | ("was" | "were") "dealt" "damage" | "dealt" "damage"
//!           | ("didn't" | "did" "not") Past
//! ```
//!
//! A past-tense clause names a this-turn fact of the object (CR 608.2c
//! history); the engine reads it from its object history. Other relatives
//! (`that player controls`, `that targets ...`) are not claimed here and
//! keep their explicit failure.
use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectPredicate, ObjectStatus};

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

/// True when a relative clause this slot reads starts at the cursor.
pub(in crate::oracle::canonical) fn relative_clause_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    is_relative_pronoun(cursor)
        && (word_at(cursor, 1, "has") && super::counter_qualifier_starts(cursor, 2)
            || word_at(cursor, 1, "have") && super::counter_qualifier_starts(cursor, 2)
            || ["is", "are", "isn't", "aren't"]
                .iter()
                .any(|verb| word_at(cursor, 1, verb))
                && status_after_copula(cursor, 2)
            || past_fact_at(cursor, 1).is_some()
            || negated_past_at(cursor, 1).is_some())
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
        loop {
            let raw = cursor.peek().unwrap_or_default().to_ascii_lowercase();
            let status = cursor
                .require_value(super::lookup_status_predicate(&raw), "a registered status")?;
            cursor.consume(&raw)?;
            members.push(status);
            let more = word_at(cursor, 0, "or")
                && cursor
                    .peek_ahead(1)
                    .is_some_and(|raw| super::lookup_status_predicate(&raw).is_some());
            if !more {
                break;
            }
            cursor.consume("or")?;
        }
        let predicate = if members.len() == 1 {
            members.remove(0)
        } else {
            ObjectPredicate::Union(members)
        };
        return Ok(if negated {
            ObjectPredicate::Not(Box::new(predicate))
        } else {
            predicate
        });
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
