//! Static player permissions share one frame: actor, permitted actions,
//! origin, base-cost policy and timing manner are independent children.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    CardZoneName, CollectionOperandAttempt, NumericExpression, ObjectKind, ObjectPredicate,
    PlayerSetReferenceNode, SpellCastQueryNode, SpellCasterScope, recognize_card_zone_atom,
    recognize_object_collection, recognize_ordinal_atom, recognize_player_set_reference,
    recognize_spell_cast_collection, try_recognize_card_library_top,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastingPermissionActorNode {
    Controller,
    Set(PlayerSetReferenceNode),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastingBaseCostPolicyNode {
    Printed,
    WithoutManaCost,
}

/// Which matching spells a cast permission covers: each of them, or only the
/// first one the actor casts in each turn (a spell-history ordinal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermittedSpellScopeNode {
    Each,
    FirstEachTurn,
}

/// One permitted action. `cast` reads the shared spell query; `play` reads
/// the shared object query in the card domain (CR 305.1 lands, 601 spells).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermittedActionNode {
    Cast {
        spells: SpellCastQueryNode,
        scope: PermittedSpellScopeNode,
    },
    Play {
        cards: ObjectPredicate,
    },
}

/// Where the permitted objects are. Without `from`, spells are cast from
/// their usual zone, the hand (CR 601.2a, 601.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermissionOriginNode {
    UsualZone,
    Zone(CardZoneName),
    /// `from the top of your library`: only the top card (CR 401.2).
    LibraryTop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastingPermissionNode {
    pub actor: CastingPermissionActorNode,
    pub actions: Vec<PermittedActionNode>,
    pub origin: PermissionOriginNode,
    pub base_cost: CastingBaseCostPolicyNode,
    /// `as though it/they had flash` (CR 702.8a timing for these spells).
    pub as_though_flash: bool,
}

/// A static permission is either an action allowance or the hidden-zone
/// look allowance `look at the top card of your library any time`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerPermissionNode {
    Act(CastingPermissionNode),
    LookAtLibraryTopAnyTime { actor: CastingPermissionActorNode },
}

pub(in crate::oracle::canonical) enum CastingPermissionRecognition {
    NotClaimed,
    RejectedEnvelope(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(PlayerPermissionNode),
}

fn peek_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn recognize_permission_actor(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastingPermissionActorNode> {
    cursor.enter("permission.actor");
    if peek_is(cursor, 0, "you") {
        cursor.consume("you")?;
        Ok(CastingPermissionActorNode::Controller)
    } else if peek_is(cursor, 0, "any") && peek_is(cursor, 1, "player") {
        // `Any player may`: the permission is held by each player.
        cursor.consume_phrase(&["any", "player"])?;
        Ok(CastingPermissionActorNode::Set(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else {
        Ok(CastingPermissionActorNode::Set(
            recognize_player_set_reference(cursor)?,
        ))
    }
}

/// `the first SPELLS you cast each turn`: a singular spell-history ordinal
/// whose caster relation must be the permission actor.
fn recognize_first_spell_each_turn(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<SpellCastQueryNode> {
    cursor.enter("permission.cast.ordinal-head");
    cursor.consume("the")?;
    let ordinal = recognize_ordinal_atom(cursor)?;
    cursor.enter("permission.cast.ordinal-value");
    cursor.require(
        ordinal == 1,
        "the first spell each turn; later spell-history ordinals are not represented",
    )?;
    cursor.enter("permission.cast.ordinal-query");
    let spells = recognize_spell_cast_collection(cursor)?;
    cursor.enter("permission.cast.ordinal-caster");
    cursor.require(
        actor == CastingPermissionActorNode::Controller
            && spells.caster == Some(SpellCasterScope::AbilityController),
        "the permission actor as the caster of the counted spells",
    )?;
    cursor.enter("permission.cast.ordinal-period");
    cursor.consume_phrase(&["each", "turn"])?;
    Ok(SpellCastQueryNode {
        predicate: spells.predicate,
        caster: None,
    })
}

fn recognize_permitted_action(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<PermittedActionNode> {
    cursor.enter("permission.action.verb");
    if peek_is(cursor, 0, "cast") {
        cursor.consume("cast")?;
        cursor.enter("permission.cast.spell-query");
        if peek_is(cursor, 0, "the") {
            return Ok(PermittedActionNode::Cast {
                spells: recognize_first_spell_each_turn(cursor, actor)?,
                scope: PermittedSpellScopeNode::FirstEachTurn,
            });
        }
        return Ok(PermittedActionNode::Cast {
            spells: recognize_spell_cast_collection(cursor)?,
            scope: PermittedSpellScopeNode::Each,
        });
    }
    cursor.consume("play")?;
    cursor.enter("permission.play.card-query");
    let query = recognize_object_collection(cursor)?;
    cursor.enter("permission.play.card-domain");
    cursor.require(
        query.controller.is_none()
            && query.predicate.has_nominal_head()
            && query.predicate.supports_domain(ObjectKind::Card),
        "a card collection without a battlefield controller relation",
    )?;
    Ok(PermittedActionNode::Play {
        cards: query.predicate,
    })
}

/// `cast A`, `play B and cast C`: one connector per extra action. A failed
/// continuation is not consumed, so its `and` stays with the parent.
fn recognize_permitted_actions(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<Vec<PermittedActionNode>> {
    let mut actions = vec![recognize_permitted_action(cursor, actor)?];
    while peek_is(cursor, 0, "and") && (peek_is(cursor, 1, "cast") || peek_is(cursor, 1, "play")) {
        cursor.enter("permission.action.connector");
        cursor.consume("and")?;
        actions.push(recognize_permitted_action(cursor, actor)?);
    }
    Ok(actions)
}

fn recognize_permission_origin(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<PermissionOriginNode> {
    cursor.enter("permission.origin.relation");
    cursor.consume("from")?;
    let possessive = match actor {
        CastingPermissionActorNode::Controller => "your",
        CastingPermissionActorNode::Set(_) => "their",
    };
    if peek_is(cursor, 0, "the") {
        cursor.enter("permission.origin.library-top");
        cursor.consume_phrase(&["the", "top", "of", possessive])?;
        let zone = recognize_card_zone_atom(cursor)?;
        cursor.require(zone == CardZoneName::Library, "the top of a library")?;
        return Ok(PermissionOriginNode::LibraryTop);
    }
    cursor.enter("permission.origin.zone");
    if !peek_is(cursor, 0, "exile") {
        cursor.consume(possessive)?;
    }
    Ok(PermissionOriginNode::Zone(recognize_card_zone_atom(
        cursor,
    )?))
}

/// The singular ordinal head is resumed by `it`, a plural collection by
/// `they`; every action of the list shares one pronoun.
fn manner_pronoun(actions: &[PermittedActionNode]) -> &'static str {
    let singular = actions.iter().all(|action| {
        matches!(
            action,
            PermittedActionNode::Cast {
                scope: PermittedSpellScopeNode::FirstEachTurn,
                ..
            }
        )
    });
    if singular { "it" } else { "they" }
}

/// `look at the top card of your library any time`. The shared library-top
/// operand owns its quantity and owner; this frame owns the time allowance.
fn recognize_look_permission(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<PlayerPermissionNode> {
    cursor.enter("permission.look.verb");
    cursor.consume_phrase(&["look", "at"])?;
    cursor.enter("permission.look.object");
    let top = match try_recognize_card_library_top(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::NotClaimed => {
            return Err(cursor.fail("the top card of a library"));
        }
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.require(
        top.count == NumericExpression::from(1),
        "exactly the top card",
    )?;
    cursor.enter("permission.look.timing");
    cursor.consume_phrase(&["any", "time"])?;
    Ok(PlayerPermissionNode::LookAtLibraryTopAnyTime { actor })
}

/// The body is a permission, not an effect executed when the permanent resolves.
/// The child borrows the original token cursor. A complete period terminates the
/// static envelope; conditional, temporary and later sentences are not claimed.
pub(in crate::oracle::canonical) fn try_recognize_casting_permission(
    cursor: &mut OracleTokenCursor<'_>,
) -> CastingPermissionRecognition {
    let mut trial = cursor.clone();
    let mut committed = false;
    let parsed = (|| -> MachineResult<PlayerPermissionNode> {
        let actor = recognize_permission_actor(&mut trial)?;
        trial.enter("permission.future-allowance");
        trial.consume("may")?;
        if peek_is(&trial, 0, "look") && peek_is(&trial, 1, "at") {
            // `may look at` selects the look allowance of this frame.
            committed = true;
            return recognize_look_permission(&mut trial, actor);
        }
        let actions = recognize_permitted_actions(&mut trial, actor)?;
        trial.enter("permission.origin");
        let origin = if peek_is(&trial, 0, "from") {
            // A stated origin is explicit semantic selection of this frame,
            // not cursor progress or a diagnostic-state naming convention.
            committed = true;
            recognize_permission_origin(&mut trial, actor)?
        } else {
            trial.require(
                peek_is(&trial, 0, "without") || peek_is(&trial, 0, "as"),
                "a source zone, a base-cost policy or a timing manner",
            )?;
            PermissionOriginNode::UsualZone
        };
        trial.enter("permission.base-cost");
        let base_cost = if peek_is(&trial, 0, "without") {
            committed = true;
            trial.consume_phrase(&["without", "paying", "their", "mana", "costs"])?;
            CastingBaseCostPolicyNode::WithoutManaCost
        } else {
            CastingBaseCostPolicyNode::Printed
        };
        trial.enter("permission.timing");
        let conjoined = peek_is(&trial, 0, "and") && peek_is(&trial, 1, "as");
        let as_though_flash = conjoined || peek_is(&trial, 0, "as");
        if as_though_flash {
            committed = true;
            if conjoined {
                trial.consume("and")?;
            }
            trial.consume_phrase(&["as", "though", manner_pronoun(&actions), "had", "flash"])?;
        }
        Ok(PlayerPermissionNode::Act(CastingPermissionNode {
            actor,
            actions,
            origin,
            base_cost,
            as_though_flash,
        }))
    })();
    let node = match parsed {
        Ok(node) => node,
        Err(failure) if committed => {
            return CastingPermissionRecognition::FailedCommitted(failure);
        }
        Err(_) => return CastingPermissionRecognition::NotClaimed,
    };
    trial.enter("permission.complete-envelope");
    if let Err(failure) = trial.consume(".").and_then(|_| trial.finish()) {
        return CastingPermissionRecognition::RejectedEnvelope(failure);
    }
    *cursor = trial;
    CastingPermissionRecognition::Parsed(node)
}
