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
//!           | ("was" | "were") "put" "there" "from" "the" "battlefield"
//!             "this" "turn"                                      zone history
//!           | Have "the" "same" Same "as" Anchor                  name relation
//!           | BlockHistory Anchor "this" "turn"                  block pairs
//!           | "dealt" ["combat"] "damage" "to" Anchor "this" "turn"
//!           | ["is" | "are"] "attacking" "you" ["or" ("a" "planeswalker"
//!             | "planeswalkers") "you" "control"]               attacked player
//! BlockHistory := "blocked" | ("was" | "were") "blocked" "by"
//!               | "blocked" "or" ("was" | "were") "blocked" "by"
//! Have     := "has" | "have" | ("doesn't" | "don't") "have"
//! Same     := "name" | "mana" "value"
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
//!
//! The same slot reads the object relations of the query, whose second
//! argument is another object (an anchor) rather than a player or a fact:
//!
//! ```text
//! Relation   := Participle Anchor                      reduced relative
//!             | Pronoun ("is" | "are") Participle Anchor
//!             | "that's" Participle Anchor
//!             | Pronoun Share
//!             | Pronoun "convoked" Anchor                CR 702.51c
//! Participle := "attached" "to"                        CR 301.5, 303.4
//!             | "blocking" ["or" "blocked" "by"]        CR 509.1
//!             | "blocked" "by"
//! Share      := ("share" | "shares") Quantity Kind "with" Anchor
//!             | ("doesn't" | "don't" | "does" "not" | "do" "not") "share"
//!               Quantity Kind "with" Anchor            negated
//! Quantity   := "a" | "an" | "at" "least" "one" | "any" | "no" (negated)
//! Kind       := "creature" Type | "card" Type | "permanent" Type
//!             | "land" Type | "color" | "colors"
//! Anchor     := "it" | "that" NOUN | "this" NOUN | NAME   a bound object
//!             | ["a" | "an"] QUERY                    some permanent of it
//! ```
//!
//! `blocking` only takes a bound object: `blocking creatures` is a status
//! adjective before its noun. A share clause without `with` (`creatures that
//! share a color`) relates the members of a set to each other; no anchor is
//! invented for it and the clause fails here.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DamagedPlayer, DepartureHistory, DeparturePlayer, LocalObjectReference, ObjectControllerScope,
    ObjectKind, ObjectPredicate, ObjectQueryContext, ObjectQueryNode, ObjectRelation, ObjectStatus,
    PermanentEffectReference, TargetedObjects, TargetedPermanents,
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
    // `was blocked this turn`; `was blocked by ANCHOR` is block history.
    if (word_at(cursor, offset, "was") || word_at(cursor, offset, "were"))
        && word_at(cursor, offset + 1, "blocked")
        && word_at(cursor, offset + 2, "this")
    {
        return Some((ObjectStatus::WasBlockedThisTurn, 2));
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

/// `that's` (straight or curly apostrophe) at the cursor.
fn contracted_relative_copula(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "that's") || word_at(cursor, 0, "that\u{2019}s")
}

/// `attacking you [or a planeswalker you control]` at `offset` (CR 508.1b,
/// 506.3): the attacked player relative to the ability controller and
/// whether the planeswalkers that player controls are attacked alternatives,
/// with the phrase width. `attacking` before a noun (`attacking creatures
/// you control`) is the status adjective and is not this relation.
fn attacking_player_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(DamagedPlayer, bool, usize)> {
    if !(word_at(cursor, offset, "attacking") && word_at(cursor, offset + 1, "you")) {
        return None;
    }
    // `you` must end the player: `you control` would be another clause.
    if ["control", "controls", "own", "cast", "don't", "do"]
        .iter()
        .any(|verb| word_at(cursor, offset + 2, verb))
    {
        return None;
    }
    let or = offset + 2;
    let relation = if word_at(cursor, or, "or")
        && word_at(cursor, or + 1, "a")
        && word_at(cursor, or + 2, "planeswalker")
    {
        Some(or + 3)
    } else if word_at(cursor, or, "or") && word_at(cursor, or + 1, "planeswalkers") {
        Some(or + 2)
    } else {
        None
    };
    match relation {
        Some(relation)
            if word_at(cursor, relation, "you") && word_at(cursor, relation + 1, "control") =>
        {
            Some((
                DamagedPlayer::AbilityController,
                true,
                relation + 2 - offset,
            ))
        }
        _ => Some((DamagedPlayer::AbilityController, false, 2)),
    }
}

/// `is|are attacking you` at `offset`: the copula of the attacked-player
/// relation.
fn copular_attack_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    (word_at(cursor, offset, "is") || word_at(cursor, offset, "are"))
        && attacking_player_at(cursor, offset + 1).is_some()
}

/// Reads `attacking you [or a planeswalker you control]` into the attacked
/// player relation of the candidate (CR 508.1b): the candidate is an
/// attacking creature whose defender is that player or, when stated, a
/// planeswalker that player controls. A creature attacking a planeswalker
/// is not attacking its controller (CR 506.3).
fn recognize_attacking_player(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relative-clause.attacking-player");
    let (player, planeswalkers, width) = cursor.require_value(
        attacking_player_at(cursor, 0),
        "`attacking you` or `attacking you or a planeswalker you control`",
    )?;
    consume_words(cursor, width)?;
    Ok(ObjectPredicate::Qualifier(
        super::ObjectQualifier::AttackingPlayer {
            player,
            planeswalkers,
        },
    ))
}

/// `dealt [combat] damage to ANCHOR this turn` at `offset` (CR 120.1,
/// 510.2): the damage relation of the candidate, as a source, to a definite
/// object, with the width up to the anchor. A player recipient is the
/// player relation (`dealt_damage_to_player_at`). Any other recipient is
/// claimed here, so a query anchor fails at the anchor reader: the damage
/// was dealt to the object that existed then, which no query names.
fn dealt_damage_to_object_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(&'static [ObjectRelationKind], usize)> {
    if !word_at(cursor, offset, "dealt") {
        return None;
    }
    let combat = word_at(cursor, offset + 1, "combat");
    let damage = offset + 1 + usize::from(combat);
    if !(word_at(cursor, damage, "damage") && word_at(cursor, damage + 1, "to")) {
        return None;
    }
    let anchor = damage + 2;
    if targeted_player_at(cursor, anchor).is_some()
        || dealt_damage_to_player_at(cursor, offset).is_some()
        || ["you", "player", "opponent", "players", "opponents"]
            .iter()
            .any(|word| word_at(cursor, anchor, word) || word_at(cursor, anchor + 1, word))
    {
        return None;
    }
    cursor.peek_ahead(anchor).is_some().then_some((
        if combat {
            DEALT_COMBAT_DAMAGE_TO_THIS_TURN
        } else {
            DEALT_DAMAGE_TO_THIS_TURN
        },
        anchor - offset,
    ))
}

/// The block-history relation at `offset` (CR 509.1, 509.1h): its relation
/// kinds and the width of the verb phrase up to its anchor.
///
/// ```text
/// BlockHistory := "blocked" Anchor                          the anchor attacked
///               | ("was" | "were") "blocked" "by" Anchor     the anchor blocked
///               | "blocked" "or" ("was" | "were") "blocked" "by" Anchor
/// ```
///
/// The clause closes with `this turn`: the pairs are read from the turn's
/// block declarations, so a pair whose creatures left combat still counts.
/// `blocked this turn` (no anchor) stays the status history, and the
/// coordination shares its one anchor (`that blocked or were blocked by
/// it`). Only a definite anchor is read: a block pair is a fact of two
/// object incarnations, and a query anchor would be evaluated on the
/// current battlefield instead of the blocks that happened.
fn block_history_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(&'static [ObjectRelationKind], usize)> {
    let passive_at = |at: usize| {
        (word_at(cursor, at, "was") || word_at(cursor, at, "were"))
            && word_at(cursor, at + 1, "blocked")
            && word_at(cursor, at + 2, "by")
    };
    if word_at(cursor, offset, "blocked")
        && word_at(cursor, offset + 1, "or")
        && passive_at(offset + 2)
    {
        return Some((BLOCKED_OR_BLOCKED_BY_THIS_TURN, 5));
    }
    if passive_at(offset) {
        return Some((BLOCKED_BY_THIS_TURN, 3));
    }
    if word_at(cursor, offset, "blocked")
        && !["by", "or", "and"]
            .iter()
            .any(|word| word_at(cursor, offset + 1, word))
        && !(word_at(cursor, offset + 1, "this") && word_at(cursor, offset + 2, "turn"))
        && cursor_after(cursor, offset + 1).is_some_and(|anchor| definite_anchor_starts(&anchor))
    {
        return Some((BLOCKED_THIS_TURN, 1));
    }
    None
}

/// Reads a block-history relation (`block_history_at`) with its definite
/// anchor and its `this turn` window.
fn recognize_block_history(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relative-clause.block-history");
    let (kinds, width) = cursor.require_value(
        block_history_at(cursor, 0),
        "`blocked ANCHOR` or `was blocked by ANCHOR`",
    )?;
    consume_words(cursor, width)?;
    let anchor = recognize_relation_anchor(cursor, true)?;
    cursor.enter("selector.relative-clause.block-history.period");
    cursor.consume_phrase(&["this", "turn"])?;
    let mut members = kinds
        .iter()
        .map(|kind| {
            ObjectPredicate::Relation(ObjectRelation::Related(RelatedObject {
                kind: *kind,
                anchor: anchor.clone(),
            }))
        })
        .collect::<Vec<_>>();
    Ok(if members.len() == 1 {
        members.remove(0)
    } else {
        ObjectPredicate::Union(members)
    })
}

/// `that targets PLAYER|a QUERY` after the stack-item selector of a declared
/// target (`target spell or ability that targets you or a creature you
/// control`, CR 115.1): the same targets relation the object query reads
/// for a spell, applied to every stack object of the selector. A pure
/// lookahead first; `None` when no such relative starts here, so the
/// selector keeps its own end.
pub(in crate::oracle::canonical) fn recognize_stack_item_targets_relative(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<TargetedObjects>> {
    if !(is_relative_pronoun(cursor) && targets_clause_starts(cursor, 1)) {
        return Ok(None);
    }
    cursor.enter("selector.stack-item.targets-relative");
    consume_words(cursor, 1)?;
    match recognize_targets_clause(cursor)? {
        ObjectPredicate::Relation(ObjectRelation::Targets(targeted)) => Ok(Some(targeted)),
        _ => Err(cursor.fail("the targets relation of a stack object")),
    }
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

/// `controlled by different players` at `offset`, with its width: a reduced
/// relative over a plural chosen set (CR 108.4). It constrains the set, not
/// one candidate; only a consumer choosing a plural set admits it.
fn distinct_controllers_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    let phrase = ["controlled", "by", "different", "players"];
    phrase
        .iter()
        .enumerate()
        .all(|(index, expected)| word_at(cursor, offset + index, expected))
        .then_some(phrase.len())
}

/// True when a relative clause of every query starts at the cursor.
pub(in crate::oracle::canonical) fn relative_clause_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if participial_relation_starts(cursor)
        || contracted_copular_participle(cursor)
        || distinct_controllers_at(cursor, 0).is_some()
    {
        return true;
    }
    if contracted_relative_copula(cursor) && attacking_player_at(cursor, 1).is_some() {
        return true;
    }
    if is_relative_pronoun(cursor) && copular_attack_at(cursor, 1) {
        return true;
    }
    if is_relative_pronoun(cursor)
        && (copular_participle_at(cursor, 1).is_some()
            || share_verb_at(cursor, 1).is_some()
            || word_at(cursor, 1, "convoked"))
    {
        return true;
    }
    is_relative_pronoun(cursor)
        && (word_at(cursor, 1, "has") && super::counter_qualifier_starts(cursor, 2)
            || word_at(cursor, 1, "have") && super::counter_qualifier_starts(cursor, 2)
            || ["is", "are", "isn't", "aren't"]
                .iter()
                .any(|verb| word_at(cursor, 1, verb))
                && status_after_copula(cursor, 2)
            || block_history_at(cursor, 1).is_some()
            || dealt_damage_to_object_at(cursor, 1).is_some()
            || past_fact_at(cursor, 1).is_some()
            || dealt_damage_to_player_at(cursor, 1).is_some()
            || negated_past_at(cursor, 1).is_some()
            || zone_arrival_width_at(cursor, 1).is_some()
            || same_characteristic_possession_at(cursor, 1).is_some())
}

/// The zone-arrival history of a card in the zone its parent named, at
/// `offset`, with its width (CR 400.7, 603.10a):
///
/// ```text
/// Arrival := ("was" | "were") ( "put" "there" "from" "the" "battlefield"
///                             | "put" "there" | "milled" ) "this" "turn"
/// ```
///
/// `there` names the parent's zone; a clause naming another zone (`put
/// into a graveyard`) is a departure, read by `departure_clause_at`.
fn zone_arrival_history_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(ObjectStatus, usize)> {
    if !(word_at(cursor, offset, "was") || word_at(cursor, offset, "were")) {
        return None;
    }
    let phrase_at = |phrase: &[&str]| {
        phrase
            .iter()
            .enumerate()
            .all(|(index, expected)| word_at(cursor, offset + 1 + index, expected))
            .then_some(1 + phrase.len())
    };
    const FORMS: [(&[&str], ObjectStatus); 3] = [
        (
            &["put", "there", "from", "the", "battlefield", "this", "turn"],
            ObjectStatus::PutThereFromBattlefieldThisTurn,
        ),
        (
            &["put", "there", "this", "turn"],
            ObjectStatus::PutThereThisTurn,
        ),
        (&["milled", "this", "turn"], ObjectStatus::MilledThisTurn),
    ];
    FORMS
        .iter()
        .find_map(|(phrase, status)| phrase_at(*phrase).map(|width| (*status, width)))
}

/// `was|were put there from the battlefield this turn` (and the other
/// zone-arrival histories) at `offset`, with its width.
fn zone_arrival_width_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    zone_arrival_history_at(cursor, offset).map(|(_, width)| width)
}

/// A relative clause after a card zone (`cards in your graveyard that were
/// put there from the battlefield this turn`, `cards in your hand that have
/// the same name as a creature you control`): the clauses this slot reads
/// whose facts a card in a zone has. A pure lookahead; the zone parent
/// composes the read predicate with its own.
pub(in crate::oracle::canonical) fn card_zone_relative_clause_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    is_relative_pronoun(cursor)
        && (zone_arrival_width_at(cursor, 1).is_some()
            || same_characteristic_possession_at(cursor, 1).is_some())
}

/// `that was|were put there from the battlefield this turn` after a zone a
/// declared card target is in: the one zone-history clause whose atom every
/// card-candidate matcher evaluates on the candidate instance (CR 400.7,
/// 603.10a). A pure lookahead.
pub(in crate::oracle::canonical) fn card_zone_history_clause_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    is_relative_pronoun(cursor) && zone_arrival_width_at(cursor, 1).is_some()
}

/// `the same name as` / `the same mana value as` at `offset`: the compared
/// characteristic and the phrase width up to the anchor (CR 201.2, 202.3).
pub(in crate::oracle::canonical) fn same_characteristic_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(SharedCharacteristic, usize)> {
    if !(word_at(cursor, offset, "the") && word_at(cursor, offset + 1, "same")) {
        return None;
    }
    let (characteristic, width) = if word_at(cursor, offset + 2, "name") {
        (SharedCharacteristic::Name, 1)
    } else if word_at(cursor, offset + 2, "mana") && word_at(cursor, offset + 3, "value") {
        (SharedCharacteristic::ManaValue, 2)
    } else {
        return None;
    };
    word_at(cursor, offset + 2 + width, "as").then_some((characteristic, 3 + width))
}

/// `has|have the same NAME as` and its negation `doesn't|don't have the same
/// NAME as` at `offset`: the possession verb's width and its polarity.
fn same_characteristic_possession_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(usize, bool)> {
    let (width, negated) = if word_at(cursor, offset, "has") || word_at(cursor, offset, "have") {
        (1, false)
    } else if ["doesn't", "don't", "doesn\u{2019}t", "don\u{2019}t"]
        .iter()
        .any(|form| word_at(cursor, offset, form))
        && word_at(cursor, offset + 1, "have")
    {
        (2, true)
    } else {
        return None;
    };
    same_characteristic_at(cursor, offset + width).map(|_| (width, negated))
}

/// `the same name as ANCHOR` (the caller read `with` or `has`): the object
/// and the anchor have a value of that characteristic in common, the shared
/// relation of the query (CR 201.2: a name; an object without a name shares
/// none). The anchor is a bound object or a battlefield query, as for every
/// relation of this slot.
pub(in crate::oracle::canonical) fn recognize_same_characteristic_relation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relation.same-characteristic");
    let (characteristic, width) = cursor.require_value(
        same_characteristic_at(cursor, 0),
        "`the same name as` or `the same mana value as`",
    )?;
    consume_words(cursor, width)?;
    let anchor = recognize_relation_anchor(cursor, false)?;
    Ok(ObjectPredicate::Relation(ObjectRelation::Related(
        RelatedObject {
            kind: ObjectRelationKind::Shares(characteristic),
            anchor,
        },
    )))
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
    if let Some(width) = distinct_controllers_at(cursor, 0) {
        // `two target creatures controlled by different players`: a
        // constraint over the chosen set, which the consumer lifts to its
        // selection constraint (CR 601.2c).
        cursor.enter("selector.relative-clause.distinct-controllers");
        consume_words(cursor, width)?;
        return Ok(ObjectPredicate::Qualifier(
            super::ObjectQualifier::DistinctControllers,
        ));
    }
    if attacking_player_at(cursor, 0).is_some() {
        // `each creature attacking you`: the reduced relative of the
        // attacked player (CR 508.1b).
        return recognize_attacking_player(cursor);
    }
    if participial_relation_at(cursor, 0).is_some() {
        // `Equipment attached to that creature`, `each creature blocking
        // it`: a reduced relative clause, the participle with no pronoun.
        return recognize_participial_relation(cursor);
    }
    if contracted_relative_copula(cursor) && attacking_player_at(cursor, 1).is_some() {
        // `target creature without flying that's attacking you`.
        cursor.enter("selector.relative-clause.copular-attack");
        consume_words(cursor, 1)?;
        return recognize_attacking_player(cursor);
    }
    if contracted_copular_participle(cursor) {
        // `each Aura you control that's attached to a creature`.
        cursor.enter("selector.relative-clause.copular-relation");
        consume_words(cursor, 1)?;
        return recognize_participial_relation(cursor);
    }
    cursor.enter("selector.relative-clause.pronoun");
    let pronoun = cursor.peek().unwrap_or_default().to_string();
    cursor.require(is_relative_pronoun(cursor), "`that`, `which` or `who`")?;
    cursor.consume(&pronoun)?;
    recognize_relative_predicate(cursor)
}

/// True at a verb phrase `recognize_relative_predicate` reads without its
/// pronoun after another subject (`if it shares a creature type with this
/// creature`, `as long as it entered this turn`, `as long as this Equipment
/// is attached to a creature`). Status copulas and possession stay with the
/// subject's own reader. A pure lookahead.
pub(in crate::oracle::canonical) fn relative_predicate_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    copular_participle_at(cursor, 0).is_some()
        || share_verb_at(cursor, 0).is_some()
        || block_history_at(cursor, 0).is_some()
        || copular_attack_at(cursor, 0)
        || dealt_damage_to_object_at(cursor, 0).is_some()
        || past_fact_at(cursor, 0).is_some()
        || negated_past_at(cursor, 0).is_some()
        || dealt_damage_to_player_at(cursor, 0).is_some()
        || targets_clause_starts(cursor, 0)
        || zone_arrival_width_at(cursor, 0).is_some()
        || same_characteristic_possession_at(cursor, 0).is_some()
}

/// The verb phrase of a relative clause after its pronoun (or after the
/// subject of a condition, `relative_predicate_starts`): one grammar for the
/// object predicate it states.
pub(in crate::oracle::canonical) fn recognize_relative_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    if let Some(copula_width) = copular_participle_at(cursor, 0) {
        // `that are attached to it`: the present copula before the same
        // participle; the relation holds now (CR 301.5, 303.4).
        cursor.enter("selector.relative-clause.copular-relation");
        for _ in 0..copula_width {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
        }
        return recognize_participial_relation(cursor);
    }
    if share_verb_at(cursor, 0).is_some() {
        return recognize_share_clause(cursor);
    }
    if word_at(cursor, 0, "convoked") {
        // `each creature that convoked it|this spell` (CR 702.51c): the
        // creatures tapped to pay the anchor's convoke cost.
        cursor.enter("selector.relative-clause.convoked");
        consume_words(cursor, 1)?;
        let anchor = recognize_relation_anchor(cursor, true)?;
        return Ok(ObjectPredicate::Relation(ObjectRelation::Related(
            RelatedObject {
                kind: ObjectRelationKind::Convoked,
                anchor,
            },
        )));
    }
    if let Some((width, negated)) = same_characteristic_possession_at(cursor, 0) {
        // `that has the same name as a creature you control`, `that doesn't
        // have the same name as another permanent you control`.
        cursor.enter("selector.relative-clause.same-characteristic");
        consume_words(cursor, width)?;
        let relation = recognize_same_characteristic_relation(cursor)?;
        return Ok(if negated {
            ObjectPredicate::Not(Box::new(relation))
        } else {
            relation
        });
    }
    if copular_attack_at(cursor, 0) {
        // `that is attacking you`: the present copula before the attacked
        // player relation.
        cursor.enter("selector.relative-clause.copular-attack");
        consume_words(cursor, 1)?;
        return recognize_attacking_player(cursor);
    }
    if let Some((kinds, width)) = dealt_damage_to_object_at(cursor, 0) {
        // `each Vampire you control that dealt damage to that creature this
        // turn`: the damage the candidate dealt to a definite object.
        cursor.enter("selector.relative-clause.damage-to-object");
        consume_words(cursor, width)?;
        let anchor = recognize_relation_anchor(cursor, true)?;
        cursor.enter("selector.relative-clause.damage-to-object.period");
        cursor.consume_phrase(&["this", "turn"])?;
        return Ok(ObjectPredicate::Relation(ObjectRelation::Related(
            RelatedObject {
                kind: kinds[0],
                anchor,
            },
        )));
    }
    if block_history_at(cursor, 0).is_some() {
        // `that blocked or were blocked by it this turn`, `that were blocked
        // by that creature this turn`: block pairs of this turn with a
        // definite anchor, read before the anchorless `blocked` status.
        return recognize_block_history(cursor);
    }
    if let Some((history, width)) = zone_arrival_history_at(cursor, 0) {
        // `cards in your graveyard that were put there from the
        // battlefield this turn`, `target creature card from a graveyard
        // that was put there this turn`, `that was milled this turn` (CR
        // 400.7, 603.10a).
        cursor.enter("selector.relative-clause.zone-history");
        consume_words(cursor, width)?;
        return Ok(ObjectPredicate::Status(history));
    }
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
    if word_at(cursor, 0, "under") {
        // `that entered the battlefield under your control this turn` (CR
        // 110.2a): the player who controlled the permanent as it entered,
        // which a later control change does not alter. Only one positive
        // entry fact takes the adjunct.
        cursor.enter("selector.relative-clause.entry-controller");
        cursor.require(
            matches!(
                members.as_slice(),
                [ObjectPredicate::Status(ObjectStatus::EnteredThisTurn)]
            ),
            "one positive `entered` fact before `under PLAYER's control`",
        )?;
        let (player, width) = cursor.require_value(
            entry_controller_at(cursor, 1),
            "`your control` or `an opponent's control` after `under`",
        )?;
        consume_words(cursor, 1 + width)?;
        members = vec![ObjectPredicate::Qualifier(
            super::ObjectQualifier::EnteredUnderControlThisTurn(player),
        )];
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

/// `your control` / `an opponent's control` at `offset`: the entering
/// permanent's controller relative to the ability controller, with the
/// phrase width (the possessor and `control`).
fn entry_controller_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(DamagedPlayer, usize)> {
    let (player, width) = match possessive_player_at(cursor, offset)? {
        (DeparturePlayer::AbilityController, width) => (DamagedPlayer::AbilityController, width),
        (DeparturePlayer::OpponentOfAbilityController, width) => {
            (DamagedPlayer::OpponentOfAbilityController, width)
        }
    };
    word_at(cursor, offset + width, "control").then_some((player, width + 1))
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

/// The object a relation of the shared query relates each candidate to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RelationAnchor {
    /// One bound object (or one bound set): the source, a declared target,
    /// the event's object. The binding environment resolved it; nothing is
    /// guessed.
    Object(PermanentEffectReference),
    /// Some battlefield permanent of a query (`attached to a creature you
    /// control`): the relation holds with at least one of them.
    Query(Box<ObjectQueryNode>),
    /// A definite description's recorded or linked object (`the sacrificed
    /// creature`, `the exiled card`): `definite_object_description.rs`.
    Described(super::DescribedObject),
}

/// The characteristic a `share` clause compares (CR 105.1, 205.2, 205.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SharedCharacteristic {
    CreatureType,
    CardType,
    /// The permanent card types (artifact, battle, creature, enchantment,
    /// land, planeswalker; CR 110.4).
    PermanentType,
    LandType,
    Color,
    /// The name (CR 201.2): `with the same name as`.
    Name,
    /// The mana value (CR 202.3): `with the same mana value as`.
    ManaValue,
}

impl SharedCharacteristic {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::CreatureType => "creatureType",
            Self::CardType => "cardType",
            Self::PermanentType => "permanentType",
            Self::LandType => "landType",
            Self::Color => "color",
            Self::Name => "name",
            Self::ManaValue => "manaValue",
        }
    }
}

/// How a candidate relates to the anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectRelationKind {
    /// The candidate is attached to the anchor (CR 301.5, 303.4).
    AttachedTo,
    /// The candidate is a creature blocking the anchor (CR 509.1).
    Blocking,
    /// The candidate is an attacking creature the anchor blocks (CR 509.1).
    BlockedBy,
    /// The candidate and the anchor have at least one value of the
    /// characteristic in common.
    Shares(SharedCharacteristic),
    /// The candidate is a creature tapped to pay the convoke cost of the
    /// anchor when it was cast (CR 702.51a, 702.51c).
    Convoked,
    /// The candidate blocked the anchor this turn (CR 509.1): a block pair
    /// of this turn's declarations whose blocker is this incarnation of the
    /// candidate and whose attacker is the anchor.
    BlockedThisTurn,
    /// The anchor blocked the candidate this turn (CR 509.1, 509.1h).
    BlockedByThisTurn,
    /// The candidate dealt damage to the anchor this turn (CR 120.1).
    DealtDamageToThisTurn,
    /// The candidate dealt combat damage to the anchor this turn (CR 510.2).
    DealtCombatDamageToThisTurn,
}

impl ObjectRelationKind {
    pub(in crate::oracle::canonical) const fn ir_kind(self) -> &'static str {
        match self {
            Self::AttachedTo => "attachedTo",
            Self::Blocking => "blocking",
            Self::BlockedBy => "blockedBy",
            Self::Shares(_) => "sharesCharacteristic",
            Self::Convoked => "convoked",
            Self::BlockedThisTurn => "blockedAnchorThisTurn",
            Self::BlockedByThisTurn => "blockedByAnchorThisTurn",
            Self::DealtDamageToThisTurn => "dealtDamageToAnchorThisTurn",
            Self::DealtCombatDamageToThisTurn => "dealtCombatDamageToAnchorThisTurn",
        }
    }
}

/// One relation of the candidate to another object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RelatedObject {
    pub kind: ObjectRelationKind,
    pub anchor: RelationAnchor,
}

impl RelatedObject {
    /// The domains whose candidates the relation can be tested on: an
    /// attachment and a block are battlefield facts; a characteristic is
    /// shared by permanents and by cards (CR 109.3).
    pub(in crate::oracle::canonical) fn supports_domain(&self, domain: ObjectKind) -> bool {
        match self.kind {
            ObjectRelationKind::AttachedTo
            | ObjectRelationKind::Blocking
            | ObjectRelationKind::BlockedBy
            | ObjectRelationKind::Convoked
            | ObjectRelationKind::BlockedThisTurn
            | ObjectRelationKind::BlockedByThisTurn
            | ObjectRelationKind::DealtDamageToThisTurn
            | ObjectRelationKind::DealtCombatDamageToThisTurn => domain == ObjectKind::Permanent,
            // A spell has the characteristics of its card (CR 112.1).
            ObjectRelationKind::Shares(_) => {
                matches!(
                    domain,
                    ObjectKind::Permanent | ObjectKind::Card | ObjectKind::Spell
                )
            }
        }
    }
}

const ATTACHED_TO: &[ObjectRelationKind] = &[ObjectRelationKind::AttachedTo];
const BLOCKING: &[ObjectRelationKind] = &[ObjectRelationKind::Blocking];
const BLOCKED_BY: &[ObjectRelationKind] = &[ObjectRelationKind::BlockedBy];
const BLOCKING_OR_BLOCKED_BY: &[ObjectRelationKind] =
    &[ObjectRelationKind::Blocking, ObjectRelationKind::BlockedBy];
const BLOCKED_THIS_TURN: &[ObjectRelationKind] = &[ObjectRelationKind::BlockedThisTurn];
const BLOCKED_BY_THIS_TURN: &[ObjectRelationKind] = &[ObjectRelationKind::BlockedByThisTurn];
const DEALT_DAMAGE_TO_THIS_TURN: &[ObjectRelationKind] =
    &[ObjectRelationKind::DealtDamageToThisTurn];
const DEALT_COMBAT_DAMAGE_TO_THIS_TURN: &[ObjectRelationKind] =
    &[ObjectRelationKind::DealtCombatDamageToThisTurn];
const BLOCKED_OR_BLOCKED_BY_THIS_TURN: &[ObjectRelationKind] = &[
    ObjectRelationKind::BlockedThisTurn,
    ObjectRelationKind::BlockedByThisTurn,
];

/// The participle of a reduced relative clause at `offset`: its relation
/// kinds (two for `blocking or blocked by`) and its token width.
fn participial_relation_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(&'static [ObjectRelationKind], usize)> {
    if word_at(cursor, offset, "attached") && word_at(cursor, offset + 1, "to") {
        return Some((ATTACHED_TO, 2));
    }
    if word_at(cursor, offset, "blocking")
        && word_at(cursor, offset + 1, "or")
        && word_at(cursor, offset + 2, "blocked")
        && word_at(cursor, offset + 3, "by")
    {
        return Some((BLOCKING_OR_BLOCKED_BY, 4));
    }
    if word_at(cursor, offset, "blocking") {
        return Some((BLOCKING, 1));
    }
    if word_at(cursor, offset, "blocked") && word_at(cursor, offset + 1, "by") {
        return Some((BLOCKED_BY, 2));
    }
    None
}

/// Whether `kinds` take a bound object only. `blocking creatures` is the
/// status adjective before its noun; only a definite object after
/// `blocking` makes it a participle.
fn definite_anchor_only(kinds: &[ObjectRelationKind]) -> bool {
    matches!(kinds, [ObjectRelationKind::Blocking])
}

/// A cursor advanced `offset` tokens, for a lookahead of the anchor.
fn cursor_after<'a>(
    cursor: &OracleTokenCursor<'a>,
    offset: usize,
) -> Option<OracleTokenCursor<'a>> {
    let mut trial = cursor.clone();
    trial.advance_to(cursor.position() + offset).ok()?;
    Some(trial)
}

/// Lexical lookahead of a bound-object anchor: `it`, `that NOUN`, `this
/// NOUN` or the source's name. It proves no binding.
fn definite_anchor_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if super::bound_object_reference_starts(cursor) {
        return true;
    }
    let names = cursor.source_names().to_vec();
    let mut trial = cursor.clone();
    super::recognize_source_reference(&mut trial, &names, None).is_ok()
}

/// True when a participial relation with a readable anchor starts at the
/// cursor: `attached to ...`, `blocking it`, `blocked by ...`. A pure
/// lookahead; the shared atom reader uses it so that the participle is not
/// read as a status adjective of the noun before it.
pub(in crate::oracle::canonical) fn participial_relation_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if attacking_player_at(cursor, 0).is_some() {
        return true;
    }
    let Some((kinds, width)) = participial_relation_at(cursor, 0) else {
        return false;
    };
    if !definite_anchor_only(kinds) {
        return true;
    }
    cursor_after(cursor, width).is_some_and(|anchor| definite_anchor_starts(&anchor))
}

/// `is` / `are` before a participle (`that are attached to it`), with the
/// copula's width. The past `were attached` is a last-known relation that
/// no anchor here records, and is not claimed.
fn copular_participle_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    if !(word_at(cursor, offset, "is") || word_at(cursor, offset, "are")) {
        return None;
    }
    let (kinds, width) = participial_relation_at(cursor, offset + 1)?;
    if definite_anchor_only(kinds) {
        let anchor = cursor_after(cursor, offset + 1 + width)?;
        if !definite_anchor_starts(&anchor) {
            return None;
        }
    }
    Some(1)
}

/// `that's` before a participle (`that's attached to a creature`): the
/// contracted present copula of the same relation.
fn contracted_copular_participle(cursor: &OracleTokenCursor<'_>) -> bool {
    if !(word_at(cursor, 0, "that's") || word_at(cursor, 0, "that\u{2019}s")) {
        return false;
    }
    let Some((kinds, width)) = participial_relation_at(cursor, 1) else {
        return false;
    };
    !definite_anchor_only(kinds)
        || cursor_after(cursor, 1 + width).is_some_and(|anchor| definite_anchor_starts(&anchor))
}

/// `share` / `shares` / `doesn't share` / `do not share` at `offset`: the
/// verb's width and whether it is negated.
fn share_verb_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(usize, bool)> {
    if word_at(cursor, offset, "share") || word_at(cursor, offset, "shares") {
        return Some((1, false));
    }
    let contracted = ["doesn't", "don't", "doesn\u{2019}t", "don\u{2019}t"]
        .iter()
        .any(|form| word_at(cursor, offset, form));
    if contracted && word_at(cursor, offset + 1, "share") {
        return Some((2, true));
    }
    if (word_at(cursor, offset, "does") || word_at(cursor, offset, "do"))
        && word_at(cursor, offset + 1, "not")
        && word_at(cursor, offset + 2, "share")
    {
        return Some((3, true));
    }
    None
}

fn consume_words(cursor: &mut OracleTokenCursor<'_>, count: usize) -> MachineResult<()> {
    for _ in 0..count {
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
    }
    Ok(())
}

/// Reads the participle and its anchor into the relation predicate.
/// `blocking or blocked by X` is the union of both relations with the same
/// anchor.
fn recognize_participial_relation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relation.participle");
    let (kinds, width) = cursor.require_value(
        participial_relation_at(cursor, 0),
        "`attached to`, `blocking` or `blocked by`",
    )?;
    consume_words(cursor, width)?;
    let anchor = recognize_relation_anchor(cursor, definite_anchor_only(kinds))?;
    let mut members = kinds
        .iter()
        .map(|kind| {
            ObjectPredicate::Relation(ObjectRelation::Related(RelatedObject {
                kind: *kind,
                anchor: anchor.clone(),
            }))
        })
        .collect::<Vec<_>>();
    Ok(if members.len() == 1 {
        members.remove(0)
    } else {
        ObjectPredicate::Union(members)
    })
}

/// The characteristic after the share quantity, with its width.
fn shared_characteristic_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(SharedCharacteristic, usize)> {
    let type_noun = |at: usize| word_at(cursor, at, "type") || word_at(cursor, at, "types");
    let typed = [
        ("creature", SharedCharacteristic::CreatureType),
        ("card", SharedCharacteristic::CardType),
        ("permanent", SharedCharacteristic::PermanentType),
        ("land", SharedCharacteristic::LandType),
    ];
    for (word, characteristic) in typed {
        if word_at(cursor, offset, word) && type_noun(offset + 1) {
            return Some((characteristic, 2));
        }
    }
    (word_at(cursor, offset, "color") || word_at(cursor, offset, "colors"))
        .then_some((SharedCharacteristic::Color, 1))
}

/// The share quantity at `offset` (`a`, `an`, `any`, `at least one`, `no`):
/// its width and whether it negates the share.
fn share_quantity_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(usize, bool)> {
    if word_at(cursor, offset, "at")
        && word_at(cursor, offset + 1, "least")
        && word_at(cursor, offset + 2, "one")
    {
        return Some((3, false));
    }
    if ["a", "an", "any"]
        .iter()
        .any(|word| word_at(cursor, offset, word))
    {
        return Some((1, false));
    }
    word_at(cursor, offset, "no").then_some((1, true))
}

/// Reads `share(s) <quantity> <characteristic>`, the head every share clause
/// has, into the characteristic and whether the share is negated.
fn recognize_share_head(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(SharedCharacteristic, bool)> {
    let (width, mut negated) = cursor.require_value(
        share_verb_at(cursor, 0),
        "`share`, `shares` or `doesn't share`",
    )?;
    consume_words(cursor, width)?;
    cursor.enter("selector.relative-clause.share.quantity");
    let (quantity_width, quantity_negates) = cursor.require_value(
        share_quantity_at(cursor, 0),
        "`a`, `an`, `at least one`, `any` or `no` before the shared characteristic",
    )?;
    if quantity_negates {
        cursor.require(!negated, "one negation of a share clause")?;
        negated = true;
    }
    consume_words(cursor, quantity_width)?;
    cursor.enter("selector.relative-clause.share.characteristic");
    let (characteristic, characteristic_width) = cursor.require_value(
        shared_characteristic_at(cursor, 0),
        "a creature type, card type, permanent type, land type or color",
    )?;
    consume_words(cursor, characteristic_width)?;
    Ok((characteristic, negated))
}

/// `[that] share(s) a creature type with it`, `that shares at least one
/// color with the exiled card`, `that share no creature types with X`,
/// `that doesn't share a color with X` (CR 105.1, 205.2, 205.3). The anchor
/// is mandatory: a share among the members of a set has no anchor and is
/// read by `recognize_shared_characteristic_set_clause`.
fn recognize_share_clause(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectPredicate> {
    cursor.enter("selector.relative-clause.share");
    let (characteristic, negated) = recognize_share_head(cursor)?;
    cursor.enter("selector.relative-clause.share.anchor");
    // `creatures that share a creature type` compares the members of the
    // set with each other: no anchor is invented for it here.
    cursor.consume("with")?;
    let anchor = recognize_relation_anchor(cursor, false)?;
    let relation = ObjectPredicate::Relation(ObjectRelation::Related(RelatedObject {
        kind: ObjectRelationKind::Shares(characteristic),
        anchor,
    }));
    Ok(if negated {
        ObjectPredicate::Not(Box::new(relation))
    } else {
        relation
    })
}

/// Whether `that share(s) ...` starts a share clause over the members of a
/// plural set (`two land cards that share a land type`, CR 205.3i), with no
/// `with ANCHOR`. Lexical lookahead over the same verb, quantity and
/// characteristic readers as the anchored clause; an anchored share is not
/// a set clause.
pub(in crate::oracle::canonical) fn shared_characteristic_set_clause_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if !word_at(cursor, 0, "that") {
        return false;
    }
    let Some((verb, _)) = share_verb_at(cursor, 1) else {
        return false;
    };
    let Some((quantity, _)) = share_quantity_at(cursor, 1 + verb) else {
        return false;
    };
    let Some((_, characteristic)) = shared_characteristic_at(cursor, 1 + verb + quantity) else {
        return false;
    };
    !word_at(cursor, 1 + verb + quantity + characteristic, "with")
}

/// Reads `that share(s) <quantity> <characteristic>` over the members of a
/// set into the shared characteristic. A negated set share (`that share no
/// creature types`) is a different constraint (pairwise distinct) and is
/// refused, as is an anchor after it.
pub(in crate::oracle::canonical) fn recognize_shared_characteristic_set_clause(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SharedCharacteristic> {
    cursor.enter("selector.relative-clause.share-set");
    cursor.consume("that")?;
    let (characteristic, negated) = recognize_share_head(cursor)?;
    cursor.enter("selector.relative-clause.share-set.polarity");
    cursor.require(!negated, "a positive share among the members of the set")?;
    cursor.enter("selector.relative-clause.share-set.anchorless");
    cursor.require(
        !word_at(cursor, 0, "with"),
        "a share among the set's members, without an anchor",
    )?;
    Ok(characteristic)
}

/// Whether the lowering has a value for this bound anchor: a declared
/// target is resolved by the program lowering against its declaration; any
/// other reference needs no declaration.
fn anchor_reference_lowerable(reference: PermanentEffectReference) -> bool {
    matches!(
        reference,
        PermanentEffectReference::Target(_) | PermanentEffectReference::TargetSet(_)
    ) || crate::oracle::canonical::effects::bound_permanent_reference_value(reference, &[])
        .is_some()
}

/// Reads the anchor of a relation. A bound object is resolved through the
/// installed binding environment; an absent antecedent is a committed
/// diagnostic. Otherwise, unless the relation takes a bound object only,
/// the anchor is a battlefield query (`attached to a creature`).
pub(in crate::oracle::canonical) fn recognize_relation_anchor(
    cursor: &mut OracleTokenCursor<'_>,
    definite_only: bool,
) -> MachineResult<RelationAnchor> {
    cursor.enter("selector.relation.anchor");
    if let Some(anchor) = super::recognize_described_relation_anchor(cursor)? {
        return Ok(anchor);
    }
    if super::bound_object_reference_starts(cursor) {
        let environment = super::installed_binding_environment(cursor);
        // `X gets +1/+1 for each Aura attached to it`: without a lexical
        // antecedent, the pronoun names the subject of the clause being read,
        // which its parent installed (`with_clause_subject`).
        if environment.object.is_none()
            && let Some(subject) = environment.subject
            && cursor.peek().is_some_and(super::is_source_anaphor)
            && anchor_reference_lowerable(subject)
        {
            consume_words(cursor, 1)?;
            return Ok(RelationAnchor::Object(subject));
        }
        // A definite-only relation is a history of the anchor's identity
        // (`that dealt damage to that creature this turn`, CR 608.2h): the
        // anchor may name an event object that already left the battlefield.
        let reference = if definite_only
            && !cursor.peek().is_some_and(super::is_source_anaphor)
            && cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        {
            super::recognize_nominal_object_read(cursor, &environment)?
        } else {
            super::recognize_bound_object_reference(cursor, &environment)?
        };
        cursor.enter("selector.relation.anchor.binding");
        cursor.require(
            anchor_reference_lowerable(reference),
            "a bound anchor object the relation can name at resolution",
        )?;
        return Ok(RelationAnchor::Object(reference));
    }
    let names = cursor.source_names().to_vec();
    let mut trial = cursor.clone();
    if super::recognize_source_reference(&mut trial, &names, None).is_ok() {
        *cursor = trial;
        return Ok(RelationAnchor::Object(PermanentEffectReference::Source(
            LocalObjectReference::SourceSelf,
        )));
    }
    cursor.enter("selector.relation.anchor.query");
    cursor.require(
        !definite_only,
        "a bound object (`it`, `that creature`, `this creature`) after the participle",
    )?;
    if word_at(cursor, 0, "a") || word_at(cursor, 0, "an") {
        consume_words(cursor, 1)?;
    }
    let query = super::recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)?
        .into_object_query(cursor)?;
    cursor.enter("selector.relation.anchor.query-domain");
    cursor.require(
        query.denotes_battlefield_permanents() && !query.exclude_source,
        "an anchor query of battlefield permanents",
    )?;
    cursor.require(
        !matches!(query.controller, Some(ObjectControllerScope::Player(_))),
        "an anchor query whose controller is relative to the ability controller",
    )?;
    Ok(RelationAnchor::Query(Box::new(query)))
}

/// Whether a relation of `predicate` is anchored on a bound object other
/// than the source (`that shares a color with it`, `it` an event object).
/// `other` before such a collection names that anchor, not the source
/// (`destroy all other creatures that share a color with it`), which no
/// source exclusion represents: the collection parent refuses it.
pub(in crate::oracle::canonical) fn relates_to_bound_non_source(
    predicate: &ObjectPredicate,
) -> bool {
    match predicate {
        ObjectPredicate::Relation(ObjectRelation::Related(related)) => matches!(
            related.anchor,
            RelationAnchor::Object(reference)
                if !matches!(reference, PermanentEffectReference::Source(_))
        ),
        ObjectPredicate::Not(operand) => relates_to_bound_non_source(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().any(relates_to_bound_non_source)
        }
        _ => false,
    }
}

/// Refuses `other` before a collection whose relation anchor is another
/// bound object (see `relates_to_bound_non_source`).
pub(in crate::oracle::canonical) fn refuse_other_than_relation_anchor(
    cursor: &mut OracleTokenCursor<'_>,
    predicate: &ObjectPredicate,
    exclude_source: bool,
) -> MachineResult<()> {
    if exclude_source && relates_to_bound_non_source(predicate) {
        cursor.enter("selector.relation.other-antecedent");
        return Err(cursor.fail(
            "an `other` collection whose antecedent is the source, not the relation's anchor",
        ));
    }
    Ok(())
}
