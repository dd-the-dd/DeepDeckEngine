use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{
    TargetCardinality, TargetListNode, TargetSelector, recognize_target_list,
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
}

/// Recognizes a player acting as the subject of an effect. `that player`
/// resolves only to the exact binding supplied by the parent; it never opens
/// a new target declaration implicitly.
pub(in crate::oracle::canonical::automata) fn recognize_player_effect_subject(
    cursor: &mut OracleTokenCursor<'_>,
    declared_target_binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
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
