use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectSequenceNode, ObjectControllerScope, ObjectKind, ObjectPredicate,
    recognize_effect_sequence, recognize_object_query,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetedObjectDomain {
    Permanent,
    Spell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedObjectSelectorNode {
    pub domain: TargetedObjectDomain,
    pub predicate: ObjectPredicate,
    pub controller: ObjectControllerScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetingStackKind {
    Spell,
    ActivatedAbility,
    TriggeredAbility,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetingStackSelectorNode {
    pub kinds: Vec<TargetingStackKind>,
    pub controller: ObjectControllerScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecameTargetEventNode {
    pub objects: Vec<TargetedObjectSelectorNode>,
    pub targeting_stack: TargetingStackSelectorNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecameTargetTriggeredAbilityNode {
    pub event: BecameTargetEventNode,
    pub effects: EffectSequenceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubjectState {
    Article,
    Selector,
    ConnectorOrEvent,
    Complete,
}

fn recognize_targeted_object_selectors(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<TargetedObjectSelectorNode>> {
    let mut state = SubjectState::Article;
    let mut selectors = Vec::new();
    loop {
        state = match state {
            SubjectState::Article => {
                cursor.enter("event.became-target.subject.article");
                let article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .ok_or_else(|| cursor.fail("`a` or `an` before a targeted-object selector"))?
                    .to_string();
                cursor.consume(&article)?;
                SubjectState::Selector
            }
            SubjectState::Selector => {
                cursor.enter("event.became-target.subject.selector");
                let query = recognize_object_query(cursor)?;
                let domain = if query.predicate.has_domain(ObjectKind::Spell) {
                    cursor.require(
                        query.predicate.supports_domain(ObjectKind::Spell),
                        "a predicate valid for spells",
                    )?;
                    TargetedObjectDomain::Spell
                } else {
                    cursor.require(
                        query.denotes_battlefield_permanents(),
                        "a predicate establishing a permanent or spell domain",
                    )?;
                    TargetedObjectDomain::Permanent
                };
                selectors.push(TargetedObjectSelectorNode {
                    domain,
                    predicate: query.predicate,
                    controller: cursor.require_value(
                        query.controller,
                        "a controller qualifier for the targeted object",
                    )?,
                });
                SubjectState::ConnectorOrEvent
            }
            SubjectState::ConnectorOrEvent => {
                cursor.enter("event.became-target.subject.connector-or-event");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    SubjectState::Article
                } else {
                    SubjectState::Complete
                }
            }
            SubjectState::Complete => {
                cursor.enter("event.became-target.subject.complete");
                cursor.require(
                    !selectors.is_empty(),
                    "at least one targeted-object selector",
                )?;
                return Ok(selectors);
            }
        };
    }
}

fn recognize_controller_scope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectControllerScope> {
    cursor.enter("event.became-target.source-controller.subject");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        cursor.consume("you")?;
        cursor.enter("event.became-target.source-controller.relation");
        cursor.consume("control")?;
        return Ok(ObjectControllerScope::AbilityController);
    }
    cursor.consume("an")?;
    cursor.enter("event.became-target.source-controller.opponent");
    cursor.consume("opponent")?;
    cursor.enter("event.became-target.source-controller.relation");
    cursor.consume("controls")?;
    Ok(ObjectControllerScope::OpponentOfAbilityController)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackSourceState {
    Article,
    Kind,
    ConnectorOrController,
    Controller,
    Complete,
}

fn recognize_targeting_stack_selector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetingStackSelectorNode> {
    let mut state = StackSourceState::Article;
    let mut kinds = Vec::new();
    let mut controller = None;
    loop {
        state = match state {
            StackSourceState::Article => {
                cursor.enter("event.became-target.stack-source.article");
                let article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .ok_or_else(|| cursor.fail("`a` or `an` before a stack-source kind"))?
                    .to_string();
                cursor.consume(&article)?;
                StackSourceState::Kind
            }
            StackSourceState::Kind => {
                cursor.enter("event.became-target.stack-source.kind");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("spell") => {
                        cursor.consume("spell")?;
                        kinds.push(TargetingStackKind::Spell);
                    }
                    Some("ability") => {
                        cursor.consume("ability")?;
                        kinds.push(TargetingStackKind::ActivatedAbility);
                        kinds.push(TargetingStackKind::TriggeredAbility);
                    }
                    _ => return Err(cursor.fail("a spell or ability stack-source kind")),
                }
                StackSourceState::ConnectorOrController
            }
            StackSourceState::ConnectorOrController => {
                cursor.enter("event.became-target.stack-source.connector-or-controller");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    StackSourceState::Kind
                } else {
                    StackSourceState::Controller
                }
            }
            StackSourceState::Controller => {
                controller = Some(recognize_controller_scope(cursor)?);
                StackSourceState::Complete
            }
            StackSourceState::Complete => {
                cursor.enter("event.became-target.stack-source.complete");
                kinds.sort_by_key(|kind| match kind {
                    TargetingStackKind::Spell => 0,
                    TargetingStackKind::ActivatedAbility => 1,
                    TargetingStackKind::TriggeredAbility => 2,
                });
                kinds.dedup();
                cursor.require(
                    !kinds.is_empty(),
                    "at least one targeting stack-source kind",
                )?;
                return Ok(TargetingStackSelectorNode {
                    kinds,
                    controller: cursor
                        .require_value(controller, "a targeting source controller")?,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_became_target_triggered_ability(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<BecameTargetTriggeredAbilityNode> {
    cursor.enter("trigger.became-target.envelope");
    cursor.consume("whenever")?;
    let objects = recognize_targeted_object_selectors(cursor)?;
    cursor.enter("event.became-target.action");
    cursor.consume("becomes")?;
    cursor.enter("event.became-target.relation");
    cursor.consume_phrase(&["the", "target", "of"])?;
    let targeting_stack = recognize_targeting_stack_selector(cursor)?;
    cursor.enter("trigger.became-target.separator");
    cursor.consume(",")?;
    let effects = recognize_effect_sequence(cursor, name_links, name_failure)?;
    Ok(BecameTargetTriggeredAbilityNode {
        event: BecameTargetEventNode {
            objects,
            targeting_stack,
        },
        effects,
    })
}

#[cfg(test)]
#[path = "targeting_event_tests.rs"]
mod tests;
