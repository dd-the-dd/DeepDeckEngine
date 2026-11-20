use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::token_subtype_list::recognize_token_subtype_list;
use super::{
    CardColor, CreateTokensEffectNode, CreatureSubtype, CreatureTokenSpecNode,
    LocalObjectReference, LocalPlayerReference, LocalTriggerEventNode, NumericAtom,
    card_color_from_oracle_word, recognize_literal_power_toughness, recognize_numeric_atom,
    recognize_source_attacked_event,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CombatTokenBatchNode {
    pub creation: CreateTokensEffectNode,
    pub tapped: bool,
    pub attacking: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DelayedBatchCleanupNode {
    SacrificeAtNextEndStep,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MobilizeReminderNode {
    pub event: LocalTriggerEventNode,
    pub tokens: CombatTokenBatchNode,
    pub cleanup: DelayedBatchCleanupNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MobilizeKeywordNode {
    pub quantity: i64,
    pub reminder: Option<MobilizeReminderNode>,
}

fn positive_literal(cursor: &mut OracleTokenCursor<'_>, expected: &str) -> MachineResult<i64> {
    let atom = recognize_numeric_atom(cursor)?;
    match atom {
        NumericAtom::Integer(value) if value > 0 => Ok(value),
        _ => Err(cursor.fail(expected)),
    }
}

fn recognize_combat_token_batch(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CombatTokenBatchNode> {
    cursor.enter("effect.combat-token.action");
    cursor.consume("create")?;
    cursor.enter("effect.combat-token.quantity");
    let quantity = positive_literal(cursor, "a positive literal token quantity")?;
    cursor.enter("effect.combat-token.tapped");
    cursor.consume("tapped")?;
    cursor.enter("effect.combat-token.connector");
    cursor.consume("and")?;
    cursor.enter("effect.combat-token.attacking");
    cursor.consume("attacking")?;
    cursor.enter("effect.combat-token.statistics");
    let statistics = recognize_literal_power_toughness(cursor)?;
    cursor.enter("effect.combat-token.color");
    let color_raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a Magic color word"))?
        .to_string();
    let color =
        card_color_from_oracle_word(&color_raw).ok_or_else(|| cursor.fail("a Magic color word"))?;
    cursor.consume(&color_raw)?;
    cursor.enter("effect.combat-token.subtype");
    let subtypes = recognize_token_subtype_list(cursor)?.subtypes;
    cursor.enter("effect.combat-token.creature-noun");
    cursor.consume("creature")?;
    cursor.enter("effect.combat-token.token-noun");
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("`token` or `tokens`"))?
        .to_string();
    cursor.require(
        (quantity == 1 && noun.eq_ignore_ascii_case("token"))
            || (quantity != 1 && noun.eq_ignore_ascii_case("tokens")),
        "token noun agreement with quantity",
    )?;
    cursor.consume(&noun)?;
    Ok(CombatTokenBatchNode {
        creation: CreateTokensEffectNode {
            player: LocalPlayerReference::Controller,
            quantity: NumericAtom::Integer(quantity),
            token: CreatureTokenSpecNode {
                power: statistics.power,
                toughness: statistics.toughness,
                colors: vec![color],
                subtypes,
            },
        },
        tapped: true,
        attacking: true,
    })
}

/// The object pronoun agrees with the batch it denotes: `it` for one created
/// token, `them` for several. The batch quantity is supplied by the parent.
fn recognize_delayed_batch_cleanup(
    cursor: &mut OracleTokenCursor<'_>,
    batch_quantity: i64,
) -> MachineResult<DelayedBatchCleanupNode> {
    cursor.enter("effect.delayed-batch.action");
    cursor.consume("sacrifice")?;
    cursor.enter("effect.delayed-batch.objects");
    let pronoun = if batch_quantity == 1 { "it" } else { "them" };
    cursor.consume(pronoun)?;
    cursor.enter("effect.delayed-batch.timing-relation");
    cursor.consume("at")?;
    cursor.enter("effect.delayed-batch.timing");
    cursor.consume_phrase(&["the", "beginning", "of", "the", "next", "end", "step"])?;
    cursor.enter("effect.delayed-batch.complete");
    Ok(DelayedBatchCleanupNode::SacrificeAtNextEndStep)
}

fn recognize_mobilize_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<MobilizeReminderNode> {
    cursor.enter("keyword.mobilize.reminder.open");
    cursor.consume("(")?;
    cursor.enter("keyword.mobilize.reminder.trigger");
    let trigger = cursor.consume_type("triggerWord")?;
    cursor.require(
        trigger.raw.eq_ignore_ascii_case("whenever"),
        "the `whenever` trigger",
    )?;
    cursor.enter("keyword.mobilize.reminder.event");
    let event = recognize_source_attacked_event(cursor, name_links, name_failure)?;
    cursor.require(
        event
            == LocalTriggerEventNode::SourceAttacked {
                source: LocalObjectReference::SourceSelf,
            },
        "the current source attacking",
    )?;
    cursor.enter("keyword.mobilize.reminder.event-separator");
    cursor.consume(",")?;
    cursor.enter("keyword.mobilize.reminder.creation");
    let tokens = recognize_combat_token_batch(cursor)?;
    cursor.enter("keyword.mobilize.reminder.creation-terminator");
    cursor.consume(".")?;
    cursor.enter("keyword.mobilize.reminder.cleanup");
    let batch_quantity = match tokens.creation.quantity {
        NumericAtom::Integer(value) => value,
        NumericAtom::VariableX => return Err(cursor.fail("a literal token batch quantity")),
    };
    let cleanup = recognize_delayed_batch_cleanup(cursor, batch_quantity)?;
    cursor.enter("keyword.mobilize.reminder.cleanup-terminator");
    cursor.consume(".")?;
    cursor.enter("keyword.mobilize.reminder.close");
    cursor.consume(")")?;
    Ok(MobilizeReminderNode {
        event,
        tokens,
        cleanup,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MobilizeState {
    Keyword,
    Quantity,
    ReminderOrComplete,
    Complete,
}

#[derive(Default)]
struct MobilizeFrame {
    quantity: Option<i64>,
    reminder: Option<MobilizeReminderNode>,
}

/// Recognizes parameterized Mobilize and validates its optional reminder as
/// independent event, token-batch and delayed-cleanup children.
pub(in crate::oracle::canonical) fn recognize_mobilize_keyword(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<MobilizeKeywordNode> {
    let mut state = MobilizeState::Keyword;
    let mut frame = MobilizeFrame::default();
    loop {
        state = match state {
            MobilizeState::Keyword => {
                cursor.enter("keyword.mobilize.keyword");
                cursor.consume("mobilize")?;
                MobilizeState::Quantity
            }
            MobilizeState::Quantity => {
                cursor.enter("keyword.mobilize.quantity");
                frame.quantity = Some(positive_literal(
                    cursor,
                    "a positive literal Mobilize quantity",
                )?);
                MobilizeState::ReminderOrComplete
            }
            MobilizeState::ReminderOrComplete => {
                cursor.enter("keyword.mobilize.reminder-or-complete");
                if cursor.peek().is_some_and(|raw| raw == "(") {
                    let reminder = recognize_mobilize_reminder(cursor, name_links, name_failure)?;
                    let printed =
                        cursor.require_value(frame.quantity, "the printed Mobilize quantity")?;
                    let reminder_quantity = match reminder.tokens.creation.quantity {
                        NumericAtom::Integer(value) => value,
                        NumericAtom::VariableX => {
                            return Err(cursor.fail("the printed Mobilize quantity"));
                        }
                    };
                    cursor.enter("keyword.mobilize.reminder.quantity-match");
                    cursor.require(
                        reminder_quantity == printed,
                        "a reminder quantity equal to the printed Mobilize quantity",
                    )?;
                    cursor.require(
                        reminder.tokens.creation.token.power == 1
                            && reminder.tokens.creation.token.toughness == 1
                            && reminder.tokens.creation.token.colors == [CardColor::Red]
                            && reminder.tokens.creation.token.subtypes
                                == [CreatureSubtype::Warrior],
                        "the canonical 1/1 red Warrior Mobilize token batch",
                    )?;
                    frame.reminder = Some(reminder);
                }
                MobilizeState::Complete
            }
            MobilizeState::Complete => {
                cursor.enter("keyword.mobilize.complete");
                cursor.finish()?;
                return Ok(MobilizeKeywordNode {
                    quantity: cursor
                        .require_value(frame.quantity, "a populated Mobilize quantity")?,
                    reminder: frame.reminder,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "mobilize_tests.rs"]
mod tests;
