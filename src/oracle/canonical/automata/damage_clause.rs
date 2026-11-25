//! The `DamageDealt` argument frame of the shared clause (`clause.rs`) for
//! the lemma `deal`, active and passive. Each policy names what the parent
//! expects in the slots after the verb; the readers are the existing shared
//! children (effect amounts and recipient groups, the event complement, the
//! replacement threshold and recipients, the event quantity).
//!
//! ```text
//! Active  := deal(s) Arguments
//! Passive := (is|are|be) dealt Arguments          -- recipient as subject
//!          | [Kind] damage would be dealt to R    -- damage as subject
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ClauseAction, ClauseArguments, ClauseFrame, ClauseModality, ClauseSubjectPolicy, ClauseTense,
    DamageDealtComplement, DamageEffectArguments, DamageEventRecipient, DamageKind,
    DamageReplacementContext, NumericExpression, SubjectVerbAgreement, VerbAgreement, VerbLemma,
    Voice, recognize_clause, recognize_damage_dealt_complement, recognize_damage_effect_arguments,
    recognize_damage_kind, recognize_damage_replacement_recipients, recognize_damage_threshold,
    recognize_dealt_damage_complement, recognize_event_quantity_expression, recognize_numeric_atom,
};

/// The only lemma of this frame.
pub(in crate::oracle::canonical) const DEAL: &[VerbLemma] = &[VerbLemma::Deal];

/// What the parent expects after the damage verb.
#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) enum DamageArguments<'a> {
    /// One-shot effect: `AMOUNT damage to RECIPIENTS` / `damage equal to E`.
    /// The clause subject of the binding environment is the damage source
    /// a possessive amount names.
    Effect,
    /// Event pattern: `[Kind] damage [to RECIPIENT]`.
    EventComplement,
    /// Passive event (the subject is the recipient): `[Kind] damage`.
    PassiveKind,
    /// Replacement event: `[N or more|less] [Kind] damage [to RECIPIENTS]`.
    ReplacementEvent(&'a DamageReplacementContext<'a>),
    /// Passive replacement with the damage as subject: `to RECIPIENTS`.
    ReplacementRecipients(&'a DamageReplacementContext<'a>),
    /// A replacement's modified quantity: `twice that much damage`.
    EventQuantity,
    /// A literal or variable amount: `N damage`.
    AtomAmount,
    /// A relative clause whose head noun is the damage (`damage SOURCE would
    /// deal`, `damage that would be dealt`): the slot was extracted.
    Gapped,
}

impl std::fmt::Debug for DamageArguments<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Effect => "Effect",
            Self::EventComplement => "EventComplement",
            Self::PassiveKind => "PassiveKind",
            Self::ReplacementEvent(_) => "ReplacementEvent",
            Self::ReplacementRecipients(_) => "ReplacementRecipients",
            Self::EventQuantity => "EventQuantity",
            Self::AtomAmount => "AtomAmount",
            Self::Gapped => "Gapped",
        })
    }
}

/// The damage frame as read, one variant per policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageClauseArguments {
    Effect(DamageEffectArguments),
    Complement(DamageDealtComplement),
    Kind(DamageKind),
    ReplacementEvent {
        kind: DamageKind,
        minimum: Option<i64>,
        maximum: Option<i64>,
        recipients: Vec<DamageEventRecipient>,
    },
    Recipients(Vec<DamageEventRecipient>),
    Amount(NumericExpression),
    Gapped,
}

fn peek_is(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// True when the frame starts after the verb: the noun `damage` before the
/// sentence punctuation, or `to` for a passive whose subject was the damage.
pub(in crate::oracle::canonical) fn damage_arguments_claim(
    cursor: &OracleTokenCursor<'_>,
    policy: DamageArguments<'_>,
) -> bool {
    match policy {
        DamageArguments::ReplacementRecipients(_) => return peek_is(cursor, "to"),
        // `is dealt` has no other sense: the passive verb claims its frame,
        // and a missing damage noun is this frame's failure.
        DamageArguments::PassiveKind | DamageArguments::Gapped => return true,
        _ => {}
    }
    (0..)
        .map_while(|offset| cursor.peek_ahead(offset))
        .take_while(|raw| !matches!(*raw, "." | "," | ";" | ":"))
        .any(|raw| raw.eq_ignore_ascii_case("damage"))
}

/// The `subject` of the damage-as-subject passive: `[Kind] damage`.
pub(in crate::oracle::canonical) fn recognize_damage_theme(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageKind> {
    let kind = recognize_damage_kind(cursor)?;
    cursor.enter("clause.damage.theme");
    cursor.consume("damage")?;
    Ok(kind)
}

pub(in crate::oracle::canonical) fn read_damage_arguments(
    cursor: &mut OracleTokenCursor<'_>,
    policy: DamageArguments<'_>,
) -> MachineResult<DamageClauseArguments> {
    Ok(match policy {
        DamageArguments::Effect => {
            DamageClauseArguments::Effect(recognize_damage_effect_arguments(cursor)?)
        }
        DamageArguments::EventComplement => {
            DamageClauseArguments::Complement(recognize_damage_dealt_complement(cursor)?)
        }
        DamageArguments::PassiveKind => {
            DamageClauseArguments::Kind(recognize_dealt_damage_complement(cursor)?)
        }
        DamageArguments::ReplacementEvent(context) => {
            let (minimum, maximum) = recognize_damage_threshold(cursor)?;
            let kind = recognize_damage_kind(cursor)?;
            cursor.enter("replacement.damage-event.noun");
            cursor.consume("damage")?;
            let recipients = if peek_is(cursor, "to") {
                cursor.consume("to")?;
                recognize_damage_replacement_recipients(cursor, context)?
            } else {
                Vec::new()
            };
            DamageClauseArguments::ReplacementEvent {
                kind,
                minimum,
                maximum,
                recipients,
            }
        }
        DamageArguments::ReplacementRecipients(context) => {
            cursor.enter("replacement.damage-event.passive");
            cursor.consume("to")?;
            DamageClauseArguments::Recipients(recognize_damage_replacement_recipients(
                cursor, context,
            )?)
        }
        DamageArguments::EventQuantity => {
            DamageClauseArguments::Amount(recognize_event_quantity_expression(cursor, "damage")?)
        }
        DamageArguments::Gapped => DamageClauseArguments::Gapped,
        DamageArguments::AtomAmount => {
            cursor.enter("replacement.damage.amount");
            let amount = NumericExpression::Atom(recognize_numeric_atom(cursor)?);
            cursor.enter("replacement.damage.resource");
            cursor.consume("damage")?;
            DamageClauseArguments::Amount(amount)
        }
    })
}

/// A damage clause whose subject the parent already read: the verb in the
/// subject's agreement, voice and modality, then the policy's arguments.
pub(in crate::oracle::canonical) fn supplied_damage_frame<'a>(
    agreement: VerbAgreement,
    voice: Voice,
    modality: ClauseModality,
    arguments: DamageArguments<'a>,
) -> ClauseFrame<'a> {
    ClauseFrame {
        subject: ClauseSubjectPolicy::Supplied(agreement),
        modality,
        tense: ClauseTense::Present,
        voice,
        arguments: vec![ClauseArguments::Damage(arguments)],
        lemmas: DEAL,
    }
}

/// The effect frame after a source subject: `deals`, or the bare `deal`
/// after a causative `have`.
pub(in crate::oracle::canonical) fn damage_effect_clause_frame(
    agreement: SubjectVerbAgreement,
) -> ClauseFrame<'static> {
    supplied_damage_frame(
        match agreement {
            SubjectVerbAgreement::Finite => VerbAgreement::ThirdPersonSingular,
            SubjectVerbAgreement::BareInfinitive => VerbAgreement::Base,
        },
        Voice::Active,
        ClauseModality::Indicative,
        DamageArguments::Effect,
    )
}

/// Reads a supplied-subject damage clause; `Ok(None)` makes no claim.
pub(in crate::oracle::canonical) fn recognize_supplied_damage_clause(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &ClauseFrame<'_>,
) -> MachineResult<Option<DamageClauseArguments>> {
    let Some(clause) = recognize_clause(cursor, frame).into_result()? else {
        return Ok(None);
    };
    match clause.action {
        ClauseAction::DealDamage(arguments) => Ok(Some(arguments)),
        _ => Err(cursor.fail("a damage frame")),
    }
}

/// `would deal` (active, the source read by the parent) or `that would be
/// dealt` (passive, after the relative pronoun) with the damage extracted.
pub(in crate::oracle::canonical) fn recognize_relative_damage_clause(
    cursor: &mut OracleTokenCursor<'_>,
    voice: Voice,
) -> MachineResult<()> {
    let frame = supplied_damage_frame(
        VerbAgreement::Base,
        voice,
        ClauseModality::Would,
        DamageArguments::Gapped,
    );
    match recognize_supplied_damage_clause(cursor, &frame)? {
        Some(DamageClauseArguments::Gapped) => Ok(()),
        _ => Err(cursor.fail(match voice {
            Voice::Active => "`would deal`",
            Voice::Passive => "`would be dealt`",
        })),
    }
}

/// The written subject a following `and deals ...` predicate shares: the
/// source of a stat change on the ability's own object (`This creature gets
/// +1/+1 until end of turn and deals 1 damage to each opponent`). Only a
/// fixed or bound singular reference is shared; a declared target keeps its
/// own slots.
pub(in crate::oracle::canonical) fn shared_damage_subject(
    previous: Option<&super::LocalEffectNode>,
) -> Option<super::PermanentOperandNode> {
    // The written subject of the previous predicate, whichever verb read it
    // (`gets +1/+0`, `gains forestwalk`): the coordinated `deals` shares it.
    let subject = match previous? {
        super::LocalEffectNode::ModifyStats(change) => &change.targets,
        super::LocalEffectNode::TemporaryKeywordGrant(grant) => &grant.objects,
        _ => return None,
    };
    match subject {
        source @ super::PermanentOperandNode::Reference(_) => Some(source.clone()),
        // `it gets +2/+2 until end of turn and deals 2 damage to ...`:
        // one bound object, never a declared set.
        source @ super::PermanentOperandNode::BoundReference(reference)
            if !matches!(reference, super::PermanentEffectReference::TargetSet(_)) =>
        {
            Some(source.clone())
        }
        _ => None,
    }
}

/// True at the finite damage verb with its subject elided (`deals N damage`).
pub(in crate::oracle::canonical) fn shared_subject_damage_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    peek_is(cursor, "deals")
        && damage_arguments_claim(
            &{
                let mut after = cursor.clone();
                let _ = after.advance_to(after.position() + 1);
                after
            },
            DamageArguments::Effect,
        )
}

/// True at a recipient reference this reader owns: the reflexive `itself`,
/// the source's `this NOUN`, the attacked `the player or planeswalker ...
/// attacking`, or the singular `them` of an event player.
fn damage_recipient_reference_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_is(cursor, "itself")
        || peek_is(cursor, "them")
        || super::this_object_nominal_at(cursor).is_some()
        || (peek_is(cursor, "the")
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("player"))
            && cursor
                .peek_ahead(2)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
            && cursor
                .peek_ahead(3)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("planeswalker")))
}

/// A damage recipient named by a reference before any target (CR 120.1,
/// 608.2c), resolved against the binding environment of the damage clause:
///
/// ```text
/// Reference := "itself"                                  -- the clause subject
///            | "this" NOUN                               -- the ability source
///            | "the" "player" "or" "planeswalker" ATTACKER "attacking"
///            | "them"                                    -- the event's player
/// ATTACKER  := "it's" | "it" "is" | "that" NOUN "is"
/// ```
///
/// `None` makes no claim; a claimed reference without its antecedent is a
/// committed diagnostic, never a new target.
pub(in crate::oracle::canonical) fn recognize_damage_recipient_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> Option<MachineResult<super::DamageRecipientNode>> {
    if !damage_recipient_reference_starts(cursor) {
        return None;
    }
    Some(read_damage_recipient_reference(cursor))
}

fn read_damage_recipient_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::DamageRecipientNode> {
    let environment = super::installed_binding_environment(cursor);
    if peek_is(cursor, "itself") {
        // CR 120.1: the object dealing the damage is dealt it.
        cursor.enter("effect.damage.recipient.reflexive");
        let subject = environment
            .subject
            .ok_or_else(|| cursor.fail("a singular damage source `itself` names"))?;
        cursor.consume("itself")?;
        return Ok(super::DamageRecipientNode::BoundObject(subject));
    }
    if peek_is(cursor, "them") {
        // The singular `they` of a player the event records (`At the
        // beginning of each player's upkeep, ... deals 1 damage to them`).
        cursor.enter("effect.damage.recipient.event-player");
        cursor.require(
            environment.frame.event_player
                && environment.player.is_none()
                && environment.player_target.is_none()
                && environment.objects.is_none(),
            "an event player as the only antecedent of `them`",
        )?;
        cursor.consume("them")?;
        return Ok(super::DamageRecipientNode::BoundPlayer(
            super::BoundPlayerReference::EventPlayer,
        ));
    }
    if super::this_object_nominal_at(cursor).is_some() {
        cursor.enter("effect.damage.recipient.source");
        cursor.advance_to(cursor.position() + 2)?;
        return Ok(super::DamageRecipientNode::BoundObject(
            super::PermanentEffectReference::Source(super::LocalObjectReference::SourceSelf),
        ));
    }
    cursor.enter("effect.damage.recipient.attacked");
    cursor.consume_phrase(&["the", "player", "or", "planeswalker"])?;
    cursor.require(
        environment.frame.defending_player,
        "an attack event that records what the creature is attacking (CR 506.3)",
    )?;
    let attacker = if peek_is(cursor, "it's") || peek_is(cursor, "it’s") {
        let raw = cursor.peek().expect("a contracted pronoun").to_string();
        cursor.consume(&raw)?;
        attacking_pronoun_antecedent(cursor, &environment)?
    } else if peek_is(cursor, "it") {
        cursor.consume_phrase(&["it", "is"])?;
        attacking_pronoun_antecedent(cursor, &environment)?
    } else {
        let attacker = super::recognize_nominal_object_reference(cursor, &environment)?;
        cursor.consume("is")?;
        attacker
    };
    cursor.enter("effect.damage.recipient.attacked.participle");
    cursor.consume("attacking")?;
    Ok(super::DamageRecipientNode::AttackedRecipient(attacker))
}

/// `it` in `the player or planeswalker it's attacking`: the attacking object
/// of the attack event, else the clause's own subject (`Whenever this
/// creature attacks, it deals ...`).
fn attacking_pronoun_antecedent(
    cursor: &OracleTokenCursor<'_>,
    environment: &super::BindingEnvironment,
) -> MachineResult<super::PermanentEffectReference> {
    environment
        .frame
        .event_object
        .as_ref()
        .filter(|slot| slot.subject && !slot.departed)
        .map(|slot| slot.reference)
        .or(environment.subject)
        .ok_or_else(|| cursor.fail("an attacking creature `it` names"))
}

#[cfg(test)]
#[path = "damage_clause_tests.rs"]
mod tests;
