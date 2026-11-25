//! Object events with a quantified subject (CR 603.2c):
//!
//! ```text
//! ObjectEvent := Subject Verb [Complement]
//! Subject     := ("a" | "an") Query            -- singular, once per object
//!              | "another" Query               -- singular, source excluded
//!              | NUMBER "or" "more" ["other"] Collection  -- plural, once per event
//! Verb        := leave(s) ("your graveyard" | "the battlefield") | attack(s)
//!              | block(s) | become(s) ("blocked" | "tapped" | "untapped")
//!              | ("is" | "are") "tapped" "for" "mana"
//!              | KEYWORD-ACTION                -- `explores`, `connives`
//!              | DealClause (active: the event complement; passive:
//!                `is|are dealt [combat|noncombat] damage`, `clause.rs`)
//!              | enter(s) | die(s)
//!              | ("is" | "are") "put into a graveyard from the battlefield"
//! ObjectEvent [alone] ModifierTail   -- `event_modifiers.rs`
//! ```
//!
//! The event kind is the verb; the subject is the shared numeric bound plus
//! the shared object query. The trigger parent interprets the bound as event
//! multiplicity; a condition parent would interpret the same bound as a count.
//! Every verb is a lexicon lemma read in the subject's agreement; only the
//! passive auxiliary (`is`/`are`) is read here.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ClauseModality, DamageArguments, DamageClauseArguments, DamageKind, EventModifierTail,
    NumericAtom, ObjectQueryNode, VerbAgreement, VerbLemma, Voice, consume_lemma, lemma_at,
    recognize_numeric_atom, recognize_object_collection, recognize_object_query,
    recognize_supplied_damage_clause, supplied_damage_frame,
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
    /// `is|are turned face up` (CR 708.8).
    TurnedFaceUp,
    /// `become(s) tapped` (CR 701.26a): a tapped transition, whatever
    /// tapped the permanent. Entering the battlefield tapped is no tap.
    BecomeTapped,
    /// `become(s) untapped` (CR 701.26b): an untapped transition.
    BecomeUntapped,
    /// `is tapped for mana` (CR 106.11a, 605.1a): a mana ability of the
    /// permanent whose cost taps it was activated.
    TappedForMana,
    /// `explores`, `connives` (CR 701.44, 701.50): the permanent performed
    /// a keyword action whose program signals its completion.
    PerformedKeywordAction(super::PermanentKeywordAction),
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
    /// `attacks alone` (CR 506.5).
    pub alone: bool,
    /// `attacks you or a planeswalker you control`: the alternatives of the
    /// attacked object (CR 506.2); empty when the attack names none.
    pub attacked: Vec<super::AttackedRecipientNode>,
    /// The shared clause modifier tail; each event kind's lowering decides
    /// which modifiers its matcher evaluates.
    pub modifiers: EventModifierTail,
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
    recognize_object_event_frame(cursor, subject)
}

/// Whether an object-event verb frame in the subject's agreement starts at
/// the cursor (a lexical probe): an event lemma, the passive auxiliary, or
/// the `deal` lemma of the shared damage clause.
pub(in crate::oracle::canonical) fn object_event_frame_starts(
    cursor: &OracleTokenCursor<'_>,
    subject: &QuantifiedEventSubjectNode,
) -> bool {
    let agreement = if subject.multiplicity == EventMultiplicity::PerObject {
        VerbAgreement::ThirdPersonSingular
    } else {
        VerbAgreement::Base
    };
    peek_word(cursor, 0, passive_auxiliary(agreement))
        || OBJECT_EVENT_LEMMAS
            .into_iter()
            .chain([VerbLemma::Deal])
            .any(|lemma| lemma_at(cursor, lemma, agreement))
        || keyword_action_verb_at(cursor, agreement).is_some()
}

/// A keyword-action verb whose grammatical subject is the acting permanent
/// (`explores`, `connives`, CR 701.44, 701.50), in the third-person form of
/// the shared keyword-action vocabulary. Only the singular agreement has
/// such a form; a plural subject reads no keyword-action verb.
fn keyword_action_verb_at(
    cursor: &OracleTokenCursor<'_>,
    agreement: VerbAgreement,
) -> Option<super::PermanentKeywordAction> {
    if agreement != VerbAgreement::ThirdPersonSingular {
        return None;
    }
    cursor
        .peek()
        .and_then(super::PermanentKeywordAction::from_subject_verb)
}

/// `AUXILIARY tapped for mana` at the cursor: the passive participle of the
/// `tap` lemma with its mana purpose (CR 106.11a). A lexical probe.
fn tapped_for_mana_starts(cursor: &OracleTokenCursor<'_>, auxiliary: &str) -> bool {
    peek_word(cursor, 0, auxiliary)
        && cursor.peek_ahead(1).is_some_and(|raw| {
            super::is_verb_form(raw, VerbLemma::Tap, super::VerbForm::PastParticiple)
        })
        && peek_word(cursor, 2, "for")
        && peek_word(cursor, 3, "mana")
}

/// The verb frame of an object event after its (already read) subject. A
/// shared-subject occurrence union (`a creature you control enters or
/// attacks`) reads each alternative frame with the same subject.
pub(in crate::oracle::canonical) fn recognize_object_event_frame(
    cursor: &mut OracleTokenCursor<'_>,
    subject: QuantifiedEventSubjectNode,
) -> MachineResult<ObjectEventNode> {
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
        let modifiers = super::read_event_modifier_tail(cursor)?;
        return Ok(ObjectEventNode {
            subject,
            verb,
            damage,
            alone: false,
            attacked: Vec::new(),
            modifiers,
        });
    }
    cursor.enter("event.object.verb");
    let mut verb = recognize_object_event_verb(cursor, agreement)?;
    cursor.enter("event.object.complement");
    match verb {
        ObjectEventVerb::LeaveControllerGraveyard => {
            cursor.consume_phrase(&["your", "graveyard"])?
        }
        ObjectEventVerb::LeaveBattlefield => cursor.consume_phrase(&["the", "battlefield"])?,
        ObjectEventVerb::BecomeBlocked => cursor.consume("blocked")?,
        // The state complement of `become` (CR 701.26).
        ObjectEventVerb::BecomeTapped => cursor.consume("tapped")?,
        ObjectEventVerb::BecomeUntapped => cursor.consume("untapped")?,
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
        | ObjectEventVerb::TurnedFaceUp
        | ObjectEventVerb::TappedForMana
        | ObjectEventVerb::PerformedKeywordAction(_)
        | ObjectEventVerb::PutIntoGraveyard(..) => {}
    }
    // The attacked player or permanent is the attack's own argument (CR
    // 506.2), read by the shared player and object readers.
    let attacked = if verb == ObjectEventVerb::Attack && super::attacked_recipient_starts(cursor) {
        super::recognize_attacked_recipients(cursor)?
    } else {
        Vec::new()
    };
    // `alone` (CR 506.5) is typed after any verb; the lowering decides which
    // verbs evaluate it. The clause tail follows.
    let alone = super::consume_alone_modifier(cursor)?;
    let modifiers = super::read_event_modifier_tail(cursor)?;
    cursor.enter("event.object.complete");
    Ok(ObjectEventNode {
        subject,
        verb,
        damage: None,
        alone,
        attacked,
        modifiers,
    })
}

/// The object-event lemmas, each with the event kind its frame denotes.
const OBJECT_EVENT_LEMMAS: [VerbLemma; 6] = [
    VerbLemma::Leave,
    VerbLemma::Attack,
    VerbLemma::Block,
    VerbLemma::Become,
    VerbLemma::Enter,
    VerbLemma::Die,
];

/// The passive auxiliary `be` agreeing with the subject (`is`/`are put`).
fn passive_auxiliary(agreement: VerbAgreement) -> &'static str {
    match agreement {
        VerbAgreement::ThirdPersonSingular => "is",
        VerbAgreement::Base => "are",
    }
}

/// Reads the event verb as a lexicon lemma in the subject's agreement. A
/// lemma written in the other agreement is a distinct agreement failure.
fn recognize_object_event_verb(
    cursor: &mut OracleTokenCursor<'_>,
    agreement: VerbAgreement,
) -> MachineResult<ObjectEventVerb> {
    let other = match agreement {
        VerbAgreement::ThirdPersonSingular => VerbAgreement::Base,
        VerbAgreement::Base => VerbAgreement::ThirdPersonSingular,
    };
    let passive_put = |cursor: &OracleTokenCursor<'_>, agreement: VerbAgreement| {
        peek_word(cursor, 0, passive_auxiliary(agreement)) && peek_word(cursor, 1, "put")
    };
    if passive_put(cursor, agreement) {
        cursor.consume(passive_auxiliary(agreement))?;
        return Ok(ObjectEventVerb::PutIntoGraveyardFromBattlefield);
    }
    if super::turned_face_up_starts(cursor, passive_auxiliary(agreement)) {
        super::consume_turned_face_up(cursor, passive_auxiliary(agreement))?;
        return Ok(ObjectEventVerb::TurnedFaceUp);
    }
    if tapped_for_mana_starts(cursor, passive_auxiliary(agreement)) {
        cursor.enter("event.object.tapped-for-mana");
        cursor.consume(passive_auxiliary(agreement))?;
        let participle = cursor
            .peek()
            .map(str::to_string)
            .ok_or_else(|| cursor.fail("the passive participle of `tap`"))?;
        cursor.consume(&participle)?;
        cursor.consume_phrase(&["for", "mana"])?;
        return Ok(ObjectEventVerb::TappedForMana);
    }
    if let Some(action) = keyword_action_verb_at(cursor, agreement) {
        cursor.enter("event.object.keyword-action");
        let raw = cursor.peek().map(str::to_string).unwrap_or_default();
        cursor.consume(&raw)?;
        return Ok(ObjectEventVerb::PerformedKeywordAction(action));
    }
    if let Some(lemma) = OBJECT_EVENT_LEMMAS
        .into_iter()
        .find(|lemma| lemma_at(cursor, *lemma, agreement))
    {
        consume_lemma(cursor, lemma, agreement)?;
        return Ok(match lemma {
            VerbLemma::Leave if peek_word(cursor, 0, "the") => ObjectEventVerb::LeaveBattlefield,
            VerbLemma::Leave => ObjectEventVerb::LeaveControllerGraveyard,
            VerbLemma::Attack => ObjectEventVerb::Attack,
            VerbLemma::Block => ObjectEventVerb::Block,
            // The state complement of `become` selects the transition; the
            // complement itself is consumed with the event's operands.
            VerbLemma::Become if peek_word(cursor, 0, "tapped") => ObjectEventVerb::BecomeTapped,
            VerbLemma::Become if peek_word(cursor, 0, "untapped") => {
                ObjectEventVerb::BecomeUntapped
            }
            VerbLemma::Become => ObjectEventVerb::BecomeBlocked,
            VerbLemma::Enter => ObjectEventVerb::Enter,
            _ => ObjectEventVerb::Die,
        });
    }
    if passive_put(cursor, other)
        || OBJECT_EVENT_LEMMAS
            .into_iter()
            .any(|lemma| lemma_at(cursor, lemma, other))
    {
        cursor.enter("event.object.verb-agreement");
        return Err(cursor.fail("a verb agreeing in number with its subject"));
    }
    Err(cursor.fail(
        "leave, attack, block, become blocked|tapped|untapped, deal combat damage, enter, die, be put into a graveyard, be tapped for mana, be dealt damage or perform a keyword action",
    ))
}

fn peek_word(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

#[cfg(test)]
#[path = "quantified_event_tests.rs"]
mod tests;
