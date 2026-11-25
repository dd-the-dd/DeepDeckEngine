//! Shared player populations independent of the action applied to them.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerSetReferenceNode {
    EachPlayer,
    OpponentsOfAbilityController,
    /// `each other opponent` after an event naming one opponent (`deals
    /// combat damage to an opponent, it deals that much damage to each other
    /// opponent`): the ability controller's opponents but the event's player
    /// (CR 102.3). Read only by `recognize_other_opponents_of_event_player`.
    OpponentsOtherThanEventPlayer,
}

/// `each other opponent`. Lexical lookahead only.
pub(in crate::oracle::canonical) fn other_opponents_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "each") && word_at(cursor, 1, "other") && word_at(cursor, 2, "opponent")
}

/// `each other opponent`: `other` excludes the opponent the triggering
/// event names, so the frame must bind an event player; without one the
/// phrase has no antecedent and fails here.
pub(in crate::oracle::canonical) fn recognize_other_opponents_of_event_player(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSetReferenceNode> {
    cursor.enter("player-set.other-opponent");
    cursor.consume_phrase(&["each", "other", "opponent"])?;
    cursor.enter("player-set.other-opponent.antecedent");
    cursor.require(
        super::installed_binding_environment(cursor)
            .frame
            .event_player,
        "an event player that `each other opponent` excludes",
    )?;
    Ok(PlayerSetReferenceNode::OpponentsOtherThanEventPlayer)
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
    /// `who lost life this turn`, `who was dealt combat damage this turn`:
    /// an event of this turn whose player is the member.
    History(MemberHistory),
}

/// An event of this turn a population member took part in (CR 119.3 life
/// loss, CR 120.3 damage dealt to a player).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum MemberHistory {
    LostLife,
    DealtDamage { combat_only: bool },
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
    /// `the player who controls the most creatures`, `the player with the
    /// most life`: a definite description names one player. Only a sole
    /// member holding the extremum is that player; on a tie no player is
    /// (an `or tied` restatement would include the tied players and is not
    /// this description).
    pub unique: bool,
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
    recognize_player_population_restriction_in_number(
        cursor,
        players,
        excluding_controller,
        context,
        false,
    )
}

/// The same restriction after a plural population noun (`opponents who
/// control an artifact`): the relative verb agrees in the plural.
pub(in crate::oracle::canonical) fn recognize_player_population_restriction_in_number(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
    context: &PlayerRelativeContext,
    plural: bool,
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
                plural,
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
        unique: false,
    })
}

/// True when `the player who|with ...` starts at the cursor: a definite
/// description of one player by a relative predicate. Lexical lookahead.
pub(in crate::oracle::canonical) fn definite_player_description_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "the")
        && word_at(cursor, 1, "player")
        && (word_at(cursor, 2, "who") || word_at(cursor, 2, "with"))
}

/// `the player who controls the most QUERY`, `the player with the most
/// life`, `the player who has the highest life total`: the shared
/// superlative over every player (`recognize_player_population_restriction`)
/// read as one player (`unique`). A description by any other predicate
/// (`the player who lost life this turn`) does not single out one player
/// and fails explicitly; so does `or tied`, which names several players.
pub(in crate::oracle::canonical) fn recognize_definite_player_description(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PlayerRelativeContext,
) -> MachineResult<PlayerPopulationQueryNode> {
    cursor.enter("player-set.definite-description");
    cursor.consume_phrase(&["the", "player"])?;
    let mut query = recognize_player_population_restriction(
        cursor,
        PlayerSetReferenceNode::EachPlayer,
        false,
        context,
    )?;
    cursor.enter("player-set.definite-description.superlative");
    cursor.require(
        matches!(
            query.predicate,
            Some(PlayerRelativePredicate::Superlative { .. })
        ) && query.except.is_none(),
        "a superlative that singles out one player (`the player with the most life`)",
    )?;
    cursor.require(
        !(word_at(cursor, 0, "or") && word_at(cursor, 1, "tied")),
        "one player; `or tied` names every tied player and is not a definite description",
    )?;
    query.unique = true;
    Ok(query)
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
    plural: bool,
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
    if !negated && let Some(history) = recognize_member_history(cursor, plural)? {
        return Ok(PlayerRelativePredicate::History(history));
    }
    cursor.enter("player-set.relative.who.verb");
    // `who doesn't control`, `opponents who control`: the base form after
    // a negation or a plural member; the third person otherwise.
    let (controls, has) = if negated || plural {
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

/// `lost life this turn`, `was|were dealt [combat] damage this turn` after
/// `who`: the member's events of this turn. `None` (nothing consumed) for
/// another verb, which the proposition grammar reads.
fn recognize_member_history(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<Option<MemberHistory>> {
    let history = if word_at(cursor, 0, "lost") && word_at(cursor, 1, "life") {
        cursor.enter("player-set.relative.who.lost-life");
        cursor.consume_phrase(&["lost", "life"])?;
        MemberHistory::LostLife
    } else if (word_at(cursor, 0, "was") || word_at(cursor, 0, "were"))
        && word_at(cursor, 1, "dealt")
    {
        cursor.enter("player-set.relative.who.dealt-damage");
        cursor.require(
            word_at(cursor, 0, if plural { "were" } else { "was" }),
            "`was` after a singular member, `were` after a plural one",
        )?;
        let copula = cursor.peek().expect("checked copula").to_string();
        cursor.consume(&copula)?;
        cursor.consume("dealt")?;
        let combat_only = word_at(cursor, 0, "combat");
        if combat_only {
            cursor.consume("combat")?;
        }
        cursor.consume("damage")?;
        MemberHistory::DealtDamage { combat_only }
    } else {
        return Ok(None);
    };
    // CR 500: the events of this turn; another window is not read here.
    cursor.enter("player-set.relative.who.history-window");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(Some(history))
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
        // `the most cards in hand`: the comparative possession measure the
        // `more cards in hand than each other player` condition reads.
        SuperlativeFrame::Possession if word_at(cursor, 0, "cards") => {
            cursor.enter("player-set.relative.superlative.cards");
            cursor.require(
                matches!(written.as_str(), "most" | "fewest"),
                "`the most|fewest cards in ZONE`",
            )?;
            super::recognize_comparative_possession(cursor)?
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

/// True when the action's player slot names a player other than the
/// ability's controller, so a distributive `they` in its trailing operands
/// (`... for each creature they control`) names that player: a performed
/// action (its performer), each member of a player set, a declared or bound
/// player, or another player reference.
pub(in crate::oracle::canonical) fn names_acting_player(effect: &super::LocalEffectNode) -> bool {
    match effect {
        super::LocalEffectNode::Performed(_)
        | super::LocalEffectNode::QuantifiedPlayerAction(_)
        | super::LocalEffectNode::TargetPlayerLife(_) => true,
        super::LocalEffectNode::PlayerAction(action) => {
            action.player != super::PlayerEffectSubject::Controller
        }
        super::LocalEffectNode::GainLife(life) => {
            life.player != super::LocalPlayerReference::Controller
        }
        super::LocalEffectNode::LoseLife(life) => {
            life.player != super::LocalPlayerReference::Controller
        }
        _ => false,
    }
}

/// Reads an action's operands with its acting player installed (the shared
/// binding-environment slot, `player_subject::with_acting_player`).
pub(in crate::oracle::canonical) fn read_with_acting_player<T>(
    cursor: &mut OracleTokenCursor<'_>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    super::source_sacrifice::atomic_action::player_subject::with_acting_player(cursor, read)
}
