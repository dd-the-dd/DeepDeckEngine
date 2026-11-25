//! Shared player populations independent of the action applied to them.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerSetReferenceNode {
    EachPlayer,
    OpponentsOfAbilityController,
}

pub(in crate::oracle::canonical) fn recognize_player_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSetReferenceNode> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
    {
        cursor.enter("player-set.quantifier");
        cursor.consume("each")?;
        cursor.enter("player-set.role");
        let role = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("opponent"))
        {
            cursor.consume("opponent")?;
            PlayerSetReferenceNode::OpponentsOfAbilityController
        } else {
            cursor.consume("player")?;
            PlayerSetReferenceNode::EachPlayer
        };
        cursor.enter("player-set.complete");
        return Ok(role);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("players"))
    {
        cursor.enter("player-set.role");
        cursor.consume("players")?;
        cursor.enter("player-set.complete");
        return Ok(PlayerSetReferenceNode::EachPlayer);
    }
    // Preserve the historical opponent reader's states and failure boundary.
    cursor.enter("player-set.possessor");
    cursor.consume("your")?;
    cursor.enter("player-set.role");
    cursor.consume("opponents")?;
    cursor.enter("player-set.complete");
    Ok(PlayerSetReferenceNode::OpponentsOfAbilityController)
}

#[cfg(test)]
#[path = "player_set_tests.rs"]
mod tests;

/// A zone holder is access scope, never an ownership predicate or target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneHolderQueryNode {
    Player(super::LocalPlayerReference),
    Players(PlayerSetReferenceNode),
}

pub(in crate::oracle::canonical) fn recognize_zone_holder_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ZoneHolderQueryNode> {
    cursor.enter("query.zone.holder");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        cursor.consume("your")?;
        Ok(ZoneHolderQueryNode::Player(
            super::LocalPlayerReference::Controller,
        ))
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
    {
        cursor.consume("all")?;
        Ok(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else if {
        let mut lexical = cursor.clone();
        super::recognize_card_zone_atom(&mut lexical)
            .ok()
            .is_some_and(|zone| {
                cursor
                    .peek()
                    .is_some_and(|raw| !raw.eq_ignore_ascii_case(zone.ir_name()))
            })
    } {
        // A plural zone noun denotes the holder population. The zone child
        // owns the actual noun and proves its lexical kind.
        Ok(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else {
        Err(cursor.fail("your zone or all player zones; zone targeting/selection requires an explicit typed producer"))
    }
}

/// Which end of a measure a superlative player predicate selects (`the
/// most`, `the highest`; `the fewest`, `the lowest`, `the least`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerExtremum {
    Greatest,
    Least,
}

/// A relative predicate restricting a player population (CR 102.1-102.3):
/// the same player-state propositions the condition grammar reads
/// (`player_condition.rs`), here about each member in turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerRelativePredicate {
    /// `who controls N QUERY`, `who has N life`, `with exactly 13 life`,
    /// `who doesn't control QUERY` (`negated`).
    State {
        proposition: super::PlayerStateProposition,
        negated: bool,
    },
    /// `who controls the most|fewest QUERY`, `with the most|least life`,
    /// `with the highest|lowest life total`: every member whose measure
    /// is the extremum of the population, ties included.
    Superlative {
        measure: super::PlayerStateMeasure,
        extremum: PlayerExtremum,
    },
}

/// A player population restricted by an exclusion and/or a relative
/// predicate (`each player other than its controller`, `each opponent who
/// doesn't control an Elf`, `each player who controls the most lands`).
/// The population, the excluded player and the predicate are independent
/// typed parts; the performer slot, a count or a recipient consume the same
/// node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerPopulationQueryNode {
    pub players: PlayerSetReferenceNode,
    /// `each other player`: the population minus the ability controller.
    pub excluding_controller: bool,
    /// `other than PLAYER`: one bound player removed from the population.
    pub except: Option<super::PlayerEffectSubject>,
    pub predicate: Option<PlayerRelativePredicate>,
}

/// True when a relative restriction of a player population starts at the
/// cursor (`who`, `whose`, `with`, `other than`). Lexical lookahead only.
pub(in crate::oracle::canonical) fn player_relative_clause_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "who")
        || word_at(cursor, 0, "whose")
        || word_at(cursor, 0, "with")
        || (word_at(cursor, 0, "other") && word_at(cursor, 1, "than"))
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The bindings a population exclusion may name (`other than its
/// controller`, `other than that player`).
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct PlayerRelativeContext {
    pub player_antecedent: Option<super::PlayerBinding>,
    pub event_player: bool,
    pub object_antecedent: Option<super::ObjectSubjectAntecedent>,
}

/// Reads the restriction of `population` at the cursor: an optional
/// `other than PLAYER`, then an optional relative predicate (`who ...`,
/// `with ...`). A restriction this grammar does not type fails with its
/// own state (`player-set.relative.*`), never as an unrestricted population.
pub(in crate::oracle::canonical) fn recognize_player_population_restriction(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
    context: &PlayerRelativeContext,
) -> MachineResult<PlayerPopulationQueryNode> {
    let mut except = None;
    if word_at(cursor, 0, "other") && word_at(cursor, 1, "than") {
        cursor.enter("player-set.relative.exclusion");
        cursor.require(
            !excluding_controller,
            "one exclusion of a player population (`each other player` already excludes you)",
        )?;
        cursor.consume_phrase(&["other", "than"])?;
        except = Some(recognize_excluded_player(cursor, context)?);
    }
    let predicate =
        if word_at(cursor, 0, "who") || word_at(cursor, 0, "whose") || word_at(cursor, 0, "with") {
            Some(recognize_player_relative_predicate(
                cursor,
                players,
                excluding_controller || except.is_some(),
            )?)
        } else {
            None
        };
    cursor.enter("player-set.relative.complete");
    cursor.require(
        except.is_some() || predicate.is_some(),
        "an exclusion or a relative predicate of the player population",
    )?;
    Ok(PlayerPopulationQueryNode {
        players,
        excluding_controller,
        except,
        predicate,
    })
}

/// `other than its controller|owner`, `other than that player`: one player
/// the binding environment already names (CR 109.4, 108.3). A declared
/// target (`other than target player`) would open a declaration inside the
/// population and is refused explicitly.
fn recognize_excluded_player(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PlayerRelativeContext,
) -> MachineResult<super::PlayerEffectSubject> {
    use super::source_sacrifice::atomic_action::player_subject::{
        player_anaphor_at, recognize_object_related_player_subject, recognize_player_effect_subject,
    };
    cursor.enter("player-set.relative.exclusion.player");
    if super::object_related_player_head_length(cursor).is_some() {
        return recognize_object_related_player_subject(cursor, context.object_antecedent.clone());
    }
    if player_anaphor_at(cursor).is_some() {
        return recognize_player_effect_subject(
            cursor,
            super::PlayerBinding(usize::MAX),
            context.player_antecedent,
            context.event_player,
        );
    }
    Err(cursor.fail(
        "a bound player after `other than` (`its controller`, `that player`); a declared target is not read here",
    ))
}

/// `who controls|has ...`, `who doesn't control ...`, `with ...`. The
/// proposition is the shared player-state grammar about the member; a
/// superlative compares the member with the whole population, so it is
/// read only for the unrestricted `each player` population.
fn recognize_player_relative_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    restricted: bool,
) -> MachineResult<PlayerRelativePredicate> {
    // The member is a third-person player: its zones are `their` zones.
    let member = super::ConditionPlayerSubject::Quantified {
        quantifier: super::PlayerQuantifier::Every,
        population: super::PlayerPopulation::Players,
    };
    if word_at(cursor, 0, "whose") {
        cursor.consume("whose")?;
        cursor.enter("player-set.relative.whose");
        return Err(cursor.fail(
            "a player predicate this grammar types; a possessive `whose` predicate is not read",
        ));
    }
    if word_at(cursor, 0, "with") {
        cursor.enter("player-set.relative.with");
        cursor.consume("with")?;
        if superlative_starts(cursor) {
            return recognize_superlative(
                cursor,
                players,
                restricted,
                SuperlativeFrame::Possession,
            );
        }
        cursor.enter("player-set.relative.with.proposition");
        return super::recognize_player_possession_proposition(cursor, member).map(|proposition| {
            PlayerRelativePredicate::State {
                proposition,
                negated: false,
            }
        });
    }
    cursor.enter("player-set.relative.who");
    cursor.consume("who")?;
    let negated = if word_at(cursor, 0, "doesn't") || word_at(cursor, 0, "don't") {
        let raw = cursor.peek().expect("checked negation").to_string();
        cursor.consume(&raw)?;
        true
    } else if word_at(cursor, 0, "does") && word_at(cursor, 1, "not") {
        cursor.consume_phrase(&["does", "not"])?;
        true
    } else {
        false
    };
    cursor.enter("player-set.relative.who.verb");
    let (controls, has) = if negated {
        ("control", "have")
    } else {
        ("controls", "has")
    };
    let possession = word_at(cursor, 0, has);
    if !possession && !word_at(cursor, 0, controls) {
        let found = cursor.peek().unwrap_or("").to_string();
        return Err(cursor.fail(&format!(
            "a player predicate this grammar types (`who controls`, `who has`, `who doesn't control`); `who {found}` is not read"
        )));
    }
    let raw = cursor.peek().expect("checked verb").to_string();
    cursor.consume(&raw)?;
    if possession {
        if !negated && superlative_starts(cursor) {
            return recognize_superlative(
                cursor,
                players,
                restricted,
                SuperlativeFrame::Possession,
            );
        }
        cursor.enter("player-set.relative.who.has");
        return super::recognize_player_possession_proposition(cursor, member).map(|proposition| {
            PlayerRelativePredicate::State {
                proposition,
                negated,
            }
        });
    }
    if !negated && superlative_starts(cursor) {
        return recognize_superlative(cursor, players, restricted, SuperlativeFrame::Controls);
    }
    cursor.enter("player-set.relative.who.controls");
    super::recognize_controlled_permanents_proposition(cursor).map(|proposition| {
        PlayerRelativePredicate::State {
            proposition,
            negated,
        }
    })
}

fn superlative_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "the")
        && ["most", "fewest", "least", "highest", "lowest", "greatest"]
            .iter()
            .any(|extremum| word_at(cursor, 1, extremum))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SuperlativeFrame {
    /// `controls the most QUERY`.
    Controls,
    /// `has|with the most life`, `the highest life total`.
    Possession,
}

/// `the most|fewest QUERY` after `controls`; `the most|least life`, `the
/// highest|lowest life total` after `has`/`with` (compared with every
/// player of the game, ties included).
fn recognize_superlative(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    restricted: bool,
    frame: SuperlativeFrame,
) -> MachineResult<PlayerRelativePredicate> {
    cursor.enter("player-set.relative.superlative");
    cursor.require(
        players == PlayerSetReferenceNode::EachPlayer && !restricted,
        "a superlative of every player (`each player who controls the most ...`)",
    )?;
    cursor.consume("the")?;
    let written = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`most`, `fewest`, `highest` or `lowest`"))?;
    let extremum = match written.as_str() {
        "most" | "highest" | "greatest" => PlayerExtremum::Greatest,
        "fewest" | "least" | "lowest" => PlayerExtremum::Least,
        _ => return Err(cursor.fail("`most`, `fewest`, `highest` or `lowest`")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    let measure = match frame {
        SuperlativeFrame::Controls => {
            cursor.enter("player-set.relative.superlative.controls");
            cursor.require(
                written == "most" || written == "fewest",
                "`the most` or `the fewest` before a controlled-permanent query",
            )?;
            let query = super::recognize_object_query(cursor)?;
            cursor.require(
                query.controller.is_none(),
                "a permanent query without its own controller relation",
            )?;
            cursor.require(
                query
                    .predicate
                    .supports_domain(super::ObjectKind::Permanent),
                "a predicate valid for a battlefield permanent",
            )?;
            super::PlayerStateMeasure::ControlledPermanents {
                predicate: query.predicate,
                exclude_source: false,
            }
        }
        SuperlativeFrame::Possession => {
            cursor.enter("player-set.relative.superlative.life");
            cursor.consume("life")?;
            if matches!(written.as_str(), "highest" | "lowest" | "greatest") {
                cursor.consume("total")?;
            } else {
                cursor.require(
                    !word_at(cursor, 0, "total"),
                    "`the most life` or `the highest life total`",
                )?;
            }
            super::PlayerStateMeasure::Life
        }
    };
    // `or tied for the most ...` restates the ties the extremum includes;
    // it is not read here and fails at the parent's verb.
    cursor.enter("player-set.relative.superlative.complete");
    Ok(PlayerRelativePredicate::Superlative { measure, extremum })
}
