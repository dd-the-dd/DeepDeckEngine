use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// Only the typed event envelope may assert this alias.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObservedSourceRelationship {
    #[default]
    Unproved,
    SameEnteringObject,
}

/// Source identity owned by an ability is not its event's observed object.
/// Only a typed event parent may supply this proof; generic owner contexts do not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceAnaphorBinding {
    #[default]
    Unproved,
    EventSubject(LocalObjectReference),
}

/// Ability defaults and a locally exhausted antecedent are distinct states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentSubjectScope {
    #[default]
    AbilityContext,
    Local(Option<PermanentEffectReference>),
}

pub(in crate::oracle::canonical) fn permanent_antecedent(
    context: &EffectRecognitionContext,
) -> Option<PermanentEffectReference> {
    match context.permanent_subject_scope {
        PermanentSubjectScope::Local(reference) => reference,
        PermanentSubjectScope::AbilityContext => {
            if context.observed_source_relationship
                == ObservedSourceRelationship::SameEnteringObject
            {
                context.source.map(PermanentEffectReference::Source)
            } else {
                context
                    .observed_event_object
                    .map(PermanentEffectReference::Observed)
                    .or_else(|| context.source.map(PermanentEffectReference::Source))
            }
        }
    }
}

/// Resolve only proved lexical/event subjects. The legacy owner fallback above
/// remains available to existing consumers until each is requalified explicitly.
pub(in crate::oracle::canonical) fn proven_permanent_antecedent(
    context: &EffectRecognitionContext,
) -> Option<PermanentEffectReference> {
    let reference = match context.permanent_subject_scope {
        PermanentSubjectScope::Local(reference) => reference,
        PermanentSubjectScope::AbilityContext => match context.source_anaphor_binding {
            SourceAnaphorBinding::EventSubject(source) => {
                Some(PermanentEffectReference::Source(source))
            }
            SourceAnaphorBinding::Unproved => context
                .observed_event_object
                .map(PermanentEffectReference::Observed),
        },
    };
    // This is the existing genuine entry alias, combined with explicit source-
    // event proof. Neither owner availability nor this alias alone mints proof.
    if reference
        == Some(PermanentEffectReference::Observed(
            TypedObservedEventObject::EnteringPermanent,
        ))
        && context.observed_source_relationship == ObservedSourceRelationship::SameEnteringObject
        && let SourceAnaphorBinding::EventSubject(source) = context.source_anaphor_binding
    {
        return Some(PermanentEffectReference::Source(source));
    }
    reference
}

/// Typed result/subject transitions have one exhaustive owner. An unbound
/// object result ends the prior lexical referent; only actions without a new
/// object subject/result preserve it. Zone/departure receipts are not inferred.
pub(in crate::oracle::canonical) fn permanent_subject_after_effect(
    effect: &LocalEffectNode,
    effect_index: usize,
    previous: PermanentSubjectScope,
) -> PermanentSubjectScope {
    use PermanentSubjectScope::Local;
    match effect {
        LocalEffectNode::PutCounters(node) => {
            Local(subject_after_operand(&node.permanent, effect_index))
        }
        LocalEffectNode::TemporaryKeywordGrant(node) => {
            Local(subject_after_operand(&node.objects, effect_index))
        }
        LocalEffectNode::ModifyStats(node) => {
            Local(subject_after_operand(&node.targets, effect_index))
        }
        LocalEffectNode::BecomeTypes(node) => {
            Local(subject_after_operand(&node.objects, effect_index))
        }
        LocalEffectNode::TapPermanents(node) => {
            Local(subject_after_operand(&node.targets, effect_index))
        }
        LocalEffectNode::PermanentKeywordAction(node) => {
            Local(subject_after_operand(&node.targets, effect_index))
        }
        LocalEffectNode::ControlChange(node) => {
            Local(subject_after_operand(&node.objects, effect_index))
        }
        LocalEffectNode::Attach(node) => {
            Local(subject_after_operand(&node.recipient, effect_index))
        }
        LocalEffectNode::UntapTarget(node) => Local(Some(node.permanent)),
        LocalEffectNode::CreateExplicitTokens(_) => Local(Some(
            PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens),
        )),
        LocalEffectNode::CardAction(CardActionNode::Move {
            cards: CardOperandNode::Selection(selection),
            destination,
        }) if destination.zone == CardZoneName::Battlefield
            && selection.quantity
                == CardSelectionQuantityNode::Exactly(NumericExpression::from(1)) =>
        {
            Local(Some(PermanentEffectReference::Observed(
                TypedObservedEventObject::MovedOntoBattlefield,
            )))
        }
        LocalEffectNode::Choose(node) => match &node.domain {
            ChoiceDomainNode::CreatureType => previous,
            ChoiceDomainNode::PermanentTargets(targets) => {
                Local(singular_permanent_target(targets, effect_index))
            }
            ChoiceDomainNode::PlayerTarget(_) => Local(None),
        },
        // A targeted source occupies the action's first slots; the recipient
        // is then no longer the action's sole singular referent.
        LocalEffectNode::DealDamage(node) => Local(match (&node.source, &node.recipient) {
            (PermanentOperandNode::Targets(_), _) => None,
            (_, DamageRecipientNode::Targets(targets)) => {
                singular_permanent_target(targets, effect_index)
            }
            _ => None,
        }),
        LocalEffectNode::Fight(_) => Local(None),
        LocalEffectNode::ZoneChange(node) => {
            Local(singular_permanent_target(&node.target, effect_index))
        }
        LocalEffectNode::TransformPermanent(node) => {
            Local(Some(PermanentEffectReference::Source(node.permanent)))
        }
        LocalEffectNode::PlayerAction(node) => match &node.action {
            super::SingularPlayerAction::Life(_) => previous,
            super::SingularPlayerAction::Draw(_)
            | super::SingularPlayerAction::Mill(_)
            | super::SingularPlayerAction::Discard(_)
            | super::SingularPlayerAction::Sacrifice(_)
            | super::SingularPlayerAction::Venture(_)
            | super::SingularPlayerAction::Proliferate => Local(None),
        },
        // A list that only changes life totals leaves the permanent subject
        // alone; any other action replaces the antecedent.
        LocalEffectNode::QuantifiedPlayerAction(node)
            if node
                .actions
                .iter()
                .all(|action| matches!(action, QuantifiedPlayerAction::Life(_))) =>
        {
            previous
        }
        LocalEffectNode::QuantifiedPlayerAction(_) => Local(None),
        LocalEffectNode::GainLife(_)
        | LocalEffectNode::LoseLife(_)
        | LocalEffectNode::AddMana(_)
        | LocalEffectNode::TemporaryPlayerPermission(_) => previous,
        LocalEffectNode::CardAction(_)
        | LocalEffectNode::OptionalSequence(_)
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::CopyStackObject(_)
        | LocalEffectNode::Retarget(_)
        | LocalEffectNode::DrawCards(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::Amass(_)
        | LocalEffectNode::EmpowerJace(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::ControllerKeywordAction(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::Surveil(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::PreventDamage(_)
        | LocalEffectNode::MoveSource(_)
        | LocalEffectNode::MoveCards(_)
        | LocalEffectNode::ZoneBatch(_)
        | LocalEffectNode::TargetCardZoneChange(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::ControllerLibrarySearch(_)
        | LocalEffectNode::OptionalCardDeployment(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::LoseAbilities(_)
        | LocalEffectNode::DestroyReferencedSet(_)
        | LocalEffectNode::DestroyCollection(_)
        | LocalEffectNode::Sacrifice(_)
        | LocalEffectNode::SelectedPermanents(_) => Local(None),
    }
}

pub(in crate::oracle::canonical) fn selector_guarantees_permanent(
    selector: &TargetSelector,
) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            // Object target selectors are lowered to a battlefield-permanent
            // candidate domain. Their predicate may add only a subtype/color;
            // it must be evaluable in that domain, not independently name it.
            predicate.supports_domain(ObjectKind::Permanent)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(selector_guarantees_permanent)
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

pub(in crate::oracle::canonical) fn singular_permanent_target(
    targets: &TargetListNode,
    effect_index: usize,
) -> Option<PermanentEffectReference> {
    let [slot] = targets.slots.as_slice() else {
        return None;
    };
    (slot.cardinality.maximum == 1 && selector_guarantees_permanent(&slot.selector)).then_some(
        PermanentEffectReference::Target(TargetBinding {
            effect_index,
            slot_index: 0,
        }),
    )
}

pub(in crate::oracle::canonical) fn subject_after_operand(
    operand: &PermanentOperandNode,
    effect_index: usize,
) -> Option<PermanentEffectReference> {
    match operand {
        PermanentOperandNode::Targets(targets) => singular_permanent_target(targets, effect_index),
        PermanentOperandNode::BoundReference(reference) => Some(*reference),
        PermanentOperandNode::Reference(source) => Some(PermanentEffectReference::Source(*source)),
        PermanentOperandNode::Collection(_) => None,
    }
}

/// The declared selector of one target slot of an ordinary permanent action.
pub(in crate::oracle::canonical) fn target_slot_selector(
    effect: &LocalEffectNode,
    slot_index: usize,
) -> Option<&TargetSelector> {
    let operand = match effect {
        LocalEffectNode::ModifyStats(node) => &node.targets,
        LocalEffectNode::TemporaryKeywordGrant(node) => &node.objects,
        LocalEffectNode::BecomeTypes(node) => &node.objects,
        LocalEffectNode::PutCounters(node) => &node.permanent,
        LocalEffectNode::TapPermanents(node) => &node.targets,
        LocalEffectNode::PermanentKeywordAction(node) => &node.targets,
        LocalEffectNode::ControlChange(node) => &node.objects,
        LocalEffectNode::Attach(node) => &node.recipient,
        LocalEffectNode::Choose(node) => {
            return node
                .permanent_targets()
                .and_then(|targets| targets.slots.get(slot_index))
                .map(|slot| &slot.selector);
        }
        _ => return None,
    };
    match operand {
        PermanentOperandNode::Targets(targets) => {
            targets.slots.get(slot_index).map(|slot| &slot.selector)
        }
        _ => None,
    }
}

/// `that creature` / `that permanent`: a nominal anaphor of the singular
/// target of the previous action. The nominal must follow from the selector
/// that declared the target; it never reinterprets a differently typed slot.
fn recognize_nominal_permanent_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentOperandNode> {
    use crate::oracle::characteristic_vocabulary::{
        CharacteristicLexeme, lookup_object_selector_lexeme,
    };
    cursor.enter("permanent.subject.nominal-anaphor");
    cursor.consume("that")?;
    cursor.enter("permanent.subject.nominal-anaphor.nominal");
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a registered card type or permanent nominal"))?
        .to_owned();
    let nominal = match lookup_object_selector_lexeme(&noun) {
        Some(CharacteristicLexeme::CardType(value)) => TargetNominal::CardType(value),
        Some(CharacteristicLexeme::ObjectDomain(value)) => TargetNominal::Domain(value),
        _ => return Err(cursor.fail("a registered card type or permanent nominal")),
    };
    cursor.enter("permanent.subject.nominal-anaphor.antecedent");
    let reference = proven_permanent_antecedent(context).ok_or_else(|| {
        cursor.fail("a preceding singular permanent subject in this lexical scope")
    })?;
    let PermanentEffectReference::Target(_) = reference else {
        return Err(cursor.fail("a declared target as the nominal anaphor antecedent"));
    };
    let selector = context
        .permanent_subject_selector
        .as_ref()
        .ok_or_else(|| cursor.fail("the declared selector of the preceding permanent target"))?;
    cursor.require(
        selector_implies_nominal(selector, &nominal.as_predicate()),
        "a nominal implied by the preceding target's declared selector",
    )?;
    cursor.consume(&noun)?;
    Ok(PermanentOperandNode::BoundReference(reference))
}

/// The shared subject reader does not declare a target for a reference.
pub(in crate::oracle::canonical) fn recognize_contextual_permanent_operand(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentOperandNode> {
    cursor.enter("permanent.subject.head");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
        && cursor.peek_ahead(1).is_some_and(|noun| {
            matches!(
                crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(noun),
                Some(crate::oracle::characteristic_vocabulary::CharacteristicLexeme::CardType(_))
                    | Some(crate::oracle::characteristic_vocabulary::CharacteristicLexeme::ObjectDomain(
                        ObjectKind::Permanent
                    ))
            )
        })
    {
        return recognize_nominal_permanent_anaphor(cursor, context);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("they"))
    {
        // `they`: the plural objects of the preceding action; only created
        // tokens are a proven plural permanent antecedent.
        cursor.enter("permanent.subject.plural-anaphor");
        let reference = proven_permanent_antecedent(context)
            .filter(|reference| {
                *reference
                    == PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens)
            })
            .ok_or_else(|| cursor.fail("tokens created by the preceding action for `they`"))?;
        cursor.consume("they")?;
        return Ok(PermanentOperandNode::BoundReference(reference));
    }
    if cursor
        .peek()
        .is_some_and(super::reference::is_source_anaphor)
    {
        let reference = proven_permanent_antecedent(context).ok_or_else(|| {
            cursor.fail(
                "a source binding or preceding singular permanent subject in this lexical scope",
            )
        })?;
        let pronoun = cursor
            .peek()
            .expect("checked source/permanent anaphor")
            .to_owned();
        cursor.consume(&pronoun)?;
        return Ok(match reference {
            PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
            _ => PermanentOperandNode::BoundReference(reference),
        });
    }
    if cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("another")
            || raw.eq_ignore_ascii_case("up")
    }) || cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity")
    {
        let targets = recognize_target_list(cursor)?;
        cursor.require(
            targets
                .slots
                .iter()
                .all(|slot| selector_guarantees_permanent(&slot.selector)),
            "target slots guaranteed to denote battlefield permanents",
        )?;
        return Ok(PermanentOperandNode::Targets(targets));
    }
    if cursor.peek_token().is_some_and(|token| {
        matches!(
            token.token_type.as_str(),
            "typeWord" | "negativeTypeWord" | "supertypeWord" | "colorWord"
        )
    }) {
        let query = recognize_object_collection(cursor)?;
        cursor.require(
            query.predicate.has_domain(ObjectKind::Permanent)
                && query.predicate.supports_domain(ObjectKind::Permanent),
            "a battlefield permanent collection",
        )?;
        return Ok(PermanentOperandNode::Collection(query));
    }
    recognize_permanent_operand(cursor, name_links, name_failure, context.source.as_ref())
}
