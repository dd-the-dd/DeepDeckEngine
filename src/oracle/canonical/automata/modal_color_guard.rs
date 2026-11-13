use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardColor, CounterEffectNode, LocalEffectNode, ZoneChangeAction, ZoneChangeEffectNode,
    card_color_from_oracle_word, recognize_counter_effect, recognize_zone_change_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ColorGuardedModalActionNode {
    CounterSpell(CounterEffectNode),
    DestroyPermanent(ZoneChangeEffectNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ColorGuardedModeNode {
    pub action: ColorGuardedModalActionNode,
    pub required_color: CardColor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChooseOneColorGuardedModalNode {
    pub modes: Vec<ColorGuardedModeNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorGuardState {
    If,
    Subject,
    Color,
    Terminator,
    Complete,
}

fn recognize_color_guard(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardColor> {
    let mut state = ColorGuardState::If;
    let mut color = None;
    loop {
        state = match state {
            ColorGuardState::If => {
                cursor.enter("condition.target-color.if");
                cursor.consume("if")?;
                ColorGuardState::Subject
            }
            ColorGuardState::Subject => {
                cursor.enter("condition.target-color.subject");
                cursor.consume("it's")?;
                ColorGuardState::Color
            }
            ColorGuardState::Color => {
                cursor.enter("condition.target-color.color");
                let token = cursor.consume_type("colorWord")?;
                color = Some(
                    card_color_from_oracle_word(&token.raw)
                        .ok_or_else(|| cursor.fail("a Magic color"))?,
                );
                ColorGuardState::Terminator
            }
            ColorGuardState::Terminator => {
                cursor.enter("condition.target-color.terminator");
                cursor.consume(".")?;
                ColorGuardState::Complete
            }
            ColorGuardState::Complete => {
                cursor.enter("condition.target-color.complete");
                return cursor.require_value(color, "a populated target color guard");
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeState {
    Marker,
    Action,
    Guard,
    Complete,
}

fn recognize_color_guarded_mode(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ColorGuardedModeNode> {
    let mut state = ModeState::Marker;
    let mut action = None;
    let mut required_color = None;
    loop {
        state = match state {
            ModeState::Marker => {
                cursor.enter("modal.mode.marker");
                cursor.consume("•")?;
                ModeState::Action
            }
            ModeState::Action => {
                cursor.enter("modal.mode.action");
                action = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("counter") => {
                            let effect = recognize_counter_effect(cursor)?;
                            let LocalEffectNode::CounterSpell(effect) = effect else {
                                return Err(cursor.fail("a counter-spell mode"));
                            };
                            ColorGuardedModalActionNode::CounterSpell(effect)
                        }
                        Some("destroy") => {
                            let effect = recognize_zone_change_effect(cursor)?;
                            let LocalEffectNode::ZoneChange(effect) = effect else {
                                return Err(cursor.fail("a destroy-permanent mode"));
                            };
                            cursor.require(
                                effect.action == ZoneChangeAction::Destroy,
                                "a destroy-permanent mode",
                            )?;
                            ColorGuardedModalActionNode::DestroyPermanent(effect)
                        }
                        _ => return Err(cursor.fail("a supported modal action")),
                    },
                );
                ModeState::Guard
            }
            ModeState::Guard => {
                cursor.enter("modal.mode.guard");
                required_color = Some(recognize_color_guard(cursor)?);
                ModeState::Complete
            }
            ModeState::Complete => {
                cursor.enter("modal.mode.complete");
                return Ok(ColorGuardedModeNode {
                    action: cursor.require_value(action, "a modal action")?,
                    required_color: cursor.require_value(required_color, "a target color guard")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModalState {
    ChoiceAction,
    Cardinality,
    HeaderSeparator,
    Mode,
    Complete,
}

/// Recognizes `Choose one —` followed by two or more bullet modes whose local
/// counter/destroy actions are independently guarded by target color.
pub(in crate::oracle::canonical) fn recognize_choose_one_color_guarded_modal(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ChooseOneColorGuardedModalNode> {
    let mut state = ModalState::ChoiceAction;
    let mut modes = Vec::new();
    loop {
        state = match state {
            ModalState::ChoiceAction => {
                cursor.enter("modal.choose-one.action");
                cursor.consume("choose")?;
                ModalState::Cardinality
            }
            ModalState::Cardinality => {
                cursor.enter("modal.choose-one.cardinality");
                cursor.consume("one")?;
                ModalState::HeaderSeparator
            }
            ModalState::HeaderSeparator => {
                cursor.enter("modal.choose-one.header-separator");
                cursor.consume("—")?;
                ModalState::Mode
            }
            ModalState::Mode => {
                cursor.enter("modal.choose-one.mode");
                if cursor.peek().is_some_and(|raw| raw == "•") {
                    modes.push(recognize_color_guarded_mode(cursor)?);
                    ModalState::Mode
                } else {
                    ModalState::Complete
                }
            }
            ModalState::Complete => {
                cursor.enter("modal.choose-one.complete");
                cursor.require(modes.len() >= 2, "at least two color-guarded modes")?;
                cursor.finish()?;
                return Ok(ChooseOneColorGuardedModalNode { modes });
            }
        };
    }
}
