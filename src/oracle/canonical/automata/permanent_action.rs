//! Ordinary action payloads share one subject grammar and lexical binding.
use super::super::{MachineFailure, OracleTokenCursor};
use super::*;
use crate::oracle::canonical::automata::{VerbAgreement, VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

pub(in crate::oracle::canonical) enum PermanentActionAttempt {
    NotClaimed,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(LocalEffectNode),
}

pub(in crate::oracle::canonical) fn ordinary_keyword_subject(
    node: TemporaryKeywordGrantNode,
) -> TemporaryKeywordGrantNode<PermanentOperandNode> {
    let objects = match node.objects {
        TemporaryKeywordGrantObjectsNode::Targets(targets) => {
            PermanentOperandNode::Targets(targets)
        }
        TemporaryKeywordGrantObjectsNode::ControlledSet(set) => PermanentOperandNode::Collection {
            objects: ObjectQueryNode {
                predicate: set.predicate,
                controller: Some(set.controller),
                exclude_source: false,
            },
            exclude_source: false,
        },
    };
    TemporaryKeywordGrantNode {
        objects,
        keywords: node.keywords,
        abilities: node.abilities,
        duration: node.duration,
    }
}

/// True when the operand denotes several permanents: a collection, several
/// target slots, or a slot whose maximum exceeds one.
fn permanent_operand_is_plural(operand: &PermanentOperandNode) -> bool {
    match operand {
        PermanentOperandNode::Targets(targets) => {
            targets.slots.len() > 1
                || targets
                    .slots
                    .iter()
                    .any(|slot| slot.cardinality.maximum > 1)
        }
        PermanentOperandNode::Collection { .. } => true,
        PermanentOperandNode::BoundReference(_) | PermanentOperandNode::Reference(_) => false,
    }
}

/// A recognized subject plus the immediately following action owns the claim
/// (S3: object subject x lemma). The payload child receives its typed
/// subject and end cursor, without reparsing.
pub(in crate::oracle::canonical) fn try_recognize_permanent_action(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> PermanentActionAttempt {
    let bare = context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive;
    let chain_agreement = if bare {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    if let Some(reference) = context.verb_chain_subject
        && (cursor
            .peek()
            .is_some_and(|raw| is_verb_form(raw, VerbLemma::Gain, chain_agreement.form()))
            || super::become_types::characteristic_change_verb_starts(
                cursor,
                chain_agreement,
                super::TypeChangeNumber::Singular,
            )
            || super::become_types::possessed_base_power_toughness_starts(cursor)
            || (!bare && super::combat_clause_starts(cursor)))
    {
        return recognize_verb_chain_action(cursor, reference, context);
    }
    // `Its base power and toughness are 1/1.` / `It's an enchantment.`: a
    // characteristic of the proven antecedent of a new sentence.
    if let Some(attempt) = try_recognize_antecedent_characteristic(cursor, context) {
        return attempt;
    }
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
    // A floating `each` after a plural subject distributes the action over
    // its members (`Up to two target creatures each get +2/+2`). Each
    // member already receives its own modification, so the quantifier adds
    // no operation; a singular subject keeps rejecting it.
    if subject_end
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
        && permanent_operand_is_plural(&subject)
        && subject_end.peek_ahead(1).is_some_and(|raw| {
            is_verb_form(raw, VerbLemma::Get, VerbForm::Base)
                || is_verb_form(raw, VerbLemma::Gain, VerbForm::Base)
        })
    {
        subject_end.enter("permanent-action.distributive-each");
        if let Err(failure) = subject_end.consume("each") {
            return PermanentActionAttempt::RejectedCandidate(failure);
        }
    }
    let action_offset = usize::from(
        subject_end
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("also")),
    );
    let Some(action) = subject_end.peek_ahead(action_offset) else {
        return PermanentActionAttempt::NotClaimed;
    };
    if action_offset == 0 && super::untap_restriction_starts(&subject_end) {
        // `SUBJECT doesn't untap during ...`: the negated untap clause owns
        // the claim; the subject's number selects the negation.
        if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context) {
            return PermanentActionAttempt::FailedCommitted(failure);
        }
        let number = match &subject {
            PermanentOperandNode::BoundReference(_) | PermanentOperandNode::Reference(_) => {
                super::SubjectNumber::Written
            }
            operand if permanent_operand_is_plural(operand) => super::SubjectNumber::Plural,
            _ => super::SubjectNumber::Singular,
        };
        let mut trial = subject_end.clone();
        return match super::recognize_untap_restriction(
            &mut trial,
            subject,
            number,
            name_links,
            name_failure,
        ) {
            Ok(node) => {
                *cursor = trial;
                PermanentActionAttempt::Parsed(LocalEffectNode::UntapRestriction(node))
            }
            Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
        };
    }
    if action_offset == 0 && PermanentKeywordAction::from_subject_verb(action).is_some() {
        // `SUBJECT explores|connives|endures`: the subject and verb own the
        // claim; the keyword child reads its own numeric argument.
        if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context) {
            return PermanentActionAttempt::FailedCommitted(failure);
        }
        let mut trial = subject_end.clone();
        return match super::recognize_subject_keyword_action(&mut trial, subject, None) {
            Ok(node) => {
                *cursor = trial;
                PermanentActionAttempt::Parsed(LocalEffectNode::PermanentKeywordAction(node))
            }
            Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
        };
    }
    // `SUBJECT becomes DESIGNATION` (CR 701.37b, 702.112b): the registry
    // names the designation; the subject is the shared permanent operand.
    if action_offset == 0
        && !bare
        && is_verb_form(action, VerbLemma::Become, VerbForm::ThirdPersonSingular)
        && let Some(designation) = subject_end
            .peek_ahead(1)
            .and_then(|word| super::PermanentDesignation::from_adjective(&word))
            .filter(|designation| designation.given_by_becomes())
    {
        if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context) {
            return PermanentActionAttempt::FailedCommitted(failure);
        }
        let mut trial = subject_end.clone();
        trial.enter("permanent-action.designation");
        let consumed = trial
            .consume("becomes")
            .and_then(|_| trial.consume(designation.flag()));
        return match consumed {
            Ok(_) => {
                *cursor = trial;
                PermanentActionAttempt::Parsed(LocalEffectNode::PermanentKeywordAction(
                    PermanentKeywordActionNode {
                        action: PermanentKeywordAction::BecomeDesignated(designation),
                        targets: subject,
                        quantity: None,
                    },
                ))
            }
            Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
        };
    }
    // `That land is a Swamp.`: the copula of a reference subject states its
    // types with the same type-change characteristics as `becomes`.
    if action_offset == 0
        && matches!(
            subject,
            PermanentOperandNode::BoundReference(_) | PermanentOperandNode::Reference(_)
        )
        && action.eq_ignore_ascii_case("is")
    {
        let mut after_copula = subject_end.clone();
        if after_copula.consume("is").is_ok()
            && super::type_change_characteristics_start(
                &after_copula,
                super::TypeChangeNumber::Singular,
            )
        {
            if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context)
            {
                return PermanentActionAttempt::FailedCommitted(failure);
            }
            let mut trial = subject_end.clone();
            return match recognize_reference_copula(&mut trial, subject, false, context) {
                Ok(node) => {
                    *cursor = trial;
                    PermanentActionAttempt::Parsed(LocalEffectNode::BecomeTypes(node))
                }
                Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
            };
        }
    }
    let Some(predicate) =
        classify_permanent_predicate(&subject_end, action, action_offset, bare, &subject, context)
    else {
        return PermanentActionAttempt::NotClaimed;
    };
    match predicate {
        PermanentPredicate::Combat => {
            // The shared combat clause owns everything after the subject; its
            // markers are granted to the subject for the clause's duration.
            if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context)
            {
                return PermanentActionAttempt::FailedCommitted(failure);
            }
            let mut trial = subject_end.clone();
            match super::recognize_temporary_combat_clause(
                &mut trial,
                context.preposed_duration.as_ref(),
            ) {
                Ok((keywords, duration)) => {
                    *cursor = trial;
                    PermanentActionAttempt::Parsed(LocalEffectNode::TemporaryKeywordGrant(
                        TemporaryKeywordGrantNode {
                            objects: subject,
                            keywords,
                            abilities: Vec::new(),
                            duration,
                        },
                    ))
                }
                Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
            }
        }
        PermanentPredicate::CharacteristicChange => {
            // `each ... loses all abilities` keeps its set-binding owner, so a
            // later `those creatures` still names the same bound set.
            if matches!(subject, PermanentOperandNode::Collection { .. })
                && is_verb_form(action, VerbLemma::Lose, VerbForm::ThirdPersonSingular)
            {
                return PermanentActionAttempt::NotClaimed;
            }
            // A type-change frame is only a candidate: other readers own the
            // `becomes a P/T ...` animation shapes, so a non-matching type list
            // must not commit this parent.
            let (agreement, number) = characteristic_subject_agreement(&subject, bare);
            let mut trial = cursor.clone();
            let fragment = match super::become_types::recognize_become_types_subject(
                &mut trial,
                |cursor| {
                    *cursor = subject_end.clone();
                    Ok(subject.clone())
                },
                |cursor| {
                    recognize_contextual_permanent_operand(
                        cursor,
                        name_links,
                        name_failure,
                        context,
                    )
                },
                context.preposed_duration.as_ref(),
                agreement,
                number,
            ) {
                Ok(fragment) => fragment,
                Err(failure) => return PermanentActionAttempt::RejectedCandidate(failure),
            };
            if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context)
            {
                return PermanentActionAttempt::FailedCommitted(failure);
            }
            *cursor = trial;
            PermanentActionAttempt::Parsed(LocalEffectNode::BecomeTypes(fragment.node))
        }
        PermanentPredicate::DealDamage | PermanentPredicate::Fight => {
            if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context)
            {
                return PermanentActionAttempt::FailedCommitted(failure);
            }
            let mut trial = subject_end.clone();
            let result = if predicate == PermanentPredicate::DealDamage {
                recognize_damage_predicate(
                    &mut trial,
                    subject.clone(),
                    context.subject_verb_agreement,
                )
                .map(LocalEffectNode::DealDamage)
            } else {
                recognize_fight_predicate(
                    &mut trial,
                    subject.clone(),
                    name_links,
                    name_failure,
                    context,
                )
                .map(LocalEffectNode::Fight)
            };
            match result {
                Ok(node) => {
                    *cursor = trial;
                    PermanentActionAttempt::Parsed(node)
                }
                Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
            }
        }
        PermanentPredicate::KeywordGrant | PermanentPredicate::ModifyStats => {
            if let Some(failure) = subject_context_failure(cursor, &subject, &subject_end, context)
            {
                return PermanentActionAttempt::FailedCommitted(failure);
            }
            let mut trial = cursor.clone();
            let result = if predicate == PermanentPredicate::KeywordGrant
                && !super::temporary_power_toughness::granted_stats_coordination_starts(
                    &subject_end,
                ) {
                super::keyword_grant::recognize_temporary_keyword_grant_subject(
                    &mut trial,
                    |cursor| {
                        *cursor = subject_end.clone();
                        Ok(subject.clone())
                    },
                    false,
                    context.preposed_duration.as_ref(),
                )
                .map(LocalEffectNode::TemporaryKeywordGrant)
            } else {
                super::temporary_power_toughness::recognize_temporary_power_toughness_subject(
                    &mut trial,
                    |cursor| {
                        *cursor = subject_end.clone();
                        Ok(subject.clone())
                    },
                    context.preposed_duration.as_ref(),
                )
                .map(|fragment| LocalEffectNode::ModifyStats(fragment.node))
            };
            match result {
                Ok(node) => {
                    *cursor = trial;
                    PermanentActionAttempt::Parsed(node)
                }
                Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
            }
        }
    }
}

/// The predicate frame a permanent subject's clause opens (S3: the object
/// subject x the lemma of its verb). One typed choice replaces a set of
/// independent claims; the frames are tried in this order of precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermanentPredicate {
    /// The shared combat clause (`can't block`, `attacks each combat`).
    Combat,
    /// `becomes TYPES` / `loses all abilities` / `has base P/T` (layers 4-7).
    CharacteristicChange,
    /// `deals AMOUNT damage` (CR 120.1).
    DealDamage,
    /// `fights OBJECT` (CR 701.14).
    Fight,
    /// `gain(s) KEYWORDS` (CR 611.2).
    KeywordGrant,
    /// `get(s) +N/+N` (CR 611.2, 613.4c).
    ModifyStats,
}

/// The verb form the clause requires after the subject: the bare verb after
/// a causative (`have target creature get`), else either number of the
/// finite verb (`it gets`, `creatures ... get`).
fn finite_or_bare(raw: &str, lemma: VerbLemma, bare: bool) -> bool {
    is_verb_form(raw, lemma, VerbForm::Base)
        || (!bare && is_verb_form(raw, lemma, VerbForm::ThirdPersonSingular))
}

/// Classifies the predicate after a permanent subject from the verb
/// lexicon and the shared clause starts. `None` makes no claim.
fn classify_permanent_predicate(
    subject_end: &OracleTokenCursor<'_>,
    action: &str,
    action_offset: usize,
    bare: bool,
    subject: &PermanentOperandNode,
    context: &EffectRecognitionContext,
) -> Option<PermanentPredicate> {
    // Only `gain` admits the adverb `also` before it.
    let adjacent = action_offset == 0;
    if adjacent && !bare && super::combat_clause_starts(subject_end) {
        return Some(PermanentPredicate::Combat);
    }
    let (agreement, number) = characteristic_subject_agreement(subject, bare);
    if adjacent
        && super::become_types::characteristic_change_verb_starts(subject_end, agreement, number)
    {
        return Some(PermanentPredicate::CharacteristicChange);
    }
    // `deals AMOUNT damage`: claimed when the shared damage clause starts
    // after the subject (the clause owns the verb form and its frame).
    if adjacent
        && matches!(
            subject,
            PermanentOperandNode::Targets(_) | PermanentOperandNode::BoundReference(_)
        )
        && super::clause_starts(
            subject_end,
            &super::damage_effect_clause_frame(context.subject_verb_agreement),
        )
    {
        return Some(PermanentPredicate::DealDamage);
    }
    if adjacent
        && (is_verb_form(action, VerbLemma::Fight, VerbForm::Base)
            || is_verb_form(action, VerbLemma::Fight, VerbForm::ThirdPersonSingular))
    {
        return Some(PermanentPredicate::Fight);
    }
    if finite_or_bare(action, VerbLemma::Gain, bare) {
        return Some(PermanentPredicate::KeywordGrant);
    }
    if adjacent && finite_or_bare(action, VerbLemma::Get, bare) {
        return Some(PermanentPredicate::ModifyStats);
    }
    None
}

/// `fights OBJECT` (CR 701.14) after a recognized permanent subject. The
/// second fighter is read by the same contextual permanent operand grammar.
fn recognize_fight_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    first: PermanentOperandNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> Result<FightEffectNode, MachineFailure> {
    cursor.enter("effect.fight.action");
    // Either number of the verb, as the subject wrote it.
    let agreement = if cursor
        .peek()
        .is_some_and(|raw| is_verb_form(raw, VerbLemma::Fight, VerbForm::Base))
    {
        crate::oracle::canonical::automata::VerbAgreement::Base
    } else {
        crate::oracle::canonical::automata::VerbAgreement::ThirdPersonSingular
    };
    crate::oracle::canonical::automata::consume_lemma(cursor, VerbLemma::Fight, agreement)?;
    cursor.enter("effect.fight.second");
    let second = recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)?;
    cursor.require(
        !matches!(second, PermanentOperandNode::Collection { .. }),
        "one permanent fighting the subject, not a collection",
    )?;
    Ok(FightEffectNode { first, second })
}

/// Source references are admitted only by a parent that establishes source
/// context, not by interpreting an instant's name as a battlefield object.
fn subject_context_failure(
    cursor: &OracleTokenCursor<'_>,
    subject: &PermanentOperandNode,
    subject_end: &OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> Option<MachineFailure> {
    let explicit_activation_source_available = matches!(
        subject,
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf)
    ) && context
        .activation_source_availability
        .names_battlefield_source();
    if matches!(subject, PermanentOperandNode::Reference(_))
        && context.source.is_none()
        && !explicit_activation_source_available
    {
        return Some(subject_end.fail("a proven permanent-source context for a source subject"));
    }
    if matches!(subject, PermanentOperandNode::Reference(_))
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        use crate::oracle::characteristic_vocabulary::{
            AcceptedForms, ObjectDomain, lookup_card_type, lookup_object_domain,
        };
        let nominal = cursor.peek_ahead(1).unwrap_or_default();
        let incompatible = lookup_card_type(nominal, AcceptedForms::SingularOnly)
            .is_some_and(|kind| !kind.may_appear_on_permanent())
            || lookup_object_domain(nominal, AcceptedForms::SingularOnly)
                == Some(ObjectDomain::Spell);
        if incompatible {
            return Some(
                subject_end.fail("a source nominal compatible with the permanent subject domain"),
            );
        }
    }
    None
}

/// Verb coordination (`X gets +1/+1 and becomes a Doctor and gains flying`)
/// shares the grammatical subject of the previous continuous-effect action.
/// The sequence supplies that proven subject; this child never invents one.
fn recognize_verb_chain_action(
    cursor: &mut OracleTokenCursor<'_>,
    reference: PermanentEffectReference,
    context: &EffectRecognitionContext,
) -> PermanentActionAttempt {
    let subject = match reference {
        PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
        other => PermanentOperandNode::BoundReference(other),
    };
    let agreement = if context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    let mut trial = cursor.clone();
    let node = if super::combat_clause_starts(cursor) {
        super::recognize_temporary_combat_clause(&mut trial, context.preposed_duration.as_ref())
            .map(|(keywords, duration)| {
                LocalEffectNode::TemporaryKeywordGrant(TemporaryKeywordGrantNode {
                    objects: subject.clone(),
                    keywords,
                    abilities: Vec::new(),
                    duration,
                })
            })
    } else if super::become_types::possessed_base_power_toughness_starts(cursor) {
        super::become_types::recognize_possessed_base_power_toughness(
            &mut trial,
            subject.clone(),
            context.preposed_duration.as_ref(),
        )
        .map(|fragment| LocalEffectNode::BecomeTypes(fragment.node))
    } else if super::become_types::characteristic_change_verb_starts(
        cursor,
        agreement,
        super::TypeChangeNumber::Singular,
    ) {
        super::become_types::recognize_become_types_subject(
            &mut trial,
            |_| Ok(subject.clone()),
            |cursor| Err(cursor.fail("a copied object read by a subject frame")),
            context.preposed_duration.as_ref(),
            agreement,
            super::TypeChangeNumber::Singular,
        )
        .map(|fragment| LocalEffectNode::BecomeTypes(fragment.node))
    } else {
        super::keyword_grant::recognize_temporary_keyword_grant_subject(
            &mut trial,
            |_| Ok(subject.clone()),
            false,
            context.preposed_duration.as_ref(),
        )
        .map(LocalEffectNode::TemporaryKeywordGrant)
    };
    match node {
        Ok(node) => {
            *cursor = trial;
            PermanentActionAttempt::Parsed(node)
        }
        Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
    }
}

/// The verb agreement and grammatical number a characteristic change takes
/// after this subject: a plural subject and a causative `have` take the base
/// verb; only a plural subject takes plural type nouns.
fn characteristic_subject_agreement(
    subject: &PermanentOperandNode,
    bare: bool,
) -> (VerbAgreement, super::TypeChangeNumber) {
    let number = if permanent_operand_is_plural(subject) {
        super::TypeChangeNumber::Plural
    } else {
        super::TypeChangeNumber::Singular
    };
    let agreement = if bare || number == super::TypeChangeNumber::Plural {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    (agreement, number)
}

/// A sentence whose subject is the proven antecedent: `It's TYPES.` (the
/// contracted copula) or `Its base power and toughness are P/T.`. `None`
/// makes no claim.
fn try_recognize_antecedent_characteristic(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> Option<PermanentActionAttempt> {
    let possessed = super::become_types::possessed_base_power_toughness_starts(cursor);
    let contracted = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it's"))
        && {
            let mut after = cursor.clone();
            after.consume("it's").is_ok()
                && super::type_change_characteristics_start(
                    &after,
                    super::TypeChangeNumber::Singular,
                )
        };
    if !possessed && !contracted {
        return None;
    }
    cursor.enter("permanent-action.antecedent-characteristic");
    let Some(reference) = super::permanent_subject::proven_permanent_antecedent(context) else {
        return Some(PermanentActionAttempt::FailedCommitted(
            cursor.fail("a proven permanent antecedent for `it`"),
        ));
    };
    let subject = match reference {
        PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
        other => PermanentOperandNode::BoundReference(other),
    };
    let mut trial = cursor.clone();
    let result = if possessed {
        super::become_types::recognize_possessed_base_power_toughness(
            &mut trial,
            subject,
            context.preposed_duration.as_ref(),
        )
        .map(|fragment| fragment.node)
    } else {
        match trial.consume("it's") {
            Ok(()) => recognize_reference_copula(&mut trial, subject, true, context),
            Err(failure) => Err(failure),
        }
    };
    Some(match result {
        Ok(node) => {
            *cursor = trial;
            PermanentActionAttempt::Parsed(LocalEffectNode::BecomeTypes(node))
        }
        Err(failure) => PermanentActionAttempt::FailedCommitted(failure),
    })
}

/// `SUBJECT is TYPES [DURATION]` / `it's TYPES`: the type-change
/// characteristics of a copula, as one characteristic change of the subject
/// (CR 205.1). Supertypes and `every basic land type` have no resolved
/// characteristic primitive and fail explicitly.
fn recognize_reference_copula(
    cursor: &mut OracleTokenCursor<'_>,
    subject: PermanentOperandNode,
    contracted: bool,
    context: &EffectRecognitionContext,
) -> Result<BecomeTypesNode<PermanentOperandNode>, MachineFailure> {
    let types =
        super::recognize_copula_type_change(cursor, super::TypeChangeNumber::Singular, contracted)?;
    cursor.enter("permanent-action.copula.characteristics");
    cursor.require(
        types.supertypes.is_empty() && !types.all_basic_land_types,
        "card types and subtypes in a resolved copula type change",
    )?;
    let mut node = BecomeTypesNode::empty(subject);
    node.subtypes = types.subtypes;
    node.card_types = types.card_types;
    node.retention = types.retention;
    cursor.enter("permanent-action.copula.duration");
    node.duration = if context.preposed_duration.is_some()
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
    {
        Some(super::recognize_trailing_or_supplied_duration(
            cursor,
            context.preposed_duration.as_ref(),
        )?)
    } else {
        None
    };
    Ok(node)
}

#[cfg(test)]
#[path = "permanent_action_tests.rs"]
mod tests;
