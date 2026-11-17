//! Object events with a quantified subject (CR 603.2c):
//!
//! ```text
//! ObjectEvent := Subject Verb [Complement]
//! Subject     := ("a" | "an") Query            -- singular, once per object
//!              | "another" Query               -- singular, source excluded
//!              | NUMBER "or" "more" ["other"] Collection  -- plural, once per event
//! Verb        := leave(s) "your graveyard" | attack(s)
//!              | deal(s) "combat damage to a player" | enter(s) | die(s)
//!              | ("is" | "are") "dealt" ["combat" | "noncombat"] "damage"
//! ```
//!
//! The event kind is the verb; the subject is the shared numeric bound plus
//! the shared object query. The trigger parent interprets the bound as event
//! multiplicity; a condition parent would interpret the same bound as a count.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DamageEventKind, NumericAtom, ObjectQueryNode, recognize_dealt_damage_complement,
    recognize_numeric_atom, recognize_object_collection, recognize_object_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EventMultiplicity {
    /// `a`/`an`/`another`: one trigger per matching object in the event.
    PerObject,
    /// `N or more`: one trigger per event containing at least N matches.
    PerEvent { minimum: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QuantifiedEventSubjectNode {
    pub multiplicity: EventMultiplicity,
    pub query: ObjectQueryNode,
    pub exclude_source: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectEventVerb {
    /// Leave the ability controller's graveyard (a look-back trigger, CR 603.10a).
    LeaveControllerGraveyard,
    Attack,
    DealCombatDamageToPlayer,
    Enter,
    Die,
    /// Passive of `deal damage`: the subject is the damaged object (CR 120.3).
    BeDealtDamage(DamageEventKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectEventNode {
    pub subject: QuantifiedEventSubjectNode,
    pub verb: ObjectEventVerb,
}

/// `N or more` at the cursor, as a lexical probe that commits nothing.
pub(in crate::oracle::canonical) fn quantified_plural_subject_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("more"))
}

pub(in crate::oracle::canonical) fn recognize_quantified_event_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<QuantifiedEventSubjectNode> {
    cursor.enter("event.quantified-subject.quantifier");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("an event subject quantifier"))?;
    match head.as_str() {
        "a" | "an" | "another" => {
            cursor.consume(&head)?;
            cursor.enter("event.quantified-subject.singular-query");
            let query = recognize_object_query(cursor)?;
            Ok(QuantifiedEventSubjectNode {
                multiplicity: EventMultiplicity::PerObject,
                query,
                exclude_source: head == "another",
            })
        }
        _ => {
            cursor.enter("event.quantified-subject.minimum");
            let NumericAtom::Integer(minimum) = recognize_numeric_atom(cursor)? else {
                return Err(cursor.fail("a fixed event threshold"));
            };
            cursor.require(minimum >= 1, "a positive event threshold")?;
            cursor.consume_phrase(&["or", "more"])?;
            cursor.enter("event.quantified-subject.plural-query");
            let exclude_source = cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("other"));
            if exclude_source {
                cursor.consume("other")?;
            }
            let query = recognize_object_collection(cursor)?;
            Ok(QuantifiedEventSubjectNode {
                multiplicity: EventMultiplicity::PerEvent { minimum },
                query,
                exclude_source,
            })
        }
    }
}

/// The verb agrees in number with the subject's quantifier: a singular
/// subject takes the third-person form, a plural subject the base form.
pub(in crate::oracle::canonical) fn recognize_object_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectEventNode> {
    let subject = recognize_quantified_event_subject(cursor)?;
    let singular = subject.multiplicity == EventMultiplicity::PerObject;
    cursor.enter("event.object.verb");
    let raw = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("an object event verb"))?;
    let (base, verb) = match raw.as_str() {
        "leave" | "leaves" => ("leave", ObjectEventVerb::LeaveControllerGraveyard),
        "attack" | "attacks" => ("attack", ObjectEventVerb::Attack),
        "deal" | "deals" => ("deal", ObjectEventVerb::DealCombatDamageToPlayer),
        "enter" | "enters" => ("enter", ObjectEventVerb::Enter),
        "die" | "dies" => ("die", ObjectEventVerb::Die),
        // Placeholder kind: the damage complement below fixes the real one.
        "is" | "are" => ("is", ObjectEventVerb::BeDealtDamage(DamageEventKind::Any)),
        _ => {
            return Err(
                cursor.fail("leave, attack, deal combat damage, enter, die or be dealt damage")
            );
        }
    };
    cursor.enter("event.object.verb-agreement");
    let expected = match (base, singular) {
        ("is", true) => "is".to_string(),
        ("is", false) => "are".to_string(),
        (_, true) => format!("{base}s"),
        (_, false) => base.to_string(),
    };
    cursor.require(
        raw == expected,
        "a verb agreeing in number with its subject",
    )?;
    cursor.consume(&expected)?;
    cursor.enter("event.object.complement");
    let mut verb = verb;
    match verb {
        ObjectEventVerb::BeDealtDamage(_) => {
            cursor.consume("dealt")?;
            verb = ObjectEventVerb::BeDealtDamage(recognize_dealt_damage_complement(cursor)?);
        }
        ObjectEventVerb::LeaveControllerGraveyard => {
            cursor.consume_phrase(&["your", "graveyard"])?
        }
        ObjectEventVerb::DealCombatDamageToPlayer => {
            cursor.consume_phrase(&["combat", "damage", "to", "a", "player"])?
        }
        ObjectEventVerb::Attack | ObjectEventVerb::Enter | ObjectEventVerb::Die => {}
    }
    cursor.enter("event.object.complete");
    Ok(ObjectEventNode { subject, verb })
}

#[cfg(test)]
#[path = "quantified_event_tests.rs"]
mod tests;
