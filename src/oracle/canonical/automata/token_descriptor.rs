//! Characteristics belong to one shared token descriptor, not a named catalog lookup.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_card_type};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ExplicitTokenTemplateNode {
    pub creature: CreatureTokenSpecNode,
    pub types: Vec<ObjectCardType>,
}

/// Battlefield entry state attached to a token phrase. Only the tapped state
/// is representable by the engine's token creation; combined states such as
/// `tapped and attacking` are left for the enclosing parent to reject because
/// the next word is neither a descriptor nor a catalog identity.
pub(in crate::oracle::canonical) fn recognize_token_entry_state(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    cursor.enter("token.entry-state");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
    {
        cursor.consume("tapped")?;
        return Ok(true);
    }
    Ok(false)
}

pub(in crate::oracle::canonical) enum TokenDescriptorAttempt {
    NotClaimed,
    Recognized(ExplicitTokenTemplateNode),
    FailedCommitted(super::super::MachineFailure),
}

#[derive(Clone, Copy)]
enum DescriptorState {
    Statistics,
    Colors,
    Subtypes,
    Types,
    Complete,
}

/// The P/T lexical atom introduces this descriptor. Unknown named identities
/// remain a different child; no cursor-distance or slash scan decides ownership.
pub(in crate::oracle::canonical) fn try_recognize_token_descriptor(
    cursor: &mut OracleTokenCursor<'_>,
) -> TokenDescriptorAttempt {
    if !cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterValue")
    {
        return TokenDescriptorAttempt::NotClaimed;
    }
    match recognize_token_descriptor(cursor) {
        Ok(node) => TokenDescriptorAttempt::Recognized(node),
        Err(failure) => TokenDescriptorAttempt::FailedCommitted(failure),
    }
}

pub(in crate::oracle::canonical) fn recognize_token_descriptor(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ExplicitTokenTemplateNode> {
    recognize_token_descriptor_in_context(cursor, true)
}

pub(in crate::oracle::canonical) fn recognize_creature_token_descriptor(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ExplicitTokenTemplateNode> {
    recognize_token_descriptor_in_context(cursor, false)
}

fn recognize_token_descriptor_in_context(
    cursor: &mut OracleTokenCursor<'_>,
    generic_type_boundary: bool,
) -> MachineResult<ExplicitTokenTemplateNode> {
    let mut state = DescriptorState::Statistics;
    let mut statistics = None;
    let mut colors = Vec::new();
    let mut subtypes = Vec::new();
    let mut types = Vec::new();
    loop {
        state = match state {
            DescriptorState::Statistics => {
                let literal_index = cursor.position();
                let value = recognize_literal_power_toughness(cursor)?;
                cursor.enter("token.descriptor.statistics-range");
                if i32::try_from(value.power).is_err() || i32::try_from(value.toughness).is_err() {
                    let token = &cursor.token_slice(literal_index, literal_index + 1)[0];
                    return Err(cursor.fail_with_span(
                        "token power and toughness representable as i32",
                        literal_index,
                        token.span_start,
                        token.span_end,
                        token.raw.clone(),
                    ));
                }
                statistics = Some(value);
                DescriptorState::Colors
            }
            DescriptorState::Colors => {
                cursor.enter("token.descriptor.colors");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("colorless"))
                {
                    cursor.consume("colorless")?;
                } else {
                    loop {
                        let color = cursor
                            .peek()
                            .and_then(card_color_from_oracle_word)
                            .ok_or_else(|| cursor.fail("a registered Magic color or colorless"))?;
                        cursor.require(!colors.contains(&color), "distinct token colors")?;
                        colors.push(color);
                        cursor.advance_to(cursor.position() + 1)?;
                        // A color-list connector is owned only when its next
                        // item is another color. Other connectors stay outside.
                        let checkpoint = cursor.checkpoint();
                        let has_separator = matches!(cursor.peek(), Some(","))
                            || cursor.peek().is_some_and(|s| s.eq_ignore_ascii_case("and"));
                        if !has_separator {
                            break;
                        }
                        if cursor.peek() == Some(",") {
                            cursor.consume(",")?;
                        }
                        if cursor.peek().is_some_and(|s| s.eq_ignore_ascii_case("and")) {
                            cursor.consume("and")?;
                        }
                        if cursor
                            .peek()
                            .and_then(card_color_from_oracle_word)
                            .is_none()
                        {
                            cursor.rollback(checkpoint);
                            break;
                        }
                    }
                }
                DescriptorState::Subtypes
            }
            DescriptorState::Subtypes => {
                subtypes = if generic_type_boundary {
                    super::token_subtype_list::recognize_token_subtypes_before_card_types(cursor)?
                } else {
                    super::token_subtype_list::recognize_token_subtype_list(cursor)?
                }
                .subtypes;
                DescriptorState::Types
            }
            DescriptorState::Types => {
                cursor.enter("token.descriptor.types");
                while let Some(kind) = cursor
                    .peek()
                    .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
                {
                    cursor.enter("token.descriptor.permanent-type-domain");
                    cursor.require(
                        kind.may_appear_on_permanent(),
                        "card types compatible with battlefield presence (CR110.4)",
                    )?;
                    cursor.require(!types.contains(&kind), "distinct token card types")?;
                    types.push(kind);
                    cursor.advance_to(cursor.position() + 1)?;
                }
                cursor.require(
                    types.contains(&ObjectCardType::Creature),
                    "Creature among types for this power/toughness token descriptor",
                )?;
                DescriptorState::Complete
            }
            DescriptorState::Complete => {
                cursor.enter("token.descriptor.complete");
                let stats = cursor.require_value(statistics, "token statistics")?;
                return Ok(ExplicitTokenTemplateNode {
                    creature: CreatureTokenSpecNode {
                        power: stats.power,
                        toughness: stats.toughness,
                        colors,
                        subtypes,
                    },
                    types,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "token_entry_state_tests.rs"]
mod entry_state_tests;
