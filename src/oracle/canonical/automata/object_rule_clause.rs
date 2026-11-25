//! Clauses whose subject is a permanent operand (a declared target, the
//! source, an anaphor, an attached object or a collection) and whose
//! predicate changes that permanent's rules state:
//!
//! ```text
//! Clause    := SUBJECT Predicate
//!            | "All creatures able to block" SUBJECT DURATION "do so" -- CR 509.1c
//!            | "its activated abilities can't be activated" DURATION   -- CR 602.5
//!            | "your life total can't change" DURATION                  -- CR 119.3
//! Predicate := (phases|phase) "out"                         -- CR 702.26b
//!            | (assigns|assign) "no" "combat" "damage" DURATION -- CR 510.1
//!            | "can't" "be" "regenerated" DURATION          -- CR 701.15
//!            | (attacks|blocks) [RECIPIENT] WINDOW "if able" -- CR 508.1d, 509.1c
//! ```
//!
//! A requirement, an activation prohibition and a life-total lock are
//! rules an effect sets for a duration (`RuleChangeNode`); each lowers to
//! one engine rule modifier, never to a card-specific operation.
//!
//! The subject is the shared permanent operand; its number selects the
//! verb's agreement (`each` is distributive and singular). A phasing is the
//! ordinary `phase out` keyword action; a combat-damage rule is a temporary
//! marker on each object for the duration (CR 611.2a). The clause claims
//! only after its lemma and particle are read: any other verb stays with the
//! other subject frames.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CombatRequirementNode, EffectDurationNode, EffectRecognitionContext, GrantedKeyword,
    LocalEffectNode, PermanentActionAttempt, PermanentEffectReference, PermanentKeywordAction,
    PermanentKeywordActionNode, PermanentOperandNode, SubjectVerbAgreement,
    TemporaryKeywordGrantNode, VerbAgreement, VerbForm, VerbLemma, lookup_verb,
    permanent_operand_is_plural, recognize_contextual_permanent_operand,
    recognize_trailing_or_supplied_duration,
};
use crate::oracle::canonical::automata::{Auxiliary, is_verb_form, lookup_auxiliary};

/// A rule an effect sets for a duration (CR 611.2a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RuleChangeNode {
    /// `SUBJECT attacks|blocks [RECIPIENT] WINDOW if able`.
    CombatRequirement(CombatRequirementNode),
    /// `its activated abilities can't be activated DURATION` (CR 602.5):
    /// mana abilities included, as for detain (CR 701.35a).
    ActivationProhibition {
        objects: PermanentOperandNode,
        duration: EffectDurationNode,
    },
    /// `your life total can't change DURATION` (CR 119.3): the ability
    /// controller neither gains nor loses life.
    LifeTotalLock { duration: EffectDurationNode },
}

impl RuleChangeNode {
    /// The permanents the rule applies to, when it applies to permanents.
    pub(in crate::oracle::canonical) fn objects(&self) -> Option<&PermanentOperandNode> {
        match self {
            Self::CombatRequirement(node) => Some(&node.creatures),
            Self::ActivationProhibition { objects, .. } => Some(objects),
            Self::LifeTotalLock { .. } => None,
        }
    }

    /// The ordinary duration of the rule; a `this combat` window has none.
    pub(in crate::oracle::canonical) fn duration(&self) -> Option<&EffectDurationNode> {
        match self {
            Self::CombatRequirement(node) => node.duration(),
            Self::ActivationProhibition { duration, .. } | Self::LifeTotalLock { duration } => {
                Some(duration)
            }
        }
    }
}

fn prohibition_word_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        lookup_auxiliary(raw).is_some_and(|(auxiliary, _)| auxiliary == Auxiliary::Cannot)
    })
}

/// `activated abilities can't be activated` at `offset`.
fn activation_prohibition_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    word_at(cursor, offset, "activated")
        && word_at(cursor, offset + 1, "abilities")
        && prohibition_word_at(cursor, offset + 2)
        && word_at(cursor, offset + 3, "be")
        && word_at(cursor, offset + 4, "activated")
}

/// `its activated abilities can't be activated DURATION`: `its` names the
/// proven permanent antecedent of the clause (the declared target of the
/// previous action, the source, a bound object).
fn recognize_activation_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<RuleChangeNode> {
    cursor.enter("rule-change.activation-prohibition.owner");
    cursor.consume("its")?;
    let objects = match super::permanent_subject::proven_permanent_antecedent(context)
        .ok_or_else(|| cursor.fail("a proven permanent antecedent for `its`"))?
    {
        PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
        other => PermanentOperandNode::BoundReference(other),
    };
    cursor.enter("rule-change.activation-prohibition");
    cursor.consume_phrase(&["activated", "abilities"])?;
    let prohibition = cursor
        .peek()
        .ok_or_else(|| cursor.fail("`can't`"))?
        .to_owned();
    cursor.consume(&prohibition)?;
    cursor.consume_phrase(&["be", "activated"])?;
    cursor.enter("rule-change.activation-prohibition.duration");
    let duration =
        recognize_trailing_or_supplied_duration(cursor, context.preposed_duration.as_ref())?;
    cursor.require(
        duration != EffectDurationNode::Indefinite,
        "a duration for the activation prohibition (`this turn`, `until your next turn`)",
    )?;
    Ok(RuleChangeNode::ActivationProhibition { objects, duration })
}

/// `your life total can't change` at the cursor.
fn life_total_lock_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "your")
        && word_at(cursor, 1, "life")
        && word_at(cursor, 2, "total")
        && prohibition_word_at(cursor, 3)
        && word_at(cursor, 4, "change")
}

fn recognize_life_total_lock(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<RuleChangeNode> {
    cursor.enter("rule-change.life-total-lock");
    cursor.consume_phrase(&["your", "life", "total"])?;
    let prohibition = cursor
        .peek()
        .ok_or_else(|| cursor.fail("`can't`"))?
        .to_owned();
    cursor.consume(&prohibition)?;
    cursor.consume("change")?;
    cursor.enter("rule-change.life-total-lock.duration");
    let duration =
        recognize_trailing_or_supplied_duration(cursor, context.preposed_duration.as_ref())?;
    cursor.require(
        duration != EffectDurationNode::Indefinite,
        "a duration for the life-total lock (`until end of turn`, `until your next turn`)",
    )?;
    Ok(RuleChangeNode::LifeTotalLock { duration })
}

/// Runs a frame child transactionally: the claim was made lexically, so a
/// failure belongs to the child.
fn claimed(
    cursor: &mut OracleTokenCursor<'_>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<LocalEffectNode>,
) -> PermanentActionAttempt {
    let mut trial = cursor.clone();
    match read(&mut trial) {
        Ok(node) => {
            *cursor = trial;
            PermanentActionAttempt::Parsed(node)
        }
        Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
    }
}

/// The verb agreement of a requirement after `subject`: the bare verb after
/// a causative `have`, the plural verb after a plural (not distributive)
/// subject, the singular verb otherwise.
fn requirement_agreement(
    subject: &PermanentOperandNode,
    distributive: bool,
    context: &EffectRecognitionContext,
) -> VerbAgreement {
    if context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive
        || (permanent_operand_is_plural(subject) && !distributive)
    {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    }
}

/// The predicate a permanent subject takes in this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectRulePredicate {
    PhaseOut,
    AssignsNoCombatDamage,
    /// `can't be regenerated DURATION` (CR 701.15): no regeneration shield
    /// replaces the object's destruction while the marker lasts.
    CannotBeRegenerated,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The predicate whose lemma and marker words start at the cursor, in any
/// agreement, with the verb form it is written in.
fn predicate_at(cursor: &OracleTokenCursor<'_>) -> Option<(ObjectRulePredicate, Vec<VerbForm>)> {
    let verb = cursor.peek()?;
    // The modal prohibition takes no agreement: every subject number writes
    // `can't be regenerated`.
    if lookup_auxiliary(verb).is_some_and(|(auxiliary, _)| auxiliary == Auxiliary::Cannot)
        && word_at(cursor, 1, "be")
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| is_verb_form(raw, VerbLemma::Regenerate, VerbForm::PastParticiple))
    {
        return Some((
            ObjectRulePredicate::CannotBeRegenerated,
            vec![VerbForm::Base, VerbForm::ThirdPersonSingular],
        ));
    }
    if let Some((_, forms)) = lookup_verb(verb, &[VerbLemma::Phase])
        && word_at(cursor, 1, "out")
    {
        return Some((ObjectRulePredicate::PhaseOut, forms));
    }
    if let Some((_, forms)) = lookup_verb(verb, &[VerbLemma::Assign])
        && word_at(cursor, 1, "no")
        && word_at(cursor, 2, "combat")
        && word_at(cursor, 3, "damage")
    {
        return Some((ObjectRulePredicate::AssignsNoCombatDamage, forms));
    }
    None
}

/// `SUBJECT phases out` / `SUBJECT assigns no combat damage this turn`. The
/// subject is tentative until the predicate's lemma is read.
pub(in crate::oracle::canonical) fn try_recognize_object_rule_clause(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> PermanentActionAttempt {
    // Frames claimed by their own head words, before any subject is read.
    if super::all_able_blockers_starts(cursor) {
        return claimed(cursor, |cursor| {
            super::recognize_all_able_blockers(cursor, name_links, name_failure, context)
                .map(LocalEffectNode::TemporaryKeywordGrant)
        });
    }
    if word_at(cursor, 0, "its") && activation_prohibition_at(cursor, 1) {
        return claimed(cursor, |cursor| {
            recognize_activation_prohibition(cursor, context).map(LocalEffectNode::RuleChange)
        });
    }
    if life_total_lock_starts(cursor) {
        return claimed(cursor, |cursor| {
            recognize_life_total_lock(cursor, context).map(LocalEffectNode::RuleChange)
        });
    }
    // `X gains haste until end of turn and attacks this combat if able`:
    // the coordinated verb shares the previous action's proven subject.
    if let Some(reference) = context.verb_chain_subject
        && super::combat_requirement_starts(cursor)
    {
        let subject = match reference {
            PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
            other => PermanentOperandNode::BoundReference(other),
        };
        let agreement = requirement_agreement(&subject, false, context);
        return claimed(cursor, |cursor| {
            super::recognize_combat_requirement(
                cursor,
                subject,
                agreement,
                name_links,
                name_failure,
                context,
            )
            .map(|node| LocalEffectNode::RuleChange(RuleChangeNode::CombatRequirement(node)))
        });
    }
    let distributive = word_at(cursor, 0, "each");
    let mut subject_end = cursor.clone();
    let subject = match recognize_contextual_permanent_operand(
        &mut subject_end,
        name_links,
        name_failure,
        context,
    ) {
        Ok(subject) => subject,
        Err(failure) => return PermanentActionAttempt::RejectedCandidate(failure),
    };
    if super::combat_requirement_starts(&subject_end) {
        // `Target creature blocks this creature this turn if able`: the
        // subject's requirement, claimed by its verb and `if able`.
        let agreement = requirement_agreement(&subject, distributive, context);
        let mut trial = subject_end;
        return match super::recognize_combat_requirement(
            &mut trial,
            subject,
            agreement,
            name_links,
            name_failure,
            context,
        ) {
            Ok(node) => {
                *cursor = trial;
                PermanentActionAttempt::Parsed(LocalEffectNode::RuleChange(
                    RuleChangeNode::CombatRequirement(node),
                ))
            }
            Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
        };
    }
    let Some((predicate, forms)) = predicate_at(&subject_end) else {
        return PermanentActionAttempt::NotClaimed;
    };
    // Without a duration, `It can't be regenerated.` is the amendment of a
    // destruction (CR 701.19c), owned by the destroy frames: no claim here.
    if predicate == ObjectRulePredicate::CannotBeRegenerated
        && context.preposed_duration.is_none()
        && !(word_at(&subject_end, 3, "this") || word_at(&subject_end, 3, "until"))
    {
        return PermanentActionAttempt::NotClaimed;
    }
    let mut trial = subject_end;
    match recognize_object_rule_predicate(
        &mut trial,
        predicate,
        &forms,
        subject,
        distributive,
        context,
    ) {
        Ok(node) => {
            *cursor = trial;
            PermanentActionAttempt::Parsed(node)
        }
        Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
    }
}

fn recognize_object_rule_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    predicate: ObjectRulePredicate,
    forms: &[VerbForm],
    subject: PermanentOperandNode,
    distributive: bool,
    context: &EffectRecognitionContext,
) -> MachineResult<LocalEffectNode> {
    cursor.enter("object-rule.agreement");
    // A plural subject takes the base form; a singular or distributive
    // (`each creature ...`) subject the third person singular.
    let expected = if permanent_operand_is_plural(&subject) && !distributive {
        VerbForm::Base
    } else {
        VerbForm::ThirdPersonSingular
    };
    cursor.require(
        forms.contains(&expected),
        match expected {
            VerbForm::Base => "the plural verb form a plural subject requires",
            _ => "the singular verb form a singular subject requires",
        },
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    match predicate {
        ObjectRulePredicate::CannotBeRegenerated => {
            cursor.enter("object-rule.cannot-be-regenerated");
            cursor.consume("be")?;
            cursor.advance_to(cursor.position() + 1)?;
            cursor.enter("object-rule.cannot-be-regenerated.duration");
            let duration = recognize_trailing_or_supplied_duration(
                cursor,
                context.preposed_duration.as_ref(),
            )?;
            // `It can't be regenerated.` after `destroy` amends that
            // destruction (CR 701.19c); it is not a lasting rule here.
            cursor.require(
                duration != EffectDurationNode::Indefinite,
                "a duration for the regeneration prohibition (`this turn`)",
            )?;
            Ok(LocalEffectNode::TemporaryKeywordGrant(
                TemporaryKeywordGrantNode {
                    objects: subject,
                    keywords: vec![GrantedKeyword::CantBeRegenerated],
                    abilities: Vec::new(),
                    duration,
                },
            ))
        }
        ObjectRulePredicate::PhaseOut => {
            cursor.enter("object-rule.phase-out");
            cursor.consume("out")?;
            Ok(LocalEffectNode::PermanentKeywordAction(
                PermanentKeywordActionNode {
                    action: PermanentKeywordAction::PhaseOut,
                    targets: subject,
                    quantity: None,
                },
            ))
        }
        ObjectRulePredicate::AssignsNoCombatDamage => {
            cursor.enter("object-rule.no-combat-damage");
            cursor.consume_phrase(&["no", "combat", "damage"])?;
            cursor.enter("object-rule.no-combat-damage.duration");
            let duration = recognize_trailing_or_supplied_duration(
                cursor,
                context.preposed_duration.as_ref(),
            )?;
            cursor.require(
                duration != EffectDurationNode::Indefinite,
                "a duration for the combat-damage rule (`this turn`)",
            )?;
            Ok(LocalEffectNode::TemporaryKeywordGrant(
                TemporaryKeywordGrantNode {
                    objects: subject,
                    keywords: vec![GrantedKeyword::AssignsNoCombatDamage],
                    abilities: Vec::new(),
                    duration,
                },
            ))
        }
    }
}

/// True when `remove OBJECTS from combat` starts here: the `remove` lemma
/// whose clause reaches `from combat` without a counter noun.
pub(in crate::oracle::canonical) fn remove_from_combat_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if !cursor.peek().is_some_and(|raw| {
        crate::oracle::canonical::automata::is_verb_form(raw, VerbLemma::Remove, VerbForm::Base)
    }) {
        return false;
    }
    for offset in 1.. {
        let Some(raw) = cursor.peek_ahead(offset) else {
            return false;
        };
        if matches!(raw, "." | ";" | ":")
            || raw.eq_ignore_ascii_case("counter")
            || raw.eq_ignore_ascii_case("counters")
        {
            return false;
        }
        if raw.eq_ignore_ascii_case("from") {
            return word_at(cursor, offset + 1, "combat");
        }
    }
    false
}

/// `remove OBJECTS from combat` (CR 506.4): the objects are the shared
/// permanent operand; the action is the ordinary keyword-action node.
pub(in crate::oracle::canonical) fn recognize_remove_from_combat(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentKeywordActionNode> {
    cursor.enter("effect.remove-from-combat.verb");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        VerbLemma::Remove,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.enter("effect.remove-from-combat.objects");
    let targets =
        recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    cursor.enter("effect.remove-from-combat.relation");
    cursor.consume_phrase(&["from", "combat"])?;
    Ok(PermanentKeywordActionNode {
        action: PermanentKeywordAction::RemoveFromCombat,
        targets,
        quantity: None,
    })
}

#[cfg(test)]
#[path = "object_rule_clause_tests.rs"]
mod tests;
