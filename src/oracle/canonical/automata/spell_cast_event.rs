use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventActorScope, ObjectKind, ObjectPredicate, SpellCastQueryNode, TriggeringPlayerBinding,
    object_predicate_uses_x, recognize_ordinal_atom, try_recognize_event_actor_scope,
};

/// Compatibility name for existing cast consumers. The subject parser and
/// scope value are shared by all actor-bearing event atoms.
pub(in crate::oracle::canonical) type SpellCastEventCaster = EventActorScope;

/// V1 binds the ordinal only to the caster's unfiltered spell history.
/// A filtered ordinal needs a separately proved history-count contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SpellCastOrdinalBasis {
    AllSpellsByCasterThisTurn,
    MatchingSpellsByCasterThisTurn,
}

/// Each consumer declares the history semantics its lowerer can preserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SpellCastOrdinalSupport {
    UnfilteredOrdinalsOnly,
    MatchingSpellOrdinals,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellCastEventOrdinal {
    pub value: i64,
    pub basis: SpellCastOrdinalBasis,
}

/// One object or player a cast spell targets (CR 115.1, 601.2c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SpellTargetNode {
    /// `this creature`, the ability source.
    Source,
    /// A player in a relation to the ability controller.
    Player(EventActorScope),
    /// A permanent matching the shared object query.
    Permanent(super::ObjectQueryNode),
}

/// `from ZONE` / `from anywhere other than your hand`: the zone the spell
/// was cast from (CR 601.2a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellOriginNode {
    /// Runtime zone name: `hand`, `graveyard`, `exile`, `library`.
    pub zone: &'static str,
    /// `anywhere other than ZONE`.
    pub excluded: bool,
    /// `your ZONE`: the spell's owner is the ability controller.
    pub own_zone: bool,
}

/// `during your turn` / `during an opponent's turn` (CR 500.1): the shared
/// event modifier of `event_modifiers.rs`.
pub(in crate::oracle::canonical) type SpellTurnNode = super::EventTurnOwner;

/// `that targets [only] T [or T]...`: the spell's targets (CR 115.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellTargetsNode {
    /// `only`: every target is one of the alternatives.
    pub only: bool,
    pub alternatives: Vec<SpellTargetNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellCastEventNode {
    pub caster: SpellCastEventCaster,
    pub query: SpellCastQueryNode,
    pub ordinal: Option<SpellCastEventOrdinal>,
    pub triggering_player: TriggeringPlayerBinding,
    /// `cast this spell`: the cast spell is the ability source itself
    /// (CR 601.2i), observed from the stack.
    pub source_spell: bool,
    /// `cast or copy`: the same spell predicate also observes a copy of a
    /// spell (CR 707.10), a distinct event of the same lemma family.
    pub also_copy: bool,
    /// `that targets …`: a predicate on the spell's targets.
    pub targets: Option<SpellTargetsNode>,
    pub origin: Option<SpellOriginNode>,
    pub turn: Option<SpellTurnNode>,
}

pub(in crate::oracle::canonical) enum SpellCastEventAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Recognized(SpellCastEventNode),
}

#[derive(Clone, Copy)]
enum SpellCastEventState {
    Subject,
    Cast,
    Determiner,
    Ordinal,
    Query,
    Period,
    Complete,
}

#[derive(Default)]
struct SpellCastEventFrame {
    caster: Option<SpellCastEventCaster>,
    query: Option<SpellCastQueryNode>,
    ordinal: Option<SpellCastEventOrdinal>,
    source_spell: bool,
    also_copy: bool,
    targets: Option<SpellTargetsNode>,
    origin: Option<SpellOriginNode>,
    turn: Option<SpellTurnNode>,
}

/// Reads `from [your|a] ZONE` or `from anywhere other than your hand`.
fn recognize_spell_origin(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<SpellOriginNode> {
    cursor.enter("event.spell-cast.origin");
    cursor.consume("from")?;
    let excluded = peek_word(cursor, 0, "anywhere");
    if excluded {
        cursor.consume_phrase(&["anywhere", "other", "than"])?;
    }
    cursor.enter("event.spell-cast.origin-owner");
    let own_zone = peek_word(cursor, 0, "your");
    if own_zone {
        cursor.consume("your")?;
    } else if peek_word(cursor, 0, "a") {
        cursor.consume("a")?;
    }
    cursor.enter("event.spell-cast.origin-zone");
    let zone = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("hand") => "hand",
        Some("graveyard") => "graveyard",
        Some("exile") => "exile",
        Some("library") => "library",
        _ => return Err(cursor.fail("hand, graveyard, exile or library")),
    };
    let raw = cursor.peek().expect("checked zone").to_string();
    cursor.consume(&raw)?;
    Ok(SpellOriginNode {
        zone,
        excluded,
        own_zone,
    })
}

fn peek_word(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// Reads `that targets [only] T (or T)*` after a spell query: each target is
/// the source, a player relation, or a permanent through the shared query.
fn recognize_spell_targets(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<SpellTargetsNode> {
    cursor.enter("event.spell-cast.targets");
    cursor.consume_phrase(&["that", "targets"])?;
    let only = peek_word(cursor, 0, "only");
    if only {
        cursor.consume("only")?;
    }
    let mut alternatives = Vec::new();
    loop {
        cursor.enter("event.spell-cast.target");
        let target = if peek_word(cursor, 0, "this") {
            super::recognize_source_reference(cursor, &[], None)?;
            SpellTargetNode::Source
        } else if let Some(player) = super::try_recognize_event_actor_scope(cursor)? {
            SpellTargetNode::Player(player)
        } else {
            let article = cursor
                .peek()
                .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                .ok_or_else(|| cursor.fail("`this PERMANENT`, a player, or `a PERMANENT`"))?
                .to_string();
            cursor.consume(&article)?;
            SpellTargetNode::Permanent(super::recognize_object_query(cursor)?)
        };
        alternatives.push(target);
        if peek_word(cursor, 0, "or") {
            cursor.consume("or")?;
            continue;
        }
        return Ok(SpellTargetsNode { only, alternatives });
    }
}

/// The enclosing ability owns the trigger word and separator. This wrapper
/// transactionally composes that envelope with the shared event atom.
pub(in crate::oracle::canonical) fn try_recognize_spell_cast_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> SpellCastEventAttempt {
    let checkpoint = cursor.checkpoint();
    cursor.enter("event.spell-cast.trigger");
    let Some(raw) = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("when") || raw.eq_ignore_ascii_case("whenever"))
    else {
        cursor.rollback(checkpoint);
        return SpellCastEventAttempt::NotClaimed;
    };
    let trigger = raw.to_string();
    if let Err(failure) = cursor.consume(&trigger) {
        return SpellCastEventAttempt::Unpromoted(failure);
    }
    // The generic cast pattern counts matching spells by the caster this
    // turn; each consumer checks the ordinal basis its lowering preserves.
    match recognize_optional_spell_cast_event_atom(
        cursor,
        SpellCastOrdinalSupport::MatchingSpellOrdinals,
    ) {
        Ok(Some(node)) => {
            cursor.enter("event.spell-cast.separator");
            match cursor.consume(",") {
                Ok(_) => SpellCastEventAttempt::Recognized(node),
                Err(failure) => SpellCastEventAttempt::Unpromoted(failure),
            }
        }
        Ok(None) => {
            cursor.rollback(checkpoint);
            SpellCastEventAttempt::NotClaimed
        }
        Err(failure) => SpellCastEventAttempt::Unpromoted(failure),
    }
}

/// One reader for caster, action, ObjectQuery, ordinal and period. Strict
/// callers already own their family envelope and project the returned node
/// only after checking all properties supported by their own lowerer.
pub(in crate::oracle::canonical) fn recognize_spell_cast_event_atom(
    cursor: &mut OracleTokenCursor<'_>,
    ordinal_support: SpellCastOrdinalSupport,
) -> MachineResult<SpellCastEventNode> {
    recognize_optional_spell_cast_event_atom(cursor, ordinal_support)?
        .ok_or_else(|| cursor.fail("a supported spell-cast event atom"))
}

/// The cast clause after the trigger word, without the envelope separator.
/// `Ok(None)` when no actor-cast clause starts at the cursor.
pub(in crate::oracle::canonical) fn recognize_optional_spell_cast_event_atom(
    cursor: &mut OracleTokenCursor<'_>,
    ordinal_support: SpellCastOrdinalSupport,
) -> MachineResult<Option<SpellCastEventNode>> {
    let mut state = SpellCastEventState::Subject;
    let mut frame = SpellCastEventFrame::default();
    loop {
        state = match state {
            SpellCastEventState::Subject => {
                cursor.enter("event.spell-cast.caster");
                frame.caster = try_recognize_event_actor_scope(cursor)?;
                if frame.caster.is_none() {
                    return Ok(None);
                }
                SpellCastEventState::Cast
            }
            SpellCastEventState::Cast => {
                cursor.enter("event.spell-cast.action");
                if !cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("cast") || raw.eq_ignore_ascii_case("casts")
                }) {
                    return Ok(None);
                }
                cursor.consume(if frame.caster == Some(SpellCastEventCaster::Controller) {
                    "cast"
                } else {
                    "casts"
                })?;
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                    && cursor.peek_ahead(1).is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("copy") || raw.eq_ignore_ascii_case("copies")
                    })
                {
                    cursor.enter("event.spell-cast.copy-union");
                    let copy = cursor.peek_ahead(1).expect("checked verb").to_string();
                    cursor.consume_phrase(&["or", &copy])?;
                    frame.also_copy = true;
                }
                SpellCastEventState::Determiner
            }
            SpellCastEventState::Determiner => {
                cursor.enter("event.spell-cast.object-head");
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                }) {
                    let article = cursor.peek().expect("checked article").to_string();
                    cursor.consume(&article)?;
                    SpellCastEventState::Query
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("spell"))
                    && frame.caster == Some(SpellCastEventCaster::Controller)
                    && !frame.also_copy
                {
                    // `When you cast this spell`: the source itself.
                    cursor.enter("event.spell-cast.source-spell");
                    cursor.consume_phrase(&["this", "spell"])?;
                    frame.source_spell = true;
                    frame.query = Some(SpellCastQueryNode {
                        predicate: ObjectPredicate::ObjectKind(ObjectKind::Spell),
                        caster: None,
                    });
                    SpellCastEventState::Complete
                } else {
                    cursor.enter("event.spell-cast.ordinal-owner");
                    cursor.consume(if frame.caster == Some(SpellCastEventCaster::Controller) {
                        "your"
                    } else {
                        "their"
                    })?;
                    SpellCastEventState::Ordinal
                }
            }
            SpellCastEventState::Ordinal => {
                let value = recognize_ordinal_atom(cursor)?;
                cursor.enter("event.spell-cast.ordinal-value");
                cursor.require(
                    value > 0 && i32::try_from(value).is_ok(),
                    "a positive runtime spell-cast ordinal",
                )?;
                frame.ordinal = Some(SpellCastEventOrdinal {
                    value,
                    basis: SpellCastOrdinalBasis::AllSpellsByCasterThisTurn,
                });
                SpellCastEventState::Query
            }
            SpellCastEventState::Query => {
                let query = super::recognize_object_noun_query(
                    cursor,
                    super::ObjectQueryContext::ORDINARY,
                )?
                .into_spell_cast_query(cursor)?;
                cursor.enter("event.spell-cast.caster-identity");
                cursor.require(
                    query.caster.is_none(),
                    "one caster relation, supplied by the event subject",
                )?;
                cursor.enter("event.spell-cast.spell-domain");
                cursor.require(
                    query.predicate.is_in_domain(ObjectKind::Spell),
                    "an explicit spell-domain ObjectQuery",
                )?;
                cursor.enter("event.spell-cast.predicate-binding");
                cursor.require(
                    !object_predicate_uses_x(&query.predicate),
                    "an event predicate without an undeclared numeric producer",
                )?;
                if let Some(ordinal) = &mut frame.ordinal {
                    cursor.enter("event.spell-cast.ordinal-basis");
                    if query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Spell) {
                        cursor.require(
                            ordinal_support == SpellCastOrdinalSupport::MatchingSpellOrdinals,
                            "an unfiltered ordinal over all spells cast by this event's caster; filtered spell ordinals are not represented",
                        )?;
                        ordinal.basis = SpellCastOrdinalBasis::MatchingSpellsByCasterThisTurn;
                    }
                }
                let mut query = query;
                // `that has KEYWORD`: the shared keyword vocabulary as one more
                // conjunct of the spell predicate (the card's own keywords).
                if peek_word(cursor, 0, "that") && peek_word(cursor, 1, "has") {
                    cursor.enter("event.spell-cast.keyword");
                    cursor.consume_phrase(&["that", "has"])?;
                    let keyword = super::recognize_parameterless_keyword(cursor)?;
                    query.predicate = ObjectPredicate::Intersection(vec![
                        query.predicate,
                        ObjectPredicate::HasKeyword(keyword),
                    ]);
                }
                frame.query = Some(query);
                if peek_word(cursor, 0, "that") && peek_word(cursor, 1, "targets") {
                    frame.targets = Some(recognize_spell_targets(cursor)?);
                }
                if peek_word(cursor, 0, "from") {
                    frame.origin = Some(recognize_spell_origin(cursor)?);
                }
                if peek_word(cursor, 0, "during") {
                    frame.turn = Some(super::recognize_turn_owner_modifier(cursor)?);
                }
                if frame.ordinal.is_some() {
                    SpellCastEventState::Period
                } else {
                    SpellCastEventState::Complete
                }
            }
            SpellCastEventState::Period => {
                cursor.enter("event.spell-cast.ordinal-period");
                cursor.consume_phrase(&["each", "turn"])?;
                SpellCastEventState::Complete
            }
            SpellCastEventState::Complete => {
                cursor.enter("event.spell-cast.complete");
                return Ok(Some(SpellCastEventNode {
                    caster: cursor.require_value(frame.caster, "a typed event caster")?,
                    query: cursor.require_value(frame.query, "a typed spell query")?,
                    ordinal: frame.ordinal,
                    triggering_player: TriggeringPlayerBinding,
                    source_spell: frame.source_spell,
                    also_copy: frame.also_copy,
                    targets: frame.targets,
                    origin: frame.origin,
                    turn: frame.turn,
                }));
            }
        };
    }
}

#[cfg(test)]
#[path = "spell_cast_event_tests.rs"]
mod tests;
