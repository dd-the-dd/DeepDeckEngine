use super::super::{MachineResult, OracleTokenCursor};
use super::EffectConnector;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectorState {
    OptionalPunctuation,
    Kind,
    Complete,
}

/// Recognizes an effect-sequence connector. The lexical layer only identifies
/// punctuation and logic words; this parent-context child decides that `and`
/// or `then`, or a comma at an already completed effect boundary, orders
/// independent effects. Children retain their internal punctuation.
pub(in crate::oracle::canonical) fn recognize_effect_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EffectConnector> {
    let mut state = ConnectorState::OptionalPunctuation;
    let mut connector = None;
    let mut coordinating_comma = false;
    loop {
        state = match state {
            ConnectorState::OptionalPunctuation => {
                cursor.enter("sequence.connector.optional-punctuation");
                if cursor.peek().is_some_and(|token| token == ",") {
                    cursor.consume(",")?;
                    coordinating_comma = true;
                }
                ConnectorState::Kind
            }
            ConnectorState::Kind => {
                cursor.enter("sequence.connector.kind");
                connector = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("and") => {
                            cursor.consume("and")?;
                            EffectConnector::And
                        }
                        Some("then") => {
                            cursor.consume("then")?;
                            EffectConnector::Then
                        }
                        // Alternatives require program control; never lower `or`
                        // to sequential conjunction, even after punctuation.
                        Some("or") => return Err(cursor.fail("`and` or `then`")),
                        // Do not consume or guess the next action here. The
                        // sequence parent delegates it transactionally to a child.
                        _ if coordinating_comma => EffectConnector::And,
                        _ => return Err(cursor.fail("`and` or `then`")),
                    },
                );
                ConnectorState::Complete
            }
            ConnectorState::Complete => {
                cursor.enter("sequence.connector.complete");
                return cursor.require_value(connector, "a populated effect connector");
            }
        };
    }
}

#[cfg(test)]
#[path = "connector_tests.rs"]
mod tests;
