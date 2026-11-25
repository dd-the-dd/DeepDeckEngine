//! Static damage prevention (CR 615.1a, 604.1): a prevention effect generated
//! by a permanent's static ability, applying while that permanent is on the
//! battlefield, as opposed to a resolving spell's shield that lasts a turn.
//!
//! ```text
//! Prevention := "prevent" "all" [Kind] "damage" "that" "would" "be" "dealt"
//!               Scope "." [reminder]
//! Kind       := "combat" | "noncombat"                          CR 120.2
//! Scope      := "to" Recipients ["by" Sources]
//!             | "to" "and" "dealt" "by" Subject   -- both scopes of one subject
//!             | "by" Subject                       -- any recipient
//! Recipients := Recipient ("and" Recipient)*
//! Recipient  := "you" | Subject
//! Subject    := the shared static subject (source, attachment, collection)
//! Sources    := an object collection of damage sources
//! ```
//!
//! The prevention is one filter over the damage event (CR 615.1a): its
//! damage kind, its recipient and its source are independent slots. The
//! historical shapes keep their engine modifiers (`preventDamage` on
//! permanents, `preventPlayerDamage` on the controller,
//! `preventCreatureDamageToSelf`); every other combination lowers to one
//! `preventScopedDamage` filter per recipient, which the engine evaluates
//! generically (`engine/static_damage_prevention.rs`). A source that is not
//! a battlefield permanent (a spell) is an explicit engine gap.
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
        Ok(modifiers) => StaticDamagePreventionAttempt::Parsed(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": modifiers,
            }),
            &[
                "Read the prevented damage kind, its recipients and its sources as independent slots",
                "Read recipients and sources with the shared static subject and object collection",
                "Lower each recipient/source scope to one battlefield-active prevention filter (CR 615.1a)",
            ],
        )),
        Err(failure) => StaticDamagePreventionAttempt::Failed(failure),
    }
}

/// The damage kind a prevention is restricted to (CR 120.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreventedDamageKind {
    Any,
    Combat,
    Noncombat,
}

/// Who is dealt the prevented damage.
#[derive(Clone, Debug, PartialEq)]
enum PreventionRecipient {
    /// The ability's controller (`you`).
    Controller,
    /// A static object subject.
    Objects(Value),
}

/// The damage sources of a scope: a collection read after `to … by`, whose
/// query keeps the historical creature-source shape, or a static subject
/// read after `dealt by`.
#[derive(Clone, Debug)]
enum PreventionSources {
    Collection(ObjectQueryNode),
    Subject(Value),
}

/// One recipient/source pair of the prevention filter; an absent slot
/// matches every recipient or source.
#[derive(Clone, Debug)]
struct PreventionScope {
    recipient: Option<PreventionRecipient>,
    sources: Option<PreventionSources>,
}

fn read_static_damage_prevention(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> MachineResult<Vec<Value>> {
    cursor.enter("static.prevent.action");
    cursor.consume("prevent")?;
    cursor.enter("static.prevent.amount");
    cursor.consume("all")?;
    cursor.enter("static.prevent.damage-kind");
    let kind = if word_is(cursor, 0, "combat") {
        cursor.consume("combat")?;
        PreventedDamageKind::Combat
    } else if word_is(cursor, 0, "noncombat") {
        cursor.consume("noncombat")?;
        PreventedDamageKind::Noncombat
    } else {
        PreventedDamageKind::Any
    };
    cursor.consume("damage")?;
    cursor.enter("static.prevent.frame");
    cursor.consume("that")?;
    recognize_relative_damage_clause(cursor, Voice::Passive)?;
    let scopes = read_prevention_scopes(cursor, input)?;
    cursor.enter("static.prevent.terminator");
    cursor.consume(".")?;
    recognize_optional_trailing_reminder(cursor)?;
    cursor.enter("static.prevent.complete");
    cursor.finish()?;
    cursor.enter("static.prevent.lowering");
    scopes
        .into_iter()
        .map(|scope| lower_prevention_scope(cursor, kind, scope))
        .collect()
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The recipient and source slots after `would be dealt`.
fn read_prevention_scopes(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> MachineResult<Vec<PreventionScope>> {
    if word_is(cursor, 0, "to") && word_is(cursor, 1, "and") {
        // `dealt to and dealt by SUBJECT`: one subject fills the recipient
        // slot of one scope and the source slot of another (CR 615.1a).
        cursor.enter("static.prevent.to-and-by");
        cursor.consume_phrase(&["to", "and", "dealt", "by"])?;
        cursor.enter("static.prevent.recipient");
        let (objects, _, _) = read_static_subject(cursor, input)?;
        return Ok(vec![
            PreventionScope {
                recipient: Some(PreventionRecipient::Objects(objects.clone())),
                sources: None,
            },
            PreventionScope {
                recipient: None,
                sources: Some(PreventionSources::Subject(objects)),
            },
        ]);
    }
    if word_is(cursor, 0, "by") {
        // `dealt by SUBJECT`: every recipient of that source's damage.
        cursor.enter("static.prevent.source");
        cursor.consume("by")?;
        let (objects, _, _) = read_static_subject(cursor, input)?;
        return Ok(vec![PreventionScope {
            recipient: None,
            sources: Some(PreventionSources::Subject(objects)),
        }]);
    }
    cursor.enter("static.prevent.recipient");
    cursor.consume("to")?;
    let mut recipients = Vec::new();
    loop {
        cursor.enter("static.prevent.recipient");
        if word_is(cursor, 0, "you") {
            cursor.consume("you")?;
            recipients.push(PreventionRecipient::Controller);
        } else {
            let (objects, _, _) = read_static_subject(cursor, input)?;
            recipients.push(PreventionRecipient::Objects(objects));
        }
        if word_is(cursor, 0, "and") {
            cursor.consume("and")?;
            continue;
        }
        break;
    }
    let mut sources = None;
    if word_is(cursor, 0, "by") {
        cursor.consume("by")?;
        cursor.enter("static.prevent.source");
        let objects = recognize_object_collection(cursor)?;
        cursor.require(
            !objects.exclude_source,
            "a damage-source collection without a source exclusion",
        )?;
        sources = Some(PreventionSources::Collection(objects));
    }
    Ok(recipients
        .into_iter()
        .map(|recipient| PreventionScope {
            recipient: Some(recipient),
            sources: sources.clone(),
        })
        .collect())
}

/// The historical modifier of a scope when one exists, otherwise the
/// generic `preventScopedDamage` filter.
fn lower_prevention_scope(
    cursor: &mut OracleTokenCursor<'_>,
    kind: PreventedDamageKind,
    scope: PreventionScope,
) -> MachineResult<Value> {
    match (&scope.recipient, &scope.sources, kind) {
        (Some(PreventionRecipient::Objects(objects)), None, PreventedDamageKind::Any) => {
            return Ok(json!({ "kind": "preventDamage", "objects": objects }));
        }
        (Some(PreventionRecipient::Objects(objects)), None, PreventedDamageKind::Combat) => {
            return Ok(json!({ "kind": "preventDamage", "objects": objects, "combatOnly": true }));
        }
        (Some(PreventionRecipient::Controller), None, PreventedDamageKind::Any) => {
            return Ok(json!({ "kind": "preventPlayerDamage", "player": controller() }));
        }
        (
            Some(PreventionRecipient::Objects(objects)),
            Some(PreventionSources::Collection(query)),
            PreventedDamageKind::Any,
        ) if *objects == self_ref()
            && query.controller.is_none()
            && query.predicate == ObjectPredicate::CardType(ObjectCardType::Creature) =>
        {
            return Ok(json!({ "kind": "preventCreatureDamageToSelf", "object": objects }));
        }
        _ => {}
    }
    let mut modifier = json!({
        "kind": "preventScopedDamage",
        "damage": match kind {
            PreventedDamageKind::Any => "any",
            PreventedDamageKind::Combat => "combat",
            PreventedDamageKind::Noncombat => "noncombat",
        },
    });
    match scope.recipient {
        None => {}
        Some(PreventionRecipient::Controller) => {
            modifier["recipients"] = json!({ "player": controller() });
        }
        Some(PreventionRecipient::Objects(objects)) => {
            modifier["recipients"] = json!({ "objects": objects });
        }
    }
    match scope.sources {
        None => {}
        Some(PreventionSources::Subject(objects)) => modifier["sources"] = objects,
        Some(PreventionSources::Collection(query)) => {
            if !query.denotes_battlefield_permanents() {
                return Err(cursor.engine_gap(
                    crate::oracle::canonical::engine_gap::EngineGap::SourceScopedPrevention,
                    "damage sources that are battlefield permanents (a spell source is not evaluated by a static prevention)",
                ));
            }
            modifier["sources"] = lower_static_permanent_query(cursor, &query)?;
        }
    }
    Ok(modifier)
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
