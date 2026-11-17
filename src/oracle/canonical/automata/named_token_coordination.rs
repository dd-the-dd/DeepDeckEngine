//! One shared token specification and its object-list coordination.
use super::super::{MachineResult, OracleTokenCursor};
use super::counter_named_token::recognize_named_token_identity;
use super::{NumericAtom, recognize_numeric_atom, recognize_token_entry_state};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct NamedTokenSpec {
    pub quantity: NumericAtom,
    pub token_name: String,
    pub tapped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum NamedTokenCreationPattern {
    Single(NamedTokenSpec),
    OneOf(Vec<NamedTokenSpec>),
}

#[derive(Clone, Copy)]
enum SpecState {
    Quantity,
    Identity,
    Noun,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_named_token_spec(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NamedTokenSpec> {
    let mut state = SpecState::Quantity;
    let mut quantity = None;
    let mut token_name = None;
    let mut tapped = false;
    loop {
        state = match state {
            SpecState::Quantity => {
                cursor.enter("effect.create-named-token.quantity");
                let value = if cursor
                    .peek()
                    .is_some_and(|s| s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an"))
                {
                    cursor.advance_to(cursor.position() + 1)?;
                    NumericAtom::Integer(1)
                } else {
                    recognize_numeric_atom(cursor)?
                };
                cursor.require(
                    !matches!(value,NumericAtom::Integer(n) if n<=0),
                    "a positive token quantity or X",
                )?;
                quantity = Some(value);
                SpecState::Identity
            }
            SpecState::Identity => {
                tapped = recognize_token_entry_state(cursor)?;
                token_name = Some(recognize_named_token_identity(cursor)?);
                SpecState::Noun
            }
            SpecState::Noun => {
                cursor.enter("effect.create-named-token.noun");
                let noun = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`token` or `tokens`"))?
                    .to_string();
                let singular = matches!(quantity, Some(NumericAtom::Integer(1)));
                cursor.require(
                    (singular && noun.eq_ignore_ascii_case("token"))
                        || (!singular && noun.eq_ignore_ascii_case("tokens")),
                    "token noun agreement with quantity",
                )?;
                cursor.consume(&noun)?;
                SpecState::Complete
            }
            SpecState::Complete => {
                return Ok(NamedTokenSpec {
                    quantity: cursor.require_value(quantity, "a named-token quantity")?,
                    token_name: cursor.require_value(token_name, "a named token")?,
                    tapped,
                });
            }
        }
    }
}

#[derive(Clone, Copy)]
enum CoordinationState {
    Connector,
    Specification,
    Complete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenConnector {
    Comma,
    And,
    Or,
}

struct PendingTokenConnector {
    checkpoint: super::super::machine::MachineCheckpoint,
    comma: bool,
    kind: TokenConnector,
}

struct CoordinationFrame {
    specs: Vec<NamedTokenSpec>,
    comma_seen: bool,
    last_explicit_or: bool,
    pending: Option<PendingTokenConnector>,
}

/// The action owns its coordinated objects. The sequence retains a connector
/// whose next child is an independent verb/actor rather than a token spec.
pub(in crate::oracle::canonical) fn recognize_named_token_coordination(
    cursor: &mut OracleTokenCursor<'_>,
    first: NamedTokenSpec,
) -> MachineResult<NamedTokenCreationPattern> {
    let mut state = CoordinationState::Connector;
    let mut frame = CoordinationFrame {
        specs: vec![first],
        comma_seen: false,
        last_explicit_or: false,
        pending: None,
    };
    loop {
        state = match state {
            CoordinationState::Connector => {
                let checkpoint = cursor.checkpoint();
                let raw = cursor.peek().unwrap_or_default().to_ascii_lowercase();
                if raw == "and/or" {
                    cursor.enter("token.coordination.inclusive-choice-support");
                    return Err(cursor.fail("a proved resolution-time subset cardinality before inclusive token alternatives"));
                }
                let kind = match raw.as_str() {
                    "or" => Some(TokenConnector::Or),
                    "and" => Some(TokenConnector::And),
                    "," => Some(TokenConnector::Comma),
                    _ => None,
                };
                if let Some(mut kind) = kind {
                    cursor.enter("token.coordination.connector");
                    cursor.consume(&raw)?;
                    let comma = kind == TokenConnector::Comma;
                    if comma
                        && cursor.peek().is_some_and(|s| {
                            s.eq_ignore_ascii_case("or") || s.eq_ignore_ascii_case("and")
                        })
                    {
                        let raw = cursor.peek().unwrap_or_default().to_ascii_lowercase();
                        kind = if raw == "or" {
                            TokenConnector::Or
                        } else {
                            TokenConnector::And
                        };
                        cursor.consume(&raw)?;
                    }
                    frame.pending = Some(PendingTokenConnector {
                        checkpoint,
                        comma,
                        kind,
                    });
                    CoordinationState::Specification
                } else {
                    CoordinationState::Complete
                }
            }
            CoordinationState::Specification => {
                let pending =
                    cursor.require_value(frame.pending.take(), "a pending token connector")?;
                match cursor.attempt(recognize_named_token_spec) {
                    Ok(spec) => {
                        if pending.kind == TokenConnector::And {
                            cursor.enter("token.coordination.simultaneous-creation-support");
                            return Err(cursor.fail(
                                "a proved simultaneous multi-specification token-creation batch",
                            ));
                        }
                        frame.comma_seen |= pending.comma;
                        frame.last_explicit_or = pending.kind == TokenConnector::Or;
                        frame.specs.push(spec);
                        CoordinationState::Connector
                    }
                    Err(failure) if pending.kind == TokenConnector::Or => return Err(failure),
                    Err(_) => {
                        cursor.rollback(pending.checkpoint);
                        CoordinationState::Complete
                    }
                }
            }
            CoordinationState::Complete => {
                return if frame.specs.len() == 1 {
                    Ok(NamedTokenCreationPattern::Single(frame.specs.remove(0)))
                } else {
                    cursor.enter("token.coordination.closing-connector");
                    cursor.require(
                        !frame.comma_seen || frame.last_explicit_or,
                        "an explicit closing `or` for this token list",
                    )?;
                    Ok(NamedTokenCreationPattern::OneOf(frame.specs))
                };
            }
        }
    }
}

#[cfg(test)]
#[path = "named_token_coordination_tests.rs"]
mod tests;
