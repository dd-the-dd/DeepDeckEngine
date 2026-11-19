//! Control-changing actions over the shared permanent operand (CR 108.4,
//! 613.1b): `[you] gain control of OBJECT [DURATION]` and `exchange control of
//! OBJECT` (CR 701.12). The object slot is the same target/reference/collection
//! child used by `tap`, `regenerate` and `goad`; the duration is the shared
//! effect-duration child or the sentence's preposed duration (CR 611.2a).
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, EffectRecognitionContext, PermanentOperandNode, TargetCardinality,
    recognize_contextual_permanent_operand, recognize_effect_duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ControlChange {
    /// The ability controller becomes the controller. `None` is the
    /// indefinite duration of a one-shot control change (CR 611.2a, 613.1b).
    Gain {
        duration: Option<EffectDurationNode>,
    },
    /// Two permanents swap controllers simultaneously (CR 701.12a).
    Exchange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControlChangeEffectNode {
    pub change: ControlChange,
    pub objects: PermanentOperandNode,
}

/// True at `gain control of`, `you gain control of` or `exchange control of`.
/// The verb is owned by this child only together with its `control of` object
/// frame; `gain N life` stays with the life reader.
pub(in crate::oracle::canonical) fn control_change_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let is = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    let verb = usize::from(is(0, "you"));
    (is(verb, "gain") || (verb == 0 && is(0, "exchange")))
        && is(verb + 1, "control")
        && is(verb + 2, "of")
}

/// Reads one control-changing action. Punctuation and following actions stay
/// with the enclosing sequence; an unsupported object or duration surfaces
/// this child's typed failure.
pub(in crate::oracle::canonical) fn recognize_control_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<ControlChangeEffectNode> {
    cursor.enter("effect.control-change.verb");
    let exchange = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("exchange"));
    if exchange {
        cursor.consume("exchange")?;
    } else {
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        {
            cursor.consume("you")?;
        }
        cursor.consume("gain")?;
    }
    cursor.enter("effect.control-change.relation");
    cursor.consume_phrase(&["control", "of"])?;
    cursor.enter("effect.control-change.object");
    let objects =
        recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    if exchange {
        // CR 701.12a: exactly two permanents exchange controllers. One target
        // slot of exactly two keeps that pair a single declared set.
        cursor.enter("effect.control-change.exchange-pair");
        cursor.require(
            matches!(
                &objects,
                PermanentOperandNode::Targets(targets)
                    if matches!(
                        targets.slots.as_slice(),
                        [slot] if slot.cardinality
                            == TargetCardinality { minimum: 2, maximum: 2 }
                    )
            ),
            "one target slot of exactly two permanents to exchange",
        )?;
        cursor.require(
            context.preposed_duration.is_none(),
            "no duration for an exchange of control",
        )?;
        return Ok(ControlChangeEffectNode {
            change: ControlChange::Exchange,
            objects,
        });
    }
    cursor.enter("effect.control-change.duration");
    let trailing = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("until"));
    let duration = match (&context.preposed_duration, trailing) {
        (Some(_), true) => {
            return Err(cursor.fail("no second duration after a sentence-level preposed duration"));
        }
        (Some(duration), false) => Some(duration.clone()),
        (None, true) => Some(recognize_effect_duration(cursor)?),
        (None, false) => None,
    };
    Ok(ControlChangeEffectNode {
        change: ControlChange::Gain { duration },
        objects,
    })
}
