use super::super::{MachineResult, OracleTokenCursor};
use super::{SelectorPredicate, TargetSelector, recognize_target_slot_selector};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventObjectNode {
    pub predicate: SelectorPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TriggerCauseEventNode {
    EnterBattlefield { object: EventObjectNode },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SuppressTriggerCreationNode {
    pub caused_by: TriggerCauseEventNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectCollectionState {
    Branch,
    ConnectorOrComplete,
    Complete,
}

fn object_predicate_from_selector(
    cursor: &OracleTokenCursor<'_>,
    selector: TargetSelector,
) -> MachineResult<SelectorPredicate> {
    match selector {
        TargetSelector::Object(predicate) => Ok(predicate),
        TargetSelector::ControlledObject { .. } => {
            Err(cursor.fail("an entering-object selector without a controller relation"))
        }
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => Err(cursor.fail("an entering-object card-type selector")),
    }
}

/// Recognizes the object collection that causes one event. In a collection
/// context, `and` joins selector branches as a union rather than expressing
/// target multiplicity or predicate intersection.
fn recognize_entering_object_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    let mut state = ObjectCollectionState::Branch;
    let mut branches = Vec::new();
    loop {
        state = match state {
            ObjectCollectionState::Branch => {
                cursor.enter("event-object.collection.branch");
                let selector = recognize_target_slot_selector(cursor)?;
                branches.push(object_predicate_from_selector(cursor, selector)?);
                ObjectCollectionState::ConnectorOrComplete
            }
            ObjectCollectionState::ConnectorOrComplete => {
                cursor.enter("event-object.collection.connector-or-complete");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume("and")?;
                    ObjectCollectionState::Branch
                } else {
                    ObjectCollectionState::Complete
                }
            }
            ObjectCollectionState::Complete => {
                cursor.enter("event-object.collection.complete");
                return match branches.len() {
                    0 => Err(cursor.fail("one or more entering-object selector branches")),
                    1 => Ok(branches.pop().expect("one selector branch")),
                    _ => Ok(SelectorPredicate::Union(branches)),
                };
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerSuppressionState {
    EventObject,
    Event,
    OptionalBattlefield,
    Negation,
    Causation,
    AbilityNoun,
    Infinitive,
    TriggerAction,
    Terminator,
    Complete,
}

/// Recognizes a static rule that suppresses trigger creation based on the
/// event's entering object. The machine emits an event predicate, not a card-
/// specific instruction, so every supported object selector shares the same
/// engine modifier.
pub(in crate::oracle::canonical) fn recognize_trigger_creation_suppression(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SuppressTriggerCreationNode> {
    let mut state = TriggerSuppressionState::EventObject;
    let mut event_object = None;
    loop {
        state = match state {
            TriggerSuppressionState::EventObject => {
                cursor.enter("static.trigger-suppression.event-object");
                event_object = Some(EventObjectNode {
                    predicate: recognize_entering_object_collection(cursor)?,
                });
                TriggerSuppressionState::Event
            }
            TriggerSuppressionState::Event => {
                cursor.enter("static.trigger-suppression.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("entering"),
                    "the `entering` event",
                )?;
                TriggerSuppressionState::OptionalBattlefield
            }
            TriggerSuppressionState::OptionalBattlefield => {
                cursor.enter("static.trigger-suppression.optional-battlefield");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
                {
                    cursor.consume_phrase(&["the", "battlefield"])?;
                }
                TriggerSuppressionState::Negation
            }
            TriggerSuppressionState::Negation => {
                cursor.enter("static.trigger-suppression.negation");
                cursor.consume("don't")?;
                TriggerSuppressionState::Causation
            }
            TriggerSuppressionState::Causation => {
                cursor.enter("static.trigger-suppression.causation");
                cursor.consume_type("causationWord")?;
                TriggerSuppressionState::AbilityNoun
            }
            TriggerSuppressionState::AbilityNoun => {
                cursor.enter("static.trigger-suppression.ability-noun");
                cursor.consume("abilities")?;
                TriggerSuppressionState::Infinitive
            }
            TriggerSuppressionState::Infinitive => {
                cursor.enter("static.trigger-suppression.infinitive");
                cursor.consume("to")?;
                TriggerSuppressionState::TriggerAction
            }
            TriggerSuppressionState::TriggerAction => {
                cursor.enter("static.trigger-suppression.trigger-action");
                cursor.consume_type("triggerActionWord")?;
                TriggerSuppressionState::Terminator
            }
            TriggerSuppressionState::Terminator => {
                cursor.enter("static.trigger-suppression.terminator");
                cursor.consume(".")?;
                TriggerSuppressionState::Complete
            }
            TriggerSuppressionState::Complete => {
                cursor.enter("static.trigger-suppression.complete");
                cursor.finish()?;
                return Ok(SuppressTriggerCreationNode {
                    caused_by: TriggerCauseEventNode::EnterBattlefield {
                        object: cursor.require_value(event_object, "an entering event object")?,
                    },
                });
            }
        };
    }
}
