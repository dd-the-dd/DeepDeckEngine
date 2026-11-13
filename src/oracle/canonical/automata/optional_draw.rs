use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DrawCardsEffectNode, LocalEffectNode, LocalPlayerReference, NumericExpression,
    OptionalDrawCardsEffectNode, TargetBinding, recognize_card_noun, recognize_player_reference,
    recognize_positive_card_count,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferencedDrawState {
    Player,
    Permission,
    Action,
    Range,
    Count,
    Cards,
    Complete,
}

#[derive(Default)]
struct ReferencedDrawMemory {
    player: Option<LocalPlayerReference>,
    optional: bool,
    maximum: Option<NumericExpression>,
}

/// Recognizes a draw effect with an explicit player subject. Permission and
/// range are independent of timing: callers may wrap the completed effect in
/// a delayed-step, condition, trigger, or ordinary effect sequence.
pub(in crate::oracle::canonical) fn recognize_referenced_draw_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<LocalEffectNode> {
    let mut state = ReferencedDrawState::Player;
    let mut memory = ReferencedDrawMemory::default();
    loop {
        state = match state {
            ReferencedDrawState::Player => {
                cursor.enter("effect.referenced-draw.player");
                memory.player = Some(recognize_player_reference(cursor, antecedent)?);
                ReferencedDrawState::Permission
            }
            ReferencedDrawState::Permission => {
                cursor.enter("effect.referenced-draw.permission");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
                {
                    cursor.consume("may")?;
                    memory.optional = true;
                }
                ReferencedDrawState::Action
            }
            ReferencedDrawState::Action => {
                cursor.enter("effect.referenced-draw.action");
                cursor.consume("draw")?;
                ReferencedDrawState::Range
            }
            ReferencedDrawState::Range => {
                cursor.enter("effect.referenced-draw.range");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("up"))
                {
                    cursor.require(memory.optional, "`may` before `up to`")?;
                    cursor.consume_phrase(&["up", "to"])?;
                }
                ReferencedDrawState::Count
            }
            ReferencedDrawState::Count => {
                cursor.enter("effect.referenced-draw.count");
                memory.maximum = Some(NumericExpression::Atom(recognize_positive_card_count(
                    cursor,
                )?));
                ReferencedDrawState::Cards
            }
            ReferencedDrawState::Cards => {
                cursor.enter("effect.referenced-draw.cards");
                let maximum = memory
                    .maximum
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated draw count"))?;
                let NumericExpression::Atom(atom) = maximum else {
                    return Err(cursor.fail("a positive card-count atom"));
                };
                recognize_card_noun(cursor, atom)?;
                ReferencedDrawState::Complete
            }
            ReferencedDrawState::Complete => {
                cursor.enter("effect.referenced-draw.complete");
                let player = cursor.require_value(memory.player, "a drawing player")?;
                let maximum = cursor.require_value(memory.maximum, "a draw maximum")?;
                return Ok(if memory.optional {
                    LocalEffectNode::OptionalDrawCards(OptionalDrawCardsEffectNode {
                        player,
                        maximum,
                    })
                } else {
                    LocalEffectNode::DrawCards(DrawCardsEffectNode {
                        player,
                        count: maximum,
                    })
                });
            }
        };
    }
}
