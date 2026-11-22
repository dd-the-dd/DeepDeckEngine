use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{
    BindingEnvironment, FrameBindings, PermanentEffectReference, TargetCardinality, TargetListNode,
    TargetSelector, recognize_defending_player, recognize_object_related_player,
    recognize_target_list, recognize_that_player,
};
pub(in crate::oracle::canonical) use super::super::super::{
    defending_player_starts, object_related_player_head_length,
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

/// `defending player` in an attack trigger (CR 506.2), resolved by the shared
/// binding environment.
pub(in crate::oracle::canonical::automata) fn recognize_defending_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    available: bool,
) -> MachineResult<PlayerEffectSubject> {
    recognize_defending_player(cursor, available).map(|reference| {
        reference
            .into_effect_subject()
            .expect("`defending player` is the event's player")
    })
}

/// `its controller|owner` / `that NOUN's controller|owner` (CR 108.3,
/// 109.4), resolved by the shared binding environment.
pub(in crate::oracle::canonical::automata) fn recognize_object_related_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<ObjectSubjectAntecedent>,
) -> MachineResult<PlayerEffectSubject> {
    // The frame's event object is read from the environment the enclosing
    // sequence installed; the lexical antecedent is the caller's.
    let event_object = cursor
        .binding_environment()
        .and_then(|environment| environment.frame.event_object.clone());
    let reference =
        recognize_object_related_player(cursor, antecedent.as_ref(), event_object.as_ref(), false)?;
    reference
        .into_effect_subject()
        .ok_or_else(|| cursor.fail("a player relation with an effect-subject lowering"))
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
        let environment = BindingEnvironment {
            frame: FrameBindings {
                event_player,
                ..FrameBindings::default()
            },
            player: antecedent,
            ..BindingEnvironment::default()
        };
        return recognize_that_player(cursor, &environment).map(|reference| {
            reference
                .into_effect_subject()
                .expect("`that player` is a bound or event player")
        });
    }

    Err(cursor.fail_with_span(
        "`you`, a player target, or bound `that player`",
        first.index,
        first.span_start,
        first.span_end,
        first.raw,
    ))
}
