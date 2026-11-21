use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventActorScope, ObjectKind, ObjectPredicate, SpellCastQueryNode, TriggeringPlayerBinding,
    object_predicate_uses_x, recognize_ordinal_atom, recognize_spell_cast_query_before_separator,
    try_recognize_event_actor_scope,
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

fn recognize_optional_spell_cast_event_atom(
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
                let query = recognize_spell_cast_query_before_separator(cursor)?;
                cursor.enter("event.spell-cast.caster-identity");
                cursor.require(
                    query.caster.is_none(),
                    "one caster relation, supplied by the event subject",
                )?;
                cursor.enter("event.spell-cast.spell-domain");
                cursor.require(
                    query.predicate.has_domain(ObjectKind::Spell)
                        && query.predicate.supports_domain(ObjectKind::Spell),
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
                frame.query = Some(query);
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
                }));
            }
        };
    }
}

#[cfg(test)]
#[path = "spell_cast_event_tests.rs"]
mod tests;
