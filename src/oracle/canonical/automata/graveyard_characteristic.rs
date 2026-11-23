use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, NumericExpression, recognize_distinct_card_type_count,
    recognize_numeric_expression, recognize_source_reference, with_prior_number,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PowerToughnessCharacteristicNode {
    pub object: LocalObjectReference,
    pub power: NumericExpression,
    pub toughness: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Source,
    PowerSubject,
    PowerEquality,
    PowerValue,
    Connector,
    ToughnessSubject,
    ToughnessEquality,
    ToughnessValue,
    Terminator,
    Complete,
}

#[derive(Default)]
struct Frame {
    object: Option<LocalObjectReference>,
    power: Option<NumericExpression>,
    toughness: Option<NumericExpression>,
}

/// Recognizes one characteristic-defining ability whose power is a distinct
/// card-type count and whose toughness arithmetically reuses that same count.
/// Source reference, zone-scoped count, numeric anaphora and arithmetic are
/// independent typed children.
pub(in crate::oracle::canonical) fn recognize_graveyard_card_type_characteristic(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PowerToughnessCharacteristicNode> {
    let mut state = State::Source;
    let mut frame = Frame::default();
    loop {
        state = match state {
            State::Source => {
                cursor.enter("characteristic.power-toughness.source");
                frame.object = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                State::PowerSubject
            }
            State::PowerSubject => {
                cursor.enter("characteristic.power-toughness.power-subject");
                cursor.consume("power")?;
                State::PowerEquality
            }
            State::PowerEquality => {
                cursor.enter("characteristic.power-toughness.power-equality");
                cursor.consume_phrase(&["is", "equal", "to"])?;
                State::PowerValue
            }
            State::PowerValue => {
                cursor.enter("characteristic.power-toughness.power-value");
                frame.power = Some(recognize_distinct_card_type_count(cursor)?);
                State::Connector
            }
            State::Connector => {
                cursor.enter("characteristic.power-toughness.connector");
                cursor.consume("and")?;
                State::ToughnessSubject
            }
            State::ToughnessSubject => {
                cursor.enter("characteristic.power-toughness.toughness-subject");
                cursor.consume_phrase(&["its", "toughness"])?;
                State::ToughnessEquality
            }
            State::ToughnessEquality => {
                cursor.enter("characteristic.power-toughness.toughness-equality");
                cursor.consume_phrase(&["is", "equal", "to"])?;
                State::ToughnessValue
            }
            State::ToughnessValue => {
                cursor.enter("characteristic.power-toughness.toughness-value");
                // `that number plus 1`: the shared arithmetic grammar reads
                // the power through the binding environment's prior number.
                let power = cursor.require_value(
                    frame.power.clone(),
                    "a prior numeric-expression binding for `that number`",
                )?;
                frame.toughness = Some(with_prior_number(cursor, power, |cursor| {
                    recognize_numeric_expression(cursor, None)
                })?);
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("characteristic.power-toughness.terminator");
                cursor.consume(".")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("characteristic.power-toughness.complete");
                cursor.finish()?;
                return Ok(PowerToughnessCharacteristicNode {
                    object: cursor.require_value(frame.object, "a characteristic object")?,
                    power: cursor.require_value(frame.power, "a power expression")?,
                    toughness: cursor.require_value(frame.toughness, "a toughness expression")?,
                });
            }
        };
    }
}
