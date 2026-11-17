use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectCastConditionNode, ObjectCastStatus, ObjectKind, ReplacementDestination,
    ReplacementObjectBinding, WouldEnterBattlefieldEventNode, ZoneReplacementNode,
    recognize_object_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WouldEnterState {
    Article,
    Object,
    Modal,
    Event,
    OptionalDestination,
    Complete,
}

/// Recognizes `a|an OBJECT would enter [the battlefield]` as a replacement
/// event. The surrounding replacement envelope owns the leading `if`.
pub(in crate::oracle::canonical) fn recognize_would_enter_battlefield_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<WouldEnterBattlefieldEventNode> {
    let mut state = WouldEnterState::Article;
    let mut object = None;
    loop {
        state = match state {
            WouldEnterState::Article => {
                cursor.enter("replacement.event.would-enter.article");
                let article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .ok_or_else(|| cursor.fail("`a` or `an` before an entering object"))?
                    .to_string();
                cursor.consume(&article)?;
                WouldEnterState::Object
            }
            WouldEnterState::Object => {
                cursor.enter("replacement.event.would-enter.object");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller.is_none(),
                    "an entering-object predicate without a controller qualifier",
                )?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent)
                        && query.predicate.has_domain(ObjectKind::Permanent),
                    "a permanent object predicate",
                )?;
                object = Some(query.predicate);
                WouldEnterState::Modal
            }
            WouldEnterState::Modal => {
                cursor.enter("replacement.event.would-enter.modal");
                cursor.consume("would")?;
                WouldEnterState::Event
            }
            WouldEnterState::Event => {
                cursor.enter("replacement.event.would-enter.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(event.raw.eq_ignore_ascii_case("enter"), "the `enter` event")?;
                WouldEnterState::OptionalDestination
            }
            WouldEnterState::OptionalDestination => {
                cursor.enter("replacement.event.would-enter.optional-destination");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
                {
                    cursor.consume_phrase(&["the", "battlefield"])?;
                }
                WouldEnterState::Complete
            }
            WouldEnterState::Complete => {
                cursor.enter("replacement.event.would-enter.complete");
                return Ok(WouldEnterBattlefieldEventNode {
                    object: cursor.require_value(object, "an entering-object predicate")?,
                    binding: ReplacementObjectBinding::EventObject,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotCastState {
    Connector,
    Object,
    Predicate,
    Complete,
}

/// Recognizes `and it wasn't cast`, retaining the parent event's object
/// binding instead of reparsing or guessing the anaphor.
pub(in crate::oracle::canonical) fn recognize_object_not_cast_condition(
    cursor: &mut OracleTokenCursor<'_>,
    object: ReplacementObjectBinding,
) -> MachineResult<ObjectCastConditionNode> {
    let mut state = NotCastState::Connector;
    loop {
        state = match state {
            NotCastState::Connector => {
                cursor.enter("replacement.condition.not-cast.connector");
                cursor.consume("and")?;
                NotCastState::Object
            }
            NotCastState::Object => {
                cursor.enter("replacement.condition.not-cast.object");
                cursor.consume("it")?;
                NotCastState::Predicate
            }
            NotCastState::Predicate => {
                cursor.enter("replacement.condition.not-cast.predicate");
                let predicate = cursor.consume_type("negativePredicateWord")?;
                cursor.require(
                    predicate.raw.eq_ignore_ascii_case("wasn't")
                        || predicate.raw.eq_ignore_ascii_case("wasn’t"),
                    "the `wasn't` cast-status predicate",
                )?;
                cursor.consume("cast")?;
                NotCastState::Complete
            }
            NotCastState::Complete => {
                cursor.enter("replacement.condition.not-cast.complete");
                return Ok(ObjectCastConditionNode {
                    object,
                    status: ObjectCastStatus::NotCast,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExileInsteadState {
    Separator,
    Action,
    Object,
    Instead,
    Complete,
}

/// Recognizes `, exile it instead` as a destination replacement over the
/// object bound by a preceding event.
pub(in crate::oracle::canonical) fn recognize_exile_instead_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    object: ReplacementObjectBinding,
) -> MachineResult<ZoneReplacementNode> {
    let mut state = ExileInsteadState::Separator;
    loop {
        state = match state {
            ExileInsteadState::Separator => {
                cursor.enter("replacement.zone.separator");
                cursor.consume(",")?;
                ExileInsteadState::Action
            }
            ExileInsteadState::Action => {
                cursor.enter("replacement.zone.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("exile"),
                    "the `exile` action",
                )?;
                ExileInsteadState::Object
            }
            ExileInsteadState::Object => {
                cursor.enter("replacement.zone.object");
                cursor.consume("it")?;
                ExileInsteadState::Instead
            }
            ExileInsteadState::Instead => {
                cursor.enter("replacement.zone.instead");
                cursor.consume("instead")?;
                ExileInsteadState::Complete
            }
            ExileInsteadState::Complete => {
                cursor.enter("replacement.zone.complete");
                return Ok(ZoneReplacementNode {
                    object,
                    destination: ReplacementDestination::Exile,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "entry_replacement_tests.rs"]
mod tests;
