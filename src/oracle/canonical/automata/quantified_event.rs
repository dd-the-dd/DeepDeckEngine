//! Object events with a quantified subject (CR 603.2c):
//!
//! ```text
//! ObjectEvent := Subject Verb [Complement]
//! Subject     := ("a" | "an") Query            -- singular, once per object
//!              | "another" Query               -- singular, source excluded
//!              | NUMBER "or" "more" ["other"] Collection  -- plural, once per event
//! Verb        := leave(s) ("your graveyard" | "the battlefield") | attack(s)
//!              | block(s) | become(s) "blocked"
//!              | DealClause (active: the event complement; passive:
//!                `is|are dealt [combat|noncombat] damage`, `clause.rs`)
//!              | enter(s) | die(s)
//!              | ("is" | "are") "put into a graveyard from the battlefield"
//! ```
//!
//! The event kind is the verb; the subject is the shared numeric bound plus
//! the shared object query. The trigger parent interprets the bound as event
//! multiplicity; a condition parent would interpret the same bound as a count.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ClauseModality, DamageArguments, DamageClauseArguments, DamageKind, NumericAtom,
    ObjectQueryNode, VerbAgreement, Voice, recognize_numeric_atom, recognize_object_collection,
    recognize_object_query, recognize_supplied_damage_clause, supplied_damage_frame,
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
    /// Declared as a blocker (CR 509.1a).
    Block,
    /// An attacking creature becomes blocked (CR 509.1h).
    BecomeBlocked,
    DealCombatDamageToPlayer,
    /// Any other damage complement; `ObjectEventNode::damage` carries it.
    DealDamage,
    Enter,
    Die,
    /// Leaves the battlefield for any zone (CR 603.6c, 603.10a).
    LeaveBattlefield,
    /// `put into a graveyard from the battlefield`, for any permanent type.
    PutIntoGraveyardFromBattlefield,
    /// `put into OWNER graveyard [from anywhere]`: a card arriving in a
    /// graveyard from any zone (CR 400.3: a card goes to its owner's).
    /// The origin zone when written (`from your library`), else any zone.
    PutIntoGraveyard(GraveyardOwner, Option<&'static str>),
    /// Passive of `deal damage`: the subject is the damaged object (CR 120.3).
    BeDealtDamage(DamageKind),
}

/// Whose graveyard a card arrives in, relative to the ability controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GraveyardOwner {
    Any,
    Controller,
    Opponent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectEventNode {
    pub subject: QuantifiedEventSubjectNode,
    pub verb: ObjectEventVerb,
    /// The shared damage complement of `DealDamage`.
    pub damage: Option<super::DamageDealtComplement>,
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
            // The collection frame reads `other` once.
            let query = recognize_object_collection(cursor)?;
            let exclude_source = query.exclude_source;
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
    // `deal(s) ...` / `is|are dealt ...`: the shared damage clause in the
    // subject's number, active with the event complement or passive.
    cursor.enter("event.object.damage-clause");
    let agreement = if singular {
        VerbAgreement::ThirdPersonSingular
    } else {
        VerbAgreement::Base
    };
    for (voice, arguments) in [
        (Voice::Active, DamageArguments::EventComplement),
        (Voice::Passive, DamageArguments::PassiveKind),
    ] {
        let frame = supplied_damage_frame(agreement, voice, ClauseModality::Indicative, arguments);
        let (verb, damage) = match recognize_supplied_damage_clause(cursor, &frame)? {
            None => continue,
            Some(DamageClauseArguments::Complement(complement))
                if complement.is_combat_damage_to_any_player() =>
            {
                (ObjectEventVerb::DealCombatDamageToPlayer, None)
            }
            Some(DamageClauseArguments::Complement(complement)) => {
                (ObjectEventVerb::DealDamage, Some(complement))
            }
            Some(DamageClauseArguments::Kind(kind)) => (ObjectEventVerb::BeDealtDamage(kind), None),
            Some(_) => return Err(cursor.fail("a damage event complement")),
        };
        cursor.enter("event.object.complete");
        return Ok(ObjectEventNode {
            subject,
            verb,
            damage,
        });
    }
    cursor.enter("event.object.verb");
    let raw = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("an object event verb"))?;
    let next_is = |word: &str| {
        cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    let (base, mut verb) = match raw.as_str() {
        "leave" | "leaves" if next_is("the") => ("leave", ObjectEventVerb::LeaveBattlefield),
        "leave" | "leaves" => ("leave", ObjectEventVerb::LeaveControllerGraveyard),
        "attack" | "attacks" => ("attack", ObjectEventVerb::Attack),
        "block" | "blocks" => ("block", ObjectEventVerb::Block),
        "become" | "becomes" => ("become", ObjectEventVerb::BecomeBlocked),
        "enter" | "enters" => ("enter", ObjectEventVerb::Enter),
        "die" | "dies" => ("die", ObjectEventVerb::Die),
        "is" | "are" if next_is("put") => ("is", ObjectEventVerb::PutIntoGraveyardFromBattlefield),
        _ => {
            return Err(cursor.fail(
                "leave, attack, block, become blocked, deal combat damage, enter, die, be put into a graveyard or be dealt damage",
            ));
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
    match verb {
        ObjectEventVerb::LeaveControllerGraveyard => {
            cursor.consume_phrase(&["your", "graveyard"])?
        }
        ObjectEventVerb::LeaveBattlefield => cursor.consume_phrase(&["the", "battlefield"])?,
        ObjectEventVerb::BecomeBlocked => cursor.consume("blocked")?,
        ObjectEventVerb::PutIntoGraveyardFromBattlefield => {
            cursor.consume_phrase(&["put", "into"])?;
            cursor.enter("event.object.graveyard-owner");
            let owner = if peek_word(cursor, 0, "your") {
                cursor.consume("your")?;
                GraveyardOwner::Controller
            } else if peek_word(cursor, 0, "an") && peek_word(cursor, 1, "opponent's") {
                cursor.consume_phrase(&["an", "opponent's"])?;
                GraveyardOwner::Opponent
            } else {
                cursor.consume("a")?;
                GraveyardOwner::Any
            };
            cursor.consume("graveyard")?;
            cursor.enter("event.object.graveyard-origin");
            if owner == GraveyardOwner::Any
                && peek_word(cursor, 0, "from")
                && peek_word(cursor, 1, "the")
            {
                cursor.consume_phrase(&["from", "the", "battlefield"])?;
            } else {
                let mut origin = None;
                if peek_word(cursor, 0, "from") && peek_word(cursor, 1, "anywhere") {
                    cursor.consume_phrase(&["from", "anywhere"])?;
                } else if peek_word(cursor, 0, "from") {
                    cursor.consume("from")?;
                    if peek_word(cursor, 0, "your") {
                        cursor.consume("your")?;
                        origin = Some(
                            match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                                Some("library") => "library",
                                Some("hand") => "hand",
                                _ => return Err(cursor.fail("library or hand")),
                            },
                        );
                        cursor.consume(origin.expect("checked origin"))?;
                        // A second origin (`or hand`) has no single matcher.
                        cursor.require(
                            !peek_word(cursor, 0, "or"),
                            "one origin zone for a graveyard arrival",
                        )?;
                    } else {
                        cursor.consume_phrase(&["the", "battlefield"])?;
                        origin = Some("battlefield");
                    }
                }
                verb = ObjectEventVerb::PutIntoGraveyard(owner, origin);
            }
        }
        // Damage verbs are the shared damage clause's (above).
        ObjectEventVerb::DealCombatDamageToPlayer
        | ObjectEventVerb::DealDamage
        | ObjectEventVerb::BeDealtDamage(_) => unreachable!("read by the damage clause"),
        ObjectEventVerb::Attack
        | ObjectEventVerb::Block
        | ObjectEventVerb::Enter
        | ObjectEventVerb::Die
        | ObjectEventVerb::PutIntoGraveyard(..) => {}
    }
    cursor.enter("event.object.complete");
    Ok(ObjectEventNode {
        subject,
        verb,
        damage: None,
    })
}

fn peek_word(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

#[cfg(test)]
#[path = "quantified_event_tests.rs"]
mod tests;
