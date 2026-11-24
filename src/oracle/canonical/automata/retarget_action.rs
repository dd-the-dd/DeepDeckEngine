//! Stack-target reassignment is one ordinary action, not a copy program.
//!
//! ```text
//! Retarget := [PLAYER] "choose" "new" "targets" "for" STACK-TARGET
//!           | ["you" ["may"]] "change" HOW "of" STACK-TARGET
//!             ["with" "a" "single" "target"] ["to" SOURCE]
//! HOW      := "the" ("target" | "targets")        -- CR 115.7a
//!           | "a" "target"                         -- CR 115.7b
//! ```
//!
//! The stack target is the shared target list with its stack-domain
//! selector. `with a single target` restricts that declared stack object to
//! one with exactly one target; it belongs to the declaration, so a spell
//! with two targets is never a legal target. `to SOURCE` names the one new
//! target the changed target must become. A text change (`change the text
//! of ...`) is another action and is not claimed.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

/// Which complete target designations the reassignment admits (CR 115.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RetargetMode {
    /// `choose new targets for` (CR 115.7d): any target may be kept.
    ChooseNew,
    /// `change the target(s) of` (CR 115.7a): every target changes to
    /// another legal target, or none does.
    ChangeAll,
    /// `change a target of` (CR 115.7b): exactly one target changes.
    ChangeOne,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RetargetActionNode {
    pub player: LocalPlayerReference,
    pub target: TargetListNode,
    pub mode: RetargetMode,
    /// `with a single target`: the declared stack object has one target.
    pub single_target: bool,
    /// `to this creature`: the new target of the changed target.
    pub forced: Option<LocalObjectReference>,
    /// `you may change`: the chooser may leave the designation unchanged.
    pub optional: bool,
}

pub(in crate::oracle::canonical) enum RetargetActionAttempt {
    NotClaimed,
    Recognized(RetargetActionNode),
    FailedCommitted(MachineFailure),
}

#[derive(Clone, Copy)]
enum State {
    Actor,
    Choose,
    New,
    Targets,
    For,
    Change,
    Designation,
    Of,
    Object,
    SingleTarget,
    NewTarget,
    Complete,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The head of a reassignment: (`you` / `you may` width, the verb kind).
fn retarget_head(cursor: &OracleTokenCursor<'_>) -> Option<(usize, bool)> {
    let actor = if word_at(cursor, 0, "you") && word_at(cursor, 1, "may") {
        2
    } else {
        usize::from(word_at(cursor, 0, "you"))
    };
    if word_at(cursor, actor, "choose") && word_at(cursor, actor + 1, "new") && actor != 2 {
        return Some((actor, false));
    }
    // `change the target(s) of` / `change a target of`; `change the text
    // of` is a text change, not a reassignment.
    let designation = (word_at(cursor, actor + 1, "the")
        && (word_at(cursor, actor + 2, "target") || word_at(cursor, actor + 2, "targets")))
        || (word_at(cursor, actor + 1, "a") && word_at(cursor, actor + 2, "target"));
    (word_at(cursor, actor, "change") && designation && word_at(cursor, actor + 3, "of"))
        .then_some((actor, true))
}

/// Absolute position of `with a single target` after the stack target,
/// within the current sentence.
fn single_target_qualifier_at(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if matches!(raw, "." | "," | ";" | ":") {
            return None;
        }
        if raw.eq_ignore_ascii_case("with")
            && word_at(cursor, offset + 1, "a")
            && word_at(cursor, offset + 2, "single")
            && word_at(cursor, offset + 3, "target")
        {
            return Some(cursor.position() + offset);
        }
        offset += 1;
    }
    None
}

pub(in crate::oracle::canonical) fn try_recognize_retarget_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> RetargetActionAttempt {
    let Some((actor_width, change)) = retarget_head(cursor) else {
        return RetargetActionAttempt::NotClaimed;
    };
    let mut trial = cursor.clone();
    let result = (|| -> MachineResult<RetargetActionNode> {
        let mut state = State::Actor;
        let mut player = None;
        let mut target = None;
        let mut mode = RetargetMode::ChooseNew;
        let mut single_target = false;
        let mut forced = None;
        let mut optional = false;
        loop {
            state = match state {
                State::Actor => {
                    trial.enter("effect.retarget.actor");
                    player = Some(if actor_width == 0 {
                        LocalPlayerReference::Controller
                    } else {
                        recognize_player_reference(&mut trial, None)?
                    });
                    if actor_width == 2 {
                        trial.enter("effect.retarget.permission");
                        trial.consume("may")?;
                        optional = true;
                    }
                    if change { State::Change } else { State::Choose }
                }
                State::Choose => {
                    trial.enter("effect.retarget.choose");
                    trial.consume("choose")?;
                    State::New
                }
                State::New => {
                    trial.enter("effect.retarget.new");
                    trial.consume("new")?;
                    State::Targets
                }
                State::Targets => {
                    trial.enter("effect.retarget.targets");
                    trial.consume("targets")?;
                    State::For
                }
                State::For => {
                    trial.enter("effect.retarget.for");
                    trial.consume("for")?;
                    State::Object
                }
                State::Change => {
                    trial.enter("effect.retarget.change");
                    trial.consume("change")?;
                    State::Designation
                }
                State::Designation => {
                    trial.enter("effect.retarget.designation");
                    mode = if word_at(&trial, 0, "a") {
                        trial.consume_phrase(&["a", "target"])?;
                        RetargetMode::ChangeOne
                    } else {
                        trial.consume("the")?;
                        let noun = trial.peek().unwrap_or_default().to_string();
                        trial.require(
                            noun.eq_ignore_ascii_case("target")
                                || noun.eq_ignore_ascii_case("targets"),
                            "`the target` or `the targets`",
                        )?;
                        trial.consume(&noun)?;
                        RetargetMode::ChangeAll
                    };
                    State::Of
                }
                State::Of => {
                    trial.enter("effect.retarget.of");
                    trial.consume("of")?;
                    State::Object
                }
                State::Object => {
                    trial.enter("effect.retarget.object");
                    // `with a single target` qualifies the declared stack
                    // object; the target list ends before it.
                    let qualifier = if mode == RetargetMode::ChooseNew {
                        None
                    } else {
                        single_target_qualifier_at(&trial)
                    };
                    let slots = match qualifier {
                        Some(end) => trial.bounded(end, recognize_target_list)?,
                        None => recognize_target_list(&mut trial)?,
                    };
                    trial.enter("effect.retarget.stack-domain");
                    trial.require(slots.slots.len() == 1, "one target stack-object slot")?;
                    let slot = &slots.slots[0];
                    trial.require(
                        slot.cardinality == TargetCardinality::EXACTLY_ONE,
                        "one required stack target",
                    )?;
                    trial.require(
                        stack_domain_selector_supported(&slot.selector),
                        "a spell or stack-ability selector",
                    )?;
                    target = Some(slots);
                    if qualifier.is_some() {
                        State::SingleTarget
                    } else {
                        State::NewTarget
                    }
                }
                State::SingleTarget => {
                    trial.enter("effect.retarget.single-target");
                    trial.consume_phrase(&["with", "a", "single", "target"])?;
                    single_target = true;
                    State::NewTarget
                }
                State::NewTarget => {
                    trial.enter("effect.retarget.new-target");
                    if mode != RetargetMode::ChooseNew && word_at(&trial, 0, "to") {
                        trial.consume("to")?;
                        forced = Some(recognize_source_reference(&mut trial, &[], None)?);
                        // CR 115.7a: with a fixed new object, `change the
                        // target` of a spell with one target changes it to
                        // that object; several targets need `a target`.
                        trial.require(
                            mode == RetargetMode::ChangeOne || single_target,
                            "`change a target` or a single-target stack object before a fixed new target",
                        )?;
                    }
                    State::Complete
                }
                State::Complete => {
                    trial.enter("effect.retarget.complete");
                    return Ok(RetargetActionNode {
                        player: trial.require_value(player, "a populated choosing actor")?,
                        target: trial.require_value(target, "a populated stack-object target")?,
                        mode,
                        single_target,
                        forced,
                        optional,
                    });
                }
            };
        }
    })();
    match result {
        Ok(node) => {
            *cursor = trial;
            RetargetActionAttempt::Recognized(node)
        }
        Err(failure) => RetargetActionAttempt::FailedCommitted(failure),
    }
}

#[cfg(test)]
#[path = "retarget_action_tests.rs"]
mod tests;
