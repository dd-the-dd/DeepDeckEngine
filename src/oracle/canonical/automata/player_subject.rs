use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{
    PermanentEffectReference, TargetCardinality, TargetListNode, TargetNominal, TargetSelector,
    recognize_target_list, selector_implies_nominal,
};
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerBinding(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerEffectSubject {
    Controller,
    DeclaredTarget {
        target: TargetListNode,
        binding: PlayerBinding,
    },
    Bound(PlayerBinding),
    /// `that player` naming the player of the triggering event, such as the
    /// player dealt combat damage (CR 603.2). The event, not the effect
    /// program, binds this player; the runtime reads it as the triggering player.
    EventPlayer,
    /// `its controller` / `that creature's owner`: the player related to an
    /// object the parent already bound (CR 108.3 owner, 109.4 controller).
    /// The runtime reads the object's current or last known information.
    ObjectRelation {
        relation: ObjectPlayerRelation,
        object: PermanentEffectReference,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectPlayerRelation {
    Controller,
    Owner,
}

/// The singular object a parent proved as the antecedent of `its` / `that
/// NOUN's`, with the selector that declared it when it is a target.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct ObjectSubjectAntecedent {
    pub reference: PermanentEffectReference,
    pub selector: Option<TargetSelector>,
}

fn player_relation(raw: &str) -> Option<ObjectPlayerRelation> {
    if raw.eq_ignore_ascii_case("controller") {
        Some(ObjectPlayerRelation::Controller)
    } else if raw.eq_ignore_ascii_case("owner") {
        Some(ObjectPlayerRelation::Owner)
    } else {
        None
    }
}

pub(in crate::oracle::canonical) fn defending_player_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("defending"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
}

/// `defending player` in an attack trigger (CR 506.2): the attacked player
/// the event records. Elsewhere it has no binding and fails explicitly.
pub(in crate::oracle::canonical::automata) fn recognize_defending_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    available: bool,
) -> MachineResult<PlayerEffectSubject> {
    cursor.enter("player-subject.defending-player");
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`defending player`"))?;
    cursor.consume_phrase(&["defending", "player"])?;
    if !available {
        return Err(cursor.fail_with_span(
            "an attack trigger event that records the defending player",
            first.index,
            first.span_start,
            first.span_end,
            first.raw,
        ));
    }
    cursor.enter("player-subject.complete");
    Ok(PlayerEffectSubject::EventPlayer)
}

/// Bounded lexical head: `its controller|owner` or `that NOUN's
/// controller|owner`. Returns the token length; it proves no binding.
pub(in crate::oracle::canonical) fn object_related_player_head_length(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    let first = cursor.peek()?;
    if first.eq_ignore_ascii_case("its") {
        return player_relation(cursor.peek_ahead(1)?).map(|_| 2);
    }
    if first.eq_ignore_ascii_case("that")
        && cursor
            .token_slice(cursor.position() + 1, cursor.position() + 2)
            .first()
            .is_some_and(|token| token.token_type == "possessiveTypeWord")
    {
        return player_relation(cursor.peek_ahead(2)?).map(|_| 3);
    }
    None
}

/// `its controller|owner` / `that NOUN's controller|owner` (CR 108.3,
/// 109.4). The object is the antecedent the parent supplies; a nominal must
/// follow from the selector that declared that target. Nothing is guessed.
pub(in crate::oracle::canonical::automata) fn recognize_object_related_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<ObjectSubjectAntecedent>,
) -> MachineResult<PlayerEffectSubject> {
    cursor.enter("player-subject.object-relation");
    let first = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("`its` or `that NOUN's`"))?;
    let nominal = if first.raw.eq_ignore_ascii_case("its") {
        cursor.consume("its")?;
        None
    } else {
        cursor.consume("that")?;
        cursor.enter("player-subject.object-relation.nominal");
        let possessive = cursor.consume_type("possessiveTypeWord")?;
        let normalized = possessive.raw.to_ascii_lowercase();
        let surface = normalized
            .strip_suffix("'s")
            .or_else(|| normalized.strip_suffix("\u{2019}s"))
            .ok_or_else(|| cursor.fail("a possessive registered object nominal"))?;
        Some(match lookup_object_selector_lexeme(surface) {
            Some(CharacteristicLexeme::CardType(value)) => TargetNominal::CardType(value),
            Some(CharacteristicLexeme::ObjectDomain(value)) => TargetNominal::Domain(value),
            _ => {
                return Err(cursor.fail("a registered card type or object domain nominal"));
            }
        })
    };
    cursor.enter("player-subject.object-relation.relation");
    let relation = cursor
        .peek()
        .and_then(player_relation)
        .ok_or_else(|| cursor.fail("`controller` or `owner`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("player-subject.object-relation.antecedent");
    let antecedent = antecedent.ok_or_else(|| {
        cursor.fail_with_span(
            "a preceding singular object binding for the possessive",
            first.index,
            first.span_start,
            first.span_end,
            first.raw.clone(),
        )
    })?;
    // Event-observed objects have no runtime player-relation contract yet.
    cursor.require(
        !matches!(antecedent.reference, PermanentEffectReference::Observed(_)),
        "a declared target, the source, or its attached permanent as the possessive antecedent",
    )?;
    if let Some(nominal) = nominal {
        cursor.require(
            antecedent.selector.as_ref().is_some_and(|selector| {
                selector_implies_nominal(selector, &nominal.as_predicate())
            }),
            "a nominal implied by the declared selector of the preceding target",
        )?;
    }
    cursor.enter("player-subject.complete");
    Ok(PlayerEffectSubject::ObjectRelation {
        relation,
        object: antecedent.reference,
    })
}

/// Recognizes a player acting as the subject of an effect. `that player`
/// resolves only to the exact binding supplied by the parent, or, without a
/// local antecedent, to the player of a triggering event that names one
/// (`event_player`); it never opens a new target declaration implicitly.
pub(in crate::oracle::canonical::automata) fn recognize_player_effect_subject(
    cursor: &mut OracleTokenCursor<'_>,
    declared_target_binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
    event_player: bool,
) -> MachineResult<PlayerEffectSubject> {
    cursor.enter("player-subject.start");
    let Some(first) = cursor.peek_token().cloned() else {
        return Err(cursor.fail("`you`, a player target, or bound `that player`"));
    };

    if first.raw.eq_ignore_ascii_case("you") {
        cursor.enter("player-subject.controller");
        cursor.consume("you")?;
        cursor.enter("player-subject.complete");
        return Ok(PlayerEffectSubject::Controller);
    }

    if first.raw.eq_ignore_ascii_case("target") {
        cursor.enter("player-subject.target");
        let target = recognize_target_list(cursor)?;
        cursor.enter("player-subject.target.validation");
        cursor.require(
            matches!(
                target.slots.as_slice(),
                [slot]
                    if matches!(&slot.selector, TargetSelector::Player(_))
                        && slot.cardinality == TargetCardinality::EXACTLY_ONE
            ),
            "exactly one target player or opponent",
        )?;
        cursor.enter("player-subject.complete");
        return Ok(PlayerEffectSubject::DeclaredTarget {
            target,
            binding: declared_target_binding,
        });
    }

    if first.raw.eq_ignore_ascii_case("that") {
        cursor.enter("player-subject.bound.reference");
        cursor.consume_phrase(&["that", "player"])?;
        cursor.enter("player-subject.bound.resolve");
        if antecedent.is_none() && event_player {
            cursor.enter("player-subject.event-player");
            cursor.enter("player-subject.complete");
            return Ok(PlayerEffectSubject::EventPlayer);
        }
        let binding = antecedent.ok_or_else(|| {
            cursor.fail_with_span(
                "a prior player binding for `that player`",
                first.index,
                first.span_start,
                first.span_end,
                first.raw,
            )
        })?;
        cursor.enter("player-subject.complete");
        return Ok(PlayerEffectSubject::Bound(binding));
    }

    Err(cursor.fail_with_span(
        "`you`, a player target, or bound `that player`",
        first.index,
        first.span_start,
        first.span_end,
        first.raw,
    ))
}
