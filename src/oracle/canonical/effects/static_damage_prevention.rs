//! Static damage prevention (CR 615.1a, 604.1): a prevention effect generated
//! by a permanent's static ability, applying while that permanent is on the
//! battlefield, as opposed to a resolving spell's shield that lasts a turn.
//!
//! ```text
//! Prevention := "prevent" "all" ["combat"] "damage" "that" "would" "be" "dealt"
//!               "to" Subject ["by" Source] "." [reminder]
//! Subject    := the shared static subject (source, attachment, collection)
//! Source     := an object collection of damage sources
//! ```
//!
//! The recipient is the same subject the static continuous frame reads; the
//! source is the shared object collection. Each lowers to an engine modifier
//! only when the engine evaluates that exact scope: every damage or combat
//! damage to a selected permanent (`preventDamage`), and damage from creature
//! sources to the ability's own source (`preventCreatureDamageToSelf`). Any
//! other scope is an explicit failure inside this frame.
use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticDamagePreventionAttempt {
    Parsed(CanonicalRuleDraft),
    /// `prevent` opened the static frame and its inner grammar stopped.
    Failed(MachineFailure),
    NotClaimed,
}

pub(in crate::oracle::canonical) fn try_parse_static_damage_prevention(
    input: &AbilityInput<'_>,
) -> StaticDamagePreventionAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticDamagePreventionAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-damage-prevention", input.tokens);
    if recognize_optional_ability_word_envelope(&mut cursor).is_err()
        || !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("prevent"))
    {
        return StaticDamagePreventionAttempt::NotClaimed;
    }
    match read_static_damage_prevention(&mut cursor, input) {
        Ok(modifier) => StaticDamagePreventionAttempt::Parsed(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [modifier],
            }),
            &[
                "Read the prevented damage kind and its recipient with the shared static subject",
                "Read an optional damage source with the shared object collection",
                "Lower to one battlefield-active prevention modifier (CR 615.1a)",
            ],
        )),
        Err(failure) => StaticDamagePreventionAttempt::Failed(failure),
    }
}

fn read_static_damage_prevention(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> MachineResult<Value> {
    cursor.enter("static.prevent.action");
    cursor.consume("prevent")?;
    cursor.enter("static.prevent.amount");
    cursor.consume("all")?;
    cursor.enter("static.prevent.damage-kind");
    let combat_only = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("combat"));
    if combat_only {
        cursor.consume("combat")?;
    }
    cursor.consume("damage")?;
    cursor.enter("static.prevent.frame");
    cursor.consume("that")?;
    recognize_relative_damage_clause(cursor, Voice::Passive)?;
    cursor.consume("to")?;
    cursor.enter("static.prevent.recipient");
    cursor.require(
        !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("you") || raw.eq_ignore_ascii_case("and")),
        "a permanent recipient (a player recipient or a recipient-and-source scope is not a supported static prevention)",
    )?;
    let (objects, _, _) = read_static_subject(cursor, input)?;
    let mut source = None;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("by"))
    {
        cursor.consume("by")?;
        cursor.enter("static.prevent.source");
        let objects = recognize_object_collection(cursor)?;
        cursor.require(
            !objects.exclude_source,
            "a damage-source collection without a source exclusion",
        )?;
        source = Some(objects);
    }
    cursor.enter("static.prevent.terminator");
    cursor.consume(".")?;
    recognize_optional_trailing_reminder(cursor)?;
    cursor.enter("static.prevent.complete");
    cursor.finish()?;
    cursor.enter("static.prevent.lowering");
    match source {
        None => {
            let mut modifier = json!({ "kind": "preventDamage", "objects": objects });
            if combat_only {
                modifier["combatOnly"] = json!(true);
            }
            Ok(modifier)
        }
        Some(query) => {
            cursor.require(
                !combat_only
                    && objects == self_ref()
                    && query.controller.is_none()
                    && query.predicate == ObjectPredicate::CardType(ObjectCardType::Creature),
                "damage from creature sources to the ability's own source (the engine evaluates no other source-scoped static prevention)",
            )?;
            Ok(json!({ "kind": "preventCreatureDamageToSelf", "object": objects }))
        }
    }
}

/// One balanced trailing parenthetical after the sentence (CR 207.2).
fn recognize_optional_trailing_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    if cursor.peek() != Some("(") {
        return Ok(());
    }
    cursor.enter("static.prevent.reminder");
    cursor.consume("(")?;
    while cursor.peek().is_some_and(|raw| raw != ")" && raw != "(") {
        cursor.advance_to(cursor.position() + 1)?;
    }
    cursor.consume(")")
}

/// The committed failure of the static prevention frame, for the frame claim.
pub(in crate::oracle::canonical) fn static_prevention_frame_failure(
    input: &AbilityInput<'_>,
    start: usize,
) -> MachineFailure {
    match try_parse_static_damage_prevention(input) {
        StaticDamagePreventionAttempt::Failed(failure) => failure,
        StaticDamagePreventionAttempt::Parsed(_) | StaticDamagePreventionAttempt::NotClaimed => {
            let mut cursor = OracleTokenCursor::new("static-damage-prevention", input.tokens);
            cursor.enter("static.prevent.frame-owner");
            let _ = cursor.advance_to(start.min(input.tokens.len()));
            cursor.fail("a damage prevention generated by a permanent's static ability (CR 615.1a)")
        }
    }
}

#[cfg(test)]
#[path = "static_damage_prevention_tests.rs"]
mod tests;
