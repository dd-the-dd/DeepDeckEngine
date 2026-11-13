use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{NumericExpression, recognize_numeric_expression};
use super::counted_card::recognize_card_count_noun;
use super::player_subject::{PlayerBinding, PlayerEffectSubject, recognize_player_effect_subject};
use super::selection_method::{CollectionSelectionMethod, recognize_postfix_selection_method};
use super::{DiscardVerbForm, recognize_discard_action};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerDiscardEffectNode {
    pub player: PlayerEffectSubject,
    pub count: NumericExpression,
    pub selection: CollectionSelectionMethod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerDiscardState {
    Subject,
    Action,
    SelectionMethod,
    Complete,
}

/// Recognizes `PLAYER discard(s) N card(s)` as one typed effect. The runtime
/// `discardCards` primitive owns choosing the requested cards from that
/// player's hand. The optional selection modifier stays independent from the
/// action so lowering can compose either an ordinary choice or `chooseCards`
/// with its existing random-selection method.
pub(in crate::oracle::canonical::automata) fn recognize_player_discard_effect(
    cursor: &mut OracleTokenCursor<'_>,
    declared_target_binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
) -> MachineResult<PlayerDiscardEffectNode> {
    let mut state = PlayerDiscardState::Subject;
    let mut player = None;
    let mut count = None;
    let mut selection = None;
    loop {
        state = match state {
            PlayerDiscardState::Subject => {
                cursor.enter("effect.player-discard.subject");
                player = Some(recognize_player_effect_subject(
                    cursor,
                    declared_target_binding,
                    antecedent,
                )?);
                PlayerDiscardState::Action
            }
            PlayerDiscardState::Action => {
                cursor.enter("effect.player-discard.action");
                let verb_form = match player.as_ref() {
                    Some(PlayerEffectSubject::Controller) => DiscardVerbForm::Imperative,
                    Some(
                        PlayerEffectSubject::DeclaredTarget { .. } | PlayerEffectSubject::Bound(_),
                    ) => DiscardVerbForm::ThirdPersonSingular,
                    None => return Err(cursor.fail("a populated discard player subject")),
                };
                let action = recognize_discard_action(cursor, verb_form, |cursor| {
                    cursor.enter("effect.player-discard.count");
                    let expression = recognize_numeric_expression(cursor, None)?;
                    recognize_card_count_noun(cursor, &expression)?;
                    Ok(expression)
                })?;
                count = Some(action.operand);
                PlayerDiscardState::SelectionMethod
            }
            PlayerDiscardState::SelectionMethod => {
                cursor.enter("effect.player-discard.selection-method");
                selection = Some(recognize_postfix_selection_method(cursor)?);
                PlayerDiscardState::Complete
            }
            PlayerDiscardState::Complete => {
                cursor.enter("effect.player-discard.complete");
                return Ok(PlayerDiscardEffectNode {
                    player: cursor.require_value(player, "a discard player subject")?,
                    count: cursor.require_value(count, "a discard card count")?,
                    selection: cursor
                        .require_value(selection, "a discard collection-selection method")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "player_discard_tests.rs"]
mod tests;
