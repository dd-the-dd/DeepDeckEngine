//! Clauses whose subject is a permanent operand (a declared target, the
//! source, an anaphor, an attached object or a collection) and whose
//! predicate changes that permanent's rules state:
//!
//! ```text
//! Clause    := SUBJECT Predicate
//! Predicate := (phases|phase) "out"                         -- CR 702.26b
//!            | (assigns|assign) "no" "combat" "damage" DURATION -- CR 510.1
//!            | "can't" "be" "regenerated" DURATION          -- CR 701.15
//! ```
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
    EffectDurationNode, EffectRecognitionContext, GrantedKeyword, LocalEffectNode,
    PermanentActionAttempt, PermanentKeywordAction, PermanentKeywordActionNode,
    PermanentOperandNode, TemporaryKeywordGrantNode, VerbForm, VerbLemma, lookup_verb,
    permanent_operand_is_plural, recognize_contextual_permanent_operand,
    recognize_trailing_or_supplied_duration,
};
use crate::oracle::canonical::automata::{Auxiliary, is_verb_form, lookup_auxiliary};

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
