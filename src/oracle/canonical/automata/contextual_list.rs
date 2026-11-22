use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ContextualListConnector {
    And,
    Or,
    Then,
}

impl ContextualListConnector {
    fn from_oracle_word(raw: &str) -> Option<Self> {
        if raw.eq_ignore_ascii_case("and") {
            Some(Self::And)
        } else if raw.eq_ignore_ascii_case("or") || raw.eq_ignore_ascii_case("and/or") {
            // `and/or` is the inclusive disjunction: either or both.
            Some(Self::Or)
        } else if raw.eq_ignore_ascii_case("then") {
            Some(Self::Then)
        } else {
            None
        }
    }
}

/// Consumes the connector word as written (`or` and `and/or` are one
/// connector).
fn consume_connector(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .filter(|raw| ContextualListConnector::from_oracle_word(raw).is_some())
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("a list connector"))?;
    cursor.consume(&raw)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum BareCommaPolicy {
    /// Commas remain unresolved until a later explicit connector supplies
    /// their parent-owned meaning. This covers ordinary and Oxford lists.
    RequireLaterConnector,
    /// The parent assigns a meaning to a comma even when no later connector
    /// appears. Effect sequencing may eventually use this for Oracle families
    /// where punctuation alone orders independent instructions.
    Use(ContextualListConnector),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::oracle::canonical) struct ContextualListPolicy<'a> {
    pub connectors: &'a [ContextualListConnector],
    pub bare_comma: BareCommaPolicy,
}

impl<'a> ContextualListPolicy<'a> {
    pub(in crate::oracle::canonical) const fn serial(
        connectors: &'a [ContextualListConnector],
    ) -> Self {
        Self {
            connectors,
            bare_comma: BareCommaPolicy::RequireLaterConnector,
        }
    }

    pub(in crate::oracle::canonical) const fn with_comma_meaning(
        connectors: &'a [ContextualListConnector],
        connector: ContextualListConnector,
    ) -> Self {
        Self {
            connectors,
            bare_comma: BareCommaPolicy::Use(connector),
        }
    }

    fn accepts(self, connector: ContextualListConnector) -> bool {
        self.connectors.contains(&connector)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ContextualListNode<T> {
    pub items: Vec<T>,
    pub connectors: Vec<ContextualListConnector>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextualListState {
    FirstItem,
    SeparatorOrComplete,
    ItemAfterSeparator,
    Complete,
}

/// Recognizes an arbitrary-length contextual list while leaving item syntax
/// and connector meaning to the active parent.
///
/// A comma records an unresolved boundary. A later `and`, `or`, or `then`
/// resolves all pending serial boundaries to that connector, which covers
/// both `A, B and C` and `A, B, and C`. With no later word, the parent's
/// `BareCommaPolicy` decides whether punctuation has a meaning or is an error.
pub(in crate::oracle::canonical) fn recognize_contextual_list<T>(
    cursor: &mut OracleTokenCursor<'_>,
    policy: ContextualListPolicy<'_>,
    recognize_item: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<ContextualListNode<T>> {
    recognize_contextual_list_internal(cursor, policy, recognize_item, false)
}

/// A parent-delimited serial list leaves its final comma unconsumed. A comma
/// continues the list only if transactional lookahead reads another complete
/// item and an explicit accepted connector linking the serial run. The item
/// callback must not commit external bindings during this read-only lookahead.
pub(in crate::oracle::canonical) fn recognize_contextual_list_before_separator<T>(
    cursor: &mut OracleTokenCursor<'_>,
    policy: ContextualListPolicy<'_>,
    recognize_item: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<ContextualListNode<T>> {
    recognize_contextual_list_internal(cursor, policy, recognize_item, true)
}

fn recognize_contextual_list_internal<T>(
    cursor: &mut OracleTokenCursor<'_>,
    policy: ContextualListPolicy<'_>,
    mut recognize_item: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
    parent_owns_separator: bool,
) -> MachineResult<ContextualListNode<T>> {
    let mut state = ContextualListState::FirstItem;
    let mut items = Vec::new();
    let mut connectors: Vec<Option<ContextualListConnector>> = Vec::new();
    let mut proven_comma_run_end = 0;

    loop {
        state = match state {
            ContextualListState::FirstItem => {
                cursor.enter("contextual-list.first-item");
                items.push(recognize_item(cursor)?);
                ContextualListState::SeparatorOrComplete
            }
            ContextualListState::SeparatorOrComplete => {
                cursor.enter("contextual-list.separator-or-complete");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    if parent_owns_separator && cursor.position() >= proven_comma_run_end {
                        let Some(end) =
                            probe_explicit_comma_run(cursor, policy, &mut recognize_item)
                        else {
                            state = ContextualListState::Complete;
                            continue;
                        };
                        proven_comma_run_end = end;
                    }
                    cursor.consume(",")?;
                    connectors.push(None);
                    if let Some(connector) = cursor
                        .peek()
                        .and_then(ContextualListConnector::from_oracle_word)
                    {
                        cursor.require(
                            policy.accepts(connector),
                            "a connector accepted by the active list parent",
                        )?;
                        consume_connector(cursor)?;
                        resolve_pending_connectors(&mut connectors, connector);
                    }
                    ContextualListState::ItemAfterSeparator
                } else if let Some(connector) = cursor
                    .peek()
                    .and_then(ContextualListConnector::from_oracle_word)
                {
                    cursor.require(
                        policy.accepts(connector),
                        "a connector accepted by the active list parent",
                    )?;
                    consume_connector(cursor)?;
                    connectors.push(Some(connector));
                    resolve_pending_connectors(&mut connectors, connector);
                    ContextualListState::ItemAfterSeparator
                } else {
                    ContextualListState::Complete
                }
            }
            ContextualListState::ItemAfterSeparator => {
                cursor.enter("contextual-list.item-after-separator");
                items.push(recognize_item(cursor)?);
                ContextualListState::SeparatorOrComplete
            }
            ContextualListState::Complete => {
                cursor.enter("contextual-list.complete");
                if connectors.iter().any(Option::is_none) {
                    match policy.bare_comma {
                        BareCommaPolicy::RequireLaterConnector => {
                            return Err(cursor.fail(
                                "a parent-approved final connector after the serial commas",
                            ));
                        }
                        BareCommaPolicy::Use(connector) => {
                            cursor.require(
                                policy.accepts(connector),
                                "a bare-comma meaning accepted by the active list parent",
                            )?;
                            resolve_pending_connectors(&mut connectors, connector);
                        }
                    }
                }
                cursor.require(
                    items.len() == connectors.len() + 1,
                    "one contextual connector between each pair of list items",
                )?;
                return Ok(ContextualListNode {
                    items,
                    connectors: connectors
                        .into_iter()
                        .map(|connector| {
                            connector.expect("all contextual connectors resolved before lowering")
                        })
                        .collect(),
                });
            }
        };
    }
}

/// Returns the end of a proved explicit serial run. The borrowed token stream
/// is reused through a cursor fork, so neither source text nor tokens are rebuilt.
fn probe_explicit_comma_run<T>(
    cursor: &OracleTokenCursor<'_>,
    policy: ContextualListPolicy<'_>,
    recognize_item: &mut impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> Option<usize> {
    let mut probe = cursor.clone();
    probe.consume(",").ok()?;
    loop {
        if let Some(connector) = probe
            .peek()
            .and_then(ContextualListConnector::from_oracle_word)
        {
            if !policy.accepts(connector) {
                return None;
            }
            consume_connector(&mut probe).ok()?;
            recognize_item(&mut probe).ok()?;
            return Some(probe.position());
        }
        recognize_item(&mut probe).ok()?;
        if probe.peek() == Some(",") {
            probe.consume(",").ok()?;
        } else if probe
            .peek()
            .and_then(ContextualListConnector::from_oracle_word)
            .is_none()
        {
            return None;
        }
    }
}

fn resolve_pending_connectors(
    connectors: &mut [Option<ContextualListConnector>],
    connector: ContextualListConnector,
) {
    for pending in connectors.iter_mut().filter(|value| value.is_none()) {
        *pending = Some(connector);
    }
}

#[cfg(test)]
#[path = "contextual_list_tests.rs"]
mod tests;
