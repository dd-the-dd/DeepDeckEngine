//! Static player permissions share one frame (CR 609.3, 601.2, 113.6):
//!
//! ```text
//! Permission := ACTOR "may" ACTION [MANNER] [CONDITION] "." [REMINDER]
//! ACTION     := ("cast" | "play") COLLECTION [ORIGIN] [BASE-COST]   allowances
//!             | "look" "at" LIBRARY-TOP "any" "time"
//!             | "cast" SOURCE ["from" ZONE]                          self cast
//!             | "exert" SOURCE "as" PRONOUN "attacks"                CR 701.43
//!             | "have" SOURCE "assign" ... "as" "though" ... "weren't" "blocked"
//!             | "pay" MANA "rather" "than" "pay" "the" "mana" "cost" "for" SPELLS
//!             | "choose" "not" "to" "untap" SOURCE "during" "your" "untap" "step"
//! MANNER     := "as" "though" ("it" | "they") "had" "flash"         CR 702.8a
//! CONDITION  := ("if" | "as" "long" "as") STATIC-CONDITION          CR 611.3a
//! ```
//!
//! Actor, action lemma, operand, origin, base-cost policy, timing manner and
//! condition are independent children: the action lemma comes from the verb
//! lexicon, the operands from the shared source, collection, zone, mana and
//! condition readers. Once a lemma and its source operand are read, the
//! frame owns every later failure.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

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
    /// `a SPELL` / `an A or B spell`: one spell for each use of the
    /// permission; only a once-per-turn window gives it a meaning.
    One,
}

/// When a static permission may be used (CR 611.3a): always, only during
/// its controller's turns, or once during each of them (a per-source usage
/// counter the runtime keeps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermissionTurnWindowNode {
    Always,
    DuringYourTurns,
    OnceDuringYourTurns,
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
    /// `[Once] during each of your turns,` before the actor.
    pub window: PermissionTurnWindowNode,
}

/// A static permission: an action allowance, the hidden-zone look
/// allowance, or a permission whose operand is the ability's own source.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum PlayerPermissionNode {
    Act(CastingPermissionNode),
    LookAtLibraryTopAnyTime {
        actor: CastingPermissionActorNode,
    },
    /// `you may cast SOURCE [from ZONE] [as though it had flash] [CONDITION]`
    /// (CR 601.3, 702.8a): the card's own casting permission, functioning
    /// from the zone it names (CR 113.6).
    CastSelf(SelfCastPermissionNode),
    /// `you may exert SOURCE as it attacks. [When you do, PROGRAM.]` (CR
    /// 701.43, 508.1g): an option of the attack declaration and the
    /// reflexive ability linked to it (CR 603.12).
    ExertAsAttacks {
        reflexive: Option<EffectProgramNode>,
    },
    /// `you may have SOURCE assign its combat damage as though it weren't
    /// blocked` (CR 510.1c-d).
    AssignCombatDamageAsThoughUnblocked,
    /// `you may pay MANA rather than pay the mana cost for SPELLS you cast`
    /// (CR 118.9): an alternative cost for each matching spell.
    AlternativeManaCost {
        cost: ManaCostNode,
        spells: ObjectPredicate,
    },
    /// `you may choose not to untap SOURCE during your untap step` (CR
    /// 502.3).
    MayChooseNotToUntap,
    /// `PERMISSION, and you may PERMISSION`: allowances granted together by
    /// one static ability, in written order.
    Conjoined(Vec<PlayerPermissionNode>),
}

/// The source card's own casting permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SelfCastPermissionNode {
    /// `from ZONE`; `None` is the usual zone, the hand (CR 601.2a).
    pub origin: Option<CardZoneName>,
    /// `as though it had flash` (CR 702.8a).
    pub as_though_flash: bool,
    /// `if you pay MANA more to cast it`: the cost increase of casting the
    /// spell through the timing manner (CR 601.2f).
    pub timing_cost: Option<ManaCostNode>,
    /// `if CONDITION` / `as long as CONDITION` (CR 611.3a).
    pub condition: Option<ConditionNode>,
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
        // `an Aura or Equipment spell`: one spell per use of the permission.
        let one = peek_is(cursor, 0, "a") || peek_is(cursor, 0, "an");
        if one {
            cursor.enter("permission.cast.article");
            let article = if peek_is(cursor, 0, "a") { "a" } else { "an" };
            consume_word(cursor, article)?;
        }
        return Ok(PermittedActionNode::Cast {
            spells: recognize_spell_cast_collection(cursor)?,
            scope: if one {
                PermittedSpellScopeNode::One
            } else {
                PermittedSpellScopeNode::Each
            },
        });
    }
    cursor.consume("play")?;
    cursor.enter("permission.play.card-query");
    let query = recognize_object_collection(cursor)?;
    cursor.enter("permission.play.card-domain");
    cursor.require(
        !query.exclude_source
            && query.controller.is_none()
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
                scope: PermittedSpellScopeNode::FirstEachTurn | PermittedSpellScopeNode::One,
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
    let first = match recognize_permission_clause(&mut trial, &mut committed) {
        Ok(node) => node,
        Err(failure) if committed => {
            return CastingPermissionRecognition::FailedCommitted(failure);
        }
        Err(_) => return CastingPermissionRecognition::NotClaimed,
    };
    // `PERMISSION, and you may PERMISSION.`: one static ability granting
    // each permission (CR 113.2c). A second clause after a complete first
    // one belongs to this frame.
    let mut nodes = vec![first];
    while permission_conjunction_follows(&trial) {
        trial.enter("permission.conjunction");
        if peek_is(&trial, 0, ",") {
            if let Err(failure) = trial.consume(",") {
                return CastingPermissionRecognition::FailedCommitted(failure);
            }
        }
        if let Err(failure) = trial.consume("and") {
            return CastingPermissionRecognition::FailedCommitted(failure);
        }
        let mut second_committed = true;
        match recognize_permission_clause(&mut trial, &mut second_committed) {
            Ok(node) => nodes.push(node),
            Err(failure) => return CastingPermissionRecognition::FailedCommitted(failure),
        }
    }
    let node = if nodes.len() == 1 {
        nodes.pop().expect("one permission clause")
    } else {
        PlayerPermissionNode::Conjoined(nodes)
    };
    trial.enter("permission.complete-envelope");
    if let Err(failure) = trial
        .consume(".")
        .and_then(|_| recognize_permission_reminders(&mut trial, &node))
        .and_then(|_| trial.finish())
    {
        return CastingPermissionRecognition::RejectedEnvelope(failure);
    }
    *cursor = trial;
    CastingPermissionRecognition::Parsed(node)
}

/// `[,] and you may` / `[,] and PLAYERS may` after a complete clause.
fn permission_conjunction_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let offset = usize::from(peek_is(cursor, 0, ","));
    peek_is(cursor, offset, "and")
        && peek_is(cursor, offset + 1, "you")
        && peek_is(cursor, offset + 2, "may")
}

/// One `ACTOR may ACTION ...` clause of a collection allowance or of the
/// look allowance. `committed` becomes true once a word only this frame
/// reads (an origin, a base-cost policy, a manner, `look at`) is read.
fn recognize_permission_clause(
    trial: &mut OracleTokenCursor<'_>,
    committed: &mut bool,
) -> MachineResult<PlayerPermissionNode> {
    let actor = recognize_permission_actor(trial)?;
    trial.enter("permission.future-allowance");
    trial.consume("may")?;
    if peek_is(trial, 0, "look") && peek_is(trial, 1, "at") {
        // `may look at` selects the look allowance of this frame.
        *committed = true;
        return recognize_look_permission(trial, actor);
    }
    let actions = recognize_permitted_actions(trial, actor)?;
    trial.enter("permission.origin");
    let origin = if peek_is(trial, 0, "from") {
        // A stated origin is explicit semantic selection of this frame,
        // not cursor progress or a diagnostic-state naming convention.
        *committed = true;
        recognize_permission_origin(trial, actor)?
    } else {
        trial.require(
            peek_is(trial, 0, "without") || peek_is(trial, 0, "as"),
            "a source zone, a base-cost policy or a timing manner",
        )?;
        PermissionOriginNode::UsualZone
    };
    trial.enter("permission.base-cost");
    let base_cost = if peek_is(trial, 0, "without") {
        *committed = true;
        // The possessive agrees with the permitted spells: `its mana cost`
        // for one spell, `their mana costs` for a collection.
        let (possessive, costs) = if manner_pronoun(&actions) == "it" {
            ("its", "cost")
        } else {
            ("their", "costs")
        };
        trial.consume_phrase(&["without", "paying", possessive, "mana", costs])?;
        CastingBaseCostPolicyNode::WithoutManaCost
    } else {
        CastingBaseCostPolicyNode::Printed
    };
    trial.enter("permission.timing");
    let conjoined = peek_is(trial, 0, "and") && peek_is(trial, 1, "as");
    let as_though_flash = conjoined || peek_is(trial, 0, "as");
    if as_though_flash {
        *committed = true;
        if conjoined {
            trial.consume("and")?;
        }
        trial.consume_phrase(&["as", "though", manner_pronoun(&actions), "had", "flash"])?;
    }
    Ok(PlayerPermissionNode::Act(CastingPermissionNode {
        window: PermissionTurnWindowNode::Always,
        actor,
        actions,
        origin,
        base_cost,
        as_though_flash,
    }))
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

fn consume_word(cursor: &mut OracleTokenCursor<'_>, expected: &str) -> MachineResult<()> {
    cursor.require(word_is(cursor, 0, expected), expected)?;
    cursor.advance_to(cursor.position() + 1)
}

fn consume_words(cursor: &mut OracleTokenCursor<'_>, words: &[&str]) -> MachineResult<()> {
    words.iter().try_for_each(|word| consume_word(cursor, word))
}

/// `(You may cast it|them any time you could cast an instant.)`: the
/// glossary of the flash manner (CR 702.8a), read only after a permission
/// that grants it.
fn recognize_flash_timing_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    pronoun: &str,
) -> MachineResult<()> {
    cursor.enter("permission.reminder.flash-timing");
    consume_words(
        cursor,
        &[
            "(", "you", "may", "cast", pronoun, "any", "time", "you", "could", "cast", "an",
            "instant", ".", ")",
        ],
    )
}

/// `(Artifacts, legendaries, and Sagas are historic.)`: the glossary of
/// the defined term `historic` (CR 700.6), read only when the permission's
/// own text uses that term.
fn recognize_historic_glossary(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("permission.reminder.historic");
    cursor.require(
        cursor
            .token_slice(0, cursor.position())
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("historic")),
        "a glossary of a defined term the permission uses",
    )?;
    consume_words(
        cursor,
        &[
            "(",
            "artifacts",
            ",",
            "legendaries",
            ",",
            "and",
            "sagas",
            "are",
            "historic",
            ".",
            ")",
        ],
    )
}

/// `(It won't untap during your next untap step.)` / `(An exerted creature
/// won't untap during your next untap step.)`: the glossary of exert (CR
/// 701.43b), read only after an exert permission.
fn recognize_exert_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("permission.reminder.exert");
    consume_word(cursor, "(")?;
    if word_is(cursor, 0, "it") {
        consume_word(cursor, "it")?;
    } else {
        consume_words(cursor, &["an", "exerted", "creature"])?;
    }
    consume_words(
        cursor,
        &[
            "won't", "untap", "during", "your", "next", "untap", "step", ".", ")",
        ],
    )
}

/// Reminder text explains a permission's own manner or terms (CR 207.2);
/// any other parenthetical is refused, never skipped.
fn recognize_permission_reminders(
    cursor: &mut OracleTokenCursor<'_>,
    node: &PlayerPermissionNode,
) -> MachineResult<()> {
    let flash_pronoun = match node {
        PlayerPermissionNode::Act(permission) if permission.as_though_flash => {
            Some(if manner_pronoun(&permission.actions) == "it" {
                "it"
            } else {
                "them"
            })
        }
        PlayerPermissionNode::CastSelf(permission) if permission.as_though_flash => Some("it"),
        _ => None,
    };
    let exert = matches!(node, PlayerPermissionNode::ExertAsAttacks { .. });
    while cursor.peek() == Some("(") {
        let checkpoint = cursor.checkpoint();
        if let Some(pronoun) = flash_pronoun
            && recognize_flash_timing_reminder(cursor, pronoun).is_ok()
        {
            continue;
        }
        cursor.rollback(checkpoint);
        if exert {
            recognize_exert_reminder(cursor)?;
            continue;
        }
        recognize_historic_glossary(cursor)?;
    }
    Ok(())
}

/// What the source-operand children read besides tokens.
pub(in crate::oracle::canonical) struct SourcePermissionInputs<'a> {
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
}

type PermissionChild = fn(
    &mut OracleTokenCursor<'_>,
    &SourcePermissionInputs<'_>,
) -> MachineResult<PlayerPermissionNode>;

/// The action child a `you may LEMMA` head selects, before any of it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StaticPermissionKind {
    /// `cast SOURCE ...`: the card's own casting permission.
    SelfCast,
    /// `exert SOURCE as it attacks`.
    Exert,
    /// `have SOURCE assign ... as though it weren't blocked`.
    UnblockedDamage,
    /// `choose not to untap SOURCE ...`.
    MayNotUntap,
    /// `pay MANA rather than pay the mana cost for SPELLS`.
    AlternativeManaCost,
}

impl StaticPermissionKind {
    fn child(self) -> PermissionChild {
        match self {
            Self::SelfCast => recognize_self_cast_permission,
            Self::Exert => recognize_exert_permission,
            Self::UnblockedDamage => recognize_unblocked_damage_permission,
            Self::MayNotUntap => recognize_may_not_untap_permission,
            Self::AlternativeManaCost => recognize_alternative_mana_cost_permission,
        }
    }
}

/// Selects the action child from the lexicon lemma after `you may` and the
/// operand head that lemma's frame requires (a source reference, `not`, or
/// the replaced mana cost). `None` leaves the head to the collection
/// allowances or to another frame.
pub(in crate::oracle::canonical) fn static_permission_kind(
    cursor: &OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> Option<StaticPermissionKind> {
    if !(peek_is(cursor, 0, "you") && peek_is(cursor, 1, "may")) {
        return None;
    }
    let lemma = cursor
        .peek_ahead(2)
        .and_then(|raw| {
            lookup_verb(
                raw,
                &[
                    VerbLemma::Cast,
                    VerbLemma::Exert,
                    VerbLemma::Have,
                    VerbLemma::Pay,
                    VerbLemma::Choose,
                ],
            )
        })
        .filter(|(_, forms)| forms.contains(&VerbForm::Base))
        .map(|(entry, _)| entry.lemma)?;
    let source_follows = source_reference_starts_at(
        cursor.position() + 3,
        cursor.peek_ahead(3),
        name_links,
        name_failure,
    );
    // The word after the source operand: `exert SOURCE as`, `have SOURCE
    // assign`. Another frame owns `have SOURCE enter ...` or `fight`.
    let after_source = |expected: &str| {
        let mut probe = cursor.clone();
        probe.advance_to(cursor.position() + 3).is_ok()
            && recognize_source_reference(&mut probe, name_links, name_failure).is_ok()
            && word_is(&probe, 0, expected)
    };
    // A word that only might start an unresolved name (`cast historic
    // spells`) is not the source: the source reference must read.
    let source_reads = || {
        let mut probe = cursor.clone();
        probe.advance_to(cursor.position() + 3).is_ok()
            && recognize_source_reference(&mut probe, name_links, name_failure).is_ok()
    };
    match lemma {
        VerbLemma::Cast if source_follows && source_reads() => Some(StaticPermissionKind::SelfCast),
        VerbLemma::Exert if source_follows && after_source("as") => {
            Some(StaticPermissionKind::Exert)
        }
        VerbLemma::Have if source_follows && after_source("assign") => {
            Some(StaticPermissionKind::UnblockedDamage)
        }
        VerbLemma::Choose if peek_is(cursor, 3, "not") => Some(StaticPermissionKind::MayNotUntap),
        VerbLemma::Pay if alternative_mana_cost_follows(cursor) => {
            Some(StaticPermissionKind::AlternativeManaCost)
        }
        _ => None,
    }
}

/// The static permission frame: `you may LEMMA ...`. The action lemma comes
/// from the verb lexicon and selects the action child; the permissions over
/// collections keep their reader (`try_recognize_casting_permission`).
pub(in crate::oracle::canonical) fn try_recognize_static_player_permission(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> CastingPermissionRecognition {
    if let Some((window, width)) = permission_turn_window(cursor) {
        return recognize_windowed_permission(cursor, window, width);
    }
    let Some(kind) = static_permission_kind(cursor, name_links, name_failure) else {
        return try_recognize_casting_permission(cursor);
    };
    let child = kind.child();
    let inputs = SourcePermissionInputs {
        name_links,
        name_failure,
    };
    let mut trial = cursor.clone();
    let parsed = (|| -> MachineResult<PlayerPermissionNode> {
        trial.enter("permission.actor");
        trial.consume_phrase(&["you", "may"])?;
        let node = child(&mut trial, &inputs)?;
        trial.enter("permission.complete-envelope");
        // The exert child reads its own sentence and reflexive program.
        if !matches!(node, PlayerPermissionNode::ExertAsAttacks { .. }) {
            trial.consume(".")?;
        }
        recognize_permission_reminders(&mut trial, &node)?;
        trial.finish()?;
        Ok(node)
    })();
    match parsed {
        Ok(node) => {
            *cursor = trial;
            CastingPermissionRecognition::Parsed(node)
        }
        Err(failure) => CastingPermissionRecognition::FailedCommitted(failure),
    }
}

/// `you may cast SOURCE [from ZONE] [as though it had flash] [CONDITION]`.
fn recognize_self_cast_permission(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &SourcePermissionInputs<'_>,
) -> MachineResult<PlayerPermissionNode> {
    cursor.enter("permission.self-cast.verb");
    cursor.consume("cast")?;
    cursor.enter("permission.self-cast.source");
    recognize_source_reference(cursor, inputs.name_links, inputs.name_failure)?;
    let mut origin = None;
    if peek_is(cursor, 0, "from") {
        cursor.enter("permission.self-cast.origin");
        cursor.consume("from")?;
        if peek_is(cursor, 0, "your") {
            cursor.consume("your")?;
        }
        let zone = recognize_card_zone_atom(cursor)?;
        cursor.require(
            matches!(zone, CardZoneName::Graveyard | CardZoneName::Exile),
            "a zone the card's own permission functions from: a graveyard or exile",
        )?;
        origin = Some(zone);
        cursor.enter("permission.self-cast.origin-list");
        cursor.require(
            !peek_is(cursor, 0, "or") && !peek_is(cursor, 0, ","),
            "one origin zone; a card permission functioning from several zones is not represented",
        )?;
    }
    let as_though_flash = peek_is(cursor, 0, "as") && peek_is(cursor, 1, "though");
    if as_though_flash {
        cursor.enter("permission.self-cast.manner");
        consume_words(cursor, &["as", "though", "it", "had", "flash"])?;
    }
    let mut timing_cost = None;
    if as_though_flash
        && origin.is_none()
        && peek_is(cursor, 0, "if")
        && peek_is(cursor, 1, "you")
        && peek_is(cursor, 2, "pay")
    {
        cursor.enter("permission.self-cast.timing-cost");
        consume_words(cursor, &["if", "you", "pay"])?;
        let cost = recognize_mana_cost(cursor)?;
        cost.fixed_runtime_mana_value()
            .map_err(|error| cursor.fail(error.expected()))?;
        consume_words(cursor, &["more", "to", "cast", "it"])?;
        timing_cost = Some(cost);
    }
    cursor.enter("permission.self-cast.cost");
    cursor.require(
        !peek_is(cursor, 0, "by")
            && !peek_is(cursor, 0, "using")
            && !(peek_is(cursor, 0, "if")
                && peek_is(cursor, 1, "you")
                && peek_is(cursor, 2, "pay")),
        "a self casting permission without its own alternative or additional cost",
    )?;
    cursor.require(
        origin.is_some() || as_though_flash,
        "a source zone or a timing manner for the card's own permission",
    )?;
    let condition = if timing_cost.is_some() {
        None
    } else {
        recognize_permission_condition(cursor, inputs)?
    };
    Ok(PlayerPermissionNode::CastSelf(SelfCastPermissionNode {
        origin,
        as_though_flash,
        timing_cost,
        condition,
    }))
}

/// `if CONDITION` / `as long as CONDITION` after the permission: the shared
/// static condition program (CR 611.3a). The sentence end stays with the
/// envelope.
fn recognize_permission_condition(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &SourcePermissionInputs<'_>,
) -> MachineResult<Option<ConditionNode>> {
    let width = if peek_is(cursor, 0, "if") {
        1
    } else if peek_is(cursor, 0, "as") && peek_is(cursor, 1, "long") && peek_is(cursor, 2, "as") {
        3
    } else {
        return Ok(None);
    };
    cursor.enter("permission.condition.introducer");
    cursor.advance_to(cursor.position() + width)?;
    recognize_static_condition(
        cursor,
        StaticConditionContext {
            source_subject: false,
            name_links: inputs.name_links,
            name_failure: inputs.name_failure,
        },
    )
    .map(Some)
}

/// The pronoun resuming a source creature (`it`, `he`, `she`).
fn source_pronoun(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<&'static str> {
    ["it", "he", "she"]
        .into_iter()
        .find(|pronoun| word_is(cursor, offset, pronoun))
}

/// `exert SOURCE as PRONOUN attacks. [When you do, PROGRAM.]`. The
/// reflexive program reads the exerted creature as its `it` (CR 603.12).
fn recognize_exert_permission(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &SourcePermissionInputs<'_>,
) -> MachineResult<PlayerPermissionNode> {
    cursor.enter("permission.exert.verb");
    cursor.consume("exert")?;
    cursor.enter("permission.exert.source");
    recognize_source_reference(cursor, inputs.name_links, inputs.name_failure)?;
    cursor.enter("permission.exert.timing");
    cursor.consume("as")?;
    let pronoun = source_pronoun(cursor, 0)
        .ok_or_else(|| cursor.fail("the source pronoun of the attack declaration"))?;
    consume_words(cursor, &[pronoun, "attacks", "."])?;
    if !(peek_is(cursor, 0, "when") && peek_is(cursor, 1, "you") && peek_is(cursor, 2, "do")) {
        return Ok(PlayerPermissionNode::ExertAsAttacks { reflexive: None });
    }
    cursor.enter("permission.exert.reflexive-connector");
    consume_words(cursor, &["when", "you", "do", ","])?;
    cursor.enter("permission.exert.reflexive-body");
    let end = root_reminder_start(cursor);
    let context = triggered_body_context(
        EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
            .with_source_event_subject(LocalObjectReference::SourceSelf),
        None,
    );
    let program = cursor.bounded(end, |body| {
        recognize_effect_program_with_context(
            body,
            inputs.name_links,
            inputs.name_failure,
            &context,
        )
    })?;
    Ok(PlayerPermissionNode::ExertAsAttacks {
        reflexive: Some(program),
    })
}

/// The absolute position of the first root parenthetical, or the end.
fn root_reminder_start(cursor: &OracleTokenCursor<'_>) -> usize {
    let mut quoted = false;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "\"" => quoted = !quoted,
            "(" if !quoted => return cursor.position() + offset,
            _ => {}
        }
        offset += 1;
    }
    cursor.position() + offset
}

/// `have SOURCE assign POSSESSIVE combat damage as though PRONOUN weren't
/// blocked`.
fn recognize_unblocked_damage_permission(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &SourcePermissionInputs<'_>,
) -> MachineResult<PlayerPermissionNode> {
    cursor.enter("permission.unblocked-damage.verb");
    cursor.consume("have")?;
    cursor.enter("permission.unblocked-damage.source");
    recognize_source_reference(cursor, inputs.name_links, inputs.name_failure)?;
    cursor.enter("permission.unblocked-damage.action");
    consume_word(cursor, "assign")?;
    let possessive = ["its", "his", "her"]
        .into_iter()
        .find(|word| word_is(cursor, 0, word))
        .ok_or_else(|| cursor.fail("the source's possessive before `combat damage`"))?;
    consume_words(cursor, &[possessive, "combat", "damage"])?;
    cursor.enter("permission.unblocked-damage.manner");
    consume_words(cursor, &["as", "though"])?;
    let pronoun = source_pronoun(cursor, 0)
        .ok_or_else(|| cursor.fail("the source pronoun of the damage manner"))?;
    consume_words(cursor, &[pronoun, "weren't", "blocked"])?;
    Ok(PlayerPermissionNode::AssignCombatDamageAsThoughUnblocked)
}

/// `choose not to untap SOURCE during your untap step`.
fn recognize_may_not_untap_permission(
    cursor: &mut OracleTokenCursor<'_>,
    inputs: &SourcePermissionInputs<'_>,
) -> MachineResult<PlayerPermissionNode> {
    cursor.enter("permission.untap-choice.verb");
    consume_words(cursor, &["choose", "not", "to", "untap"])?;
    cursor.enter("permission.untap-choice.source");
    recognize_source_reference(cursor, inputs.name_links, inputs.name_failure)?;
    cursor.enter("permission.untap-choice.step");
    consume_words(cursor, &["during", "your", "untap", "step"])?;
    Ok(PlayerPermissionNode::MayChooseNotToUntap)
}

/// `pay MANA rather than pay the mana cost for ...` in this sentence.
fn alternative_mana_cost_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut offset = 3;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if matches!(raw, "." | "(") {
            return false;
        }
        if ["rather", "than", "pay", "the", "mana", "cost", "for"]
            .iter()
            .enumerate()
            .all(|(at, word)| peek_is(cursor, offset + at, word))
        {
            return true;
        }
        offset += 1;
    }
    false
}

/// `pay MANA rather than pay the mana cost for SPELLS you cast`: the shared
/// mana-cost reader and the shared spell query; the caster relation must be
/// the permission actor.
fn recognize_alternative_mana_cost_permission(
    cursor: &mut OracleTokenCursor<'_>,
    _inputs: &SourcePermissionInputs<'_>,
) -> MachineResult<PlayerPermissionNode> {
    cursor.enter("permission.alternative-cost.verb");
    cursor.consume("pay")?;
    cursor.enter("permission.alternative-cost.mana");
    let cost = recognize_mana_cost(cursor)?;
    cost.fixed_runtime_mana_value()
        .map_err(|error| cursor.fail(error.expected()))?;
    cursor.enter("permission.alternative-cost.replaced");
    consume_words(
        cursor,
        &["rather", "than", "pay", "the", "mana", "cost", "for"],
    )?;
    cursor.enter("permission.alternative-cost.spells");
    let spells = recognize_spell_cast_collection(cursor)?;
    cursor.require(
        spells.caster == Some(SpellCasterScope::AbilityController)
            && !object_predicate_uses_x(&spells.predicate),
        "spells the permission actor casts, without an undeclared numeric producer",
    )?;
    Ok(PlayerPermissionNode::AlternativeManaCost {
        cost,
        spells: spells.predicate,
    })
}

/// `once during each of your turns ,` / `during each of your turns ,`: the
/// turn window before the actor, and its width with the comma.
fn permission_turn_window(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(PermissionTurnWindowNode, usize)> {
    let during = |offset: usize| {
        ["during", "each", "of", "your", "turns", ","]
            .iter()
            .enumerate()
            .all(|(at, word)| peek_is(cursor, offset + at, word))
    };
    if peek_is(cursor, 0, "once") && during(1) {
        Some((PermissionTurnWindowNode::OnceDuringYourTurns, 7))
    } else if during(0) {
        Some((PermissionTurnWindowNode::DuringYourTurns, 6))
    } else {
        None
    }
}

/// A turn window governs the collection allowance after it. The window
/// commits the frame: the allowance's own failure, or its absence, is this
/// frame's failure.
fn recognize_windowed_permission(
    cursor: &mut OracleTokenCursor<'_>,
    window: PermissionTurnWindowNode,
    width: usize,
) -> CastingPermissionRecognition {
    let mut trial = cursor.clone();
    trial.enter("permission.window");
    if let Err(failure) = trial.advance_to(trial.position() + width) {
        return CastingPermissionRecognition::FailedCommitted(failure);
    }
    match try_recognize_casting_permission(&mut trial) {
        CastingPermissionRecognition::Parsed(PlayerPermissionNode::Act(mut node)) => {
            node.window = window;
            *cursor = trial;
            CastingPermissionRecognition::Parsed(PlayerPermissionNode::Act(node))
        }
        CastingPermissionRecognition::Parsed(_) => {
            trial.enter("permission.window.allowance");
            CastingPermissionRecognition::FailedCommitted(
                trial.fail("a cast or play allowance governed by the turn window"),
            )
        }
        CastingPermissionRecognition::FailedCommitted(failure)
        | CastingPermissionRecognition::RejectedEnvelope(failure) => {
            CastingPermissionRecognition::FailedCommitted(failure)
        }
        CastingPermissionRecognition::NotClaimed => {
            trial.enter("permission.window.allowance");
            CastingPermissionRecognition::FailedCommitted(
                trial.fail("a cast or play allowance governed by the turn window"),
            )
        }
    }
}
