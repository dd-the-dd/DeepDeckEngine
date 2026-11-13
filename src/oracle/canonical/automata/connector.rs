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
/// or `then` orders independent effects.
pub(in crate::oracle::canonical) fn recognize_effect_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EffectConnector> {
    let mut state = ConnectorState::OptionalPunctuation;
    let mut connector = None;
    loop {
        state = match state {
            ConnectorState::OptionalPunctuation => {
                cursor.enter("sequence.connector.optional-punctuation");
                if cursor.peek().is_some_and(|token| token == ",") {
                    cursor.consume(",")?;
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
