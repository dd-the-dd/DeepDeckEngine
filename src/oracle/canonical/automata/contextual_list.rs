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
        } else if raw.eq_ignore_ascii_case("or") {
            Some(Self::Or)
        } else if raw.eq_ignore_ascii_case("then") {
            Some(Self::Then)
        } else {
            None
        }
    }

    fn oracle_word(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
            Self::Then => "then",
        }
    }
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
    mut recognize_item: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<ContextualListNode<T>> {
    let mut state = ContextualListState::FirstItem;
    let mut items = Vec::new();
    let mut connectors: Vec<Option<ContextualListConnector>> = Vec::new();

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
                        cursor.consume(connector.oracle_word())?;
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
                    cursor.consume(connector.oracle_word())?;
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
