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

/// The selector of the one card an action put onto the battlefield: a chosen
/// card, a single card target, or the last action of an optional sequence
/// (`you may put a creature card ... onto the battlefield`). `Some(None)` is
/// a chosen card without a predicate.
pub(in crate::oracle::canonical) fn moved_onto_battlefield_selector(
    effect: &LocalEffectNode,
) -> Option<Option<TargetSelector>> {
    match effect {
        LocalEffectNode::CardAction(CardActionNode::Move {
            cards: CardOperandNode::Selection(selection),
            destination,
        }) if destination.zone == CardZoneName::Battlefield
            && selection.quantity
                == CardSelectionQuantityNode::Exactly(NumericExpression::from(1)) =>
        {
            Some(selection.predicate.clone().map(TargetSelector::Object))
        }
        LocalEffectNode::CardAction(CardActionNode::Move {
            cards: CardOperandNode::Targets(targets),
            destination,
        }) if destination.zone == CardZoneName::Battlefield
            && targets.cardinality == ExpressedTargetCardinality::EXACTLY_ONE =>
        {
            Some(Some(TargetSelector::Object(targets.predicate.clone())))
        }
        LocalEffectNode::OptionalSequence(body) => body
            .effects
            .last()
            .and_then(moved_onto_battlefield_selector),
        // `you may put a creature card from your hand onto the battlefield`:
        // the one card chosen, if any (an empty choice binds nothing).
        LocalEffectNode::OptionalCardDeployment(deployment) => {
            Some(Some(TargetSelector::Object(deployment.filter.clone())))
        }
        _ => None,
    }
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
/// The antecedent an action may operate on: a departed event subject
/// (`a creature dies`) is no permanent any more (CR 400.7).
pub(in crate::oracle::canonical) fn proven_permanent_antecedent(
    context: &EffectRecognitionContext,
) -> Option<PermanentEffectReference> {
    proven_antecedent(context, false)
}

/// The antecedent a condition, a property or a player relation reads: a
/// departed event subject still answers by last known information (CR
/// 608.2h), so `if it was enchanted` or `its controller` keep it.
pub(in crate::oracle::canonical) fn proven_read_antecedent(
    context: &EffectRecognitionContext,
) -> Option<PermanentEffectReference> {
    proven_antecedent(context, true)
}

fn proven_antecedent(
    context: &EffectRecognitionContext,
    include_departed: bool,
) -> Option<PermanentEffectReference> {
    let reference = match context.permanent_subject_scope {
        PermanentSubjectScope::Local(reference) => reference,
        PermanentSubjectScope::AbilityContext => match context.source_anaphor_binding {
            SourceAnaphorBinding::EventSubject(source) => {
                Some(PermanentEffectReference::Source(source))
            }
            // Without a source subject, `it` names the event's observed
            // object, else the object that is the event's subject while it is
            // still a permanent (a dying object is the event's card instead).
            SourceAnaphorBinding::Unproved => context
                .observed_event_object
                .map(PermanentEffectReference::Observed)
                .or_else(|| {
                    context
                        .frame
                        .event_object
                        .as_ref()
                        .filter(|slot| slot.subject && (include_departed || !slot.departed))
                        .map(|slot| slot.reference)
                }),
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
        LocalEffectNode::MoveCounters(node) => {
            Local(subject_after_operand(&node.destination, effect_index))
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
        LocalEffectNode::UntapRestriction(node) => {
            Local(subject_after_operand(&node.objects, effect_index))
        }
        // Another player's action ends the local permanent subject.
        LocalEffectNode::Performed(_) => Local(None),
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
        LocalEffectNode::ReferencedPermanentAction(node) => Local(Some(node.object)),
        LocalEffectNode::CreateExplicitTokens(_) => Local(Some(
            PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens),
        )),
        // CR 400.7: the effect that put one card onto the battlefield finds
        // the permanent it became (`that creature gains haste`).
        effect if moved_onto_battlefield_selector(effect).is_some() => Local(Some(
            PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield),
        )),
        LocalEffectNode::Choose(node) => match &node.domain {
            ChoiceDomainNode::CreatureType | ChoiceDomainNode::Color => previous,
            ChoiceDomainNode::PermanentTargets(targets) => {
                Local(singular_permanent_target(targets, effect_index))
            }
            ChoiceDomainNode::PlayerTarget(_) | ChoiceDomainNode::Player(_) => Local(None),
        },
        // A targeted source occupies the action's first slots; the recipient
        // is then no longer the action's sole singular referent.
        LocalEffectNode::DealDamage(node) => Local(match (&node.source, &node.recipient) {
            (PermanentOperandNode::Targets(_), _) => None,
            (_, DamageRecipientNode::Targets(targets)) => {
                singular_permanent_target(targets, effect_index)
            }
            (_, DamageRecipientNode::BoundObject(reference)) => Some(*reference),
            _ => None,
        }),
        LocalEffectNode::Fight(_) => Local(None),
        // The single protected target is the next action's antecedent.
        LocalEffectNode::PreventDamage(node) => Local(match &node.recipient {
            Some(super::DamagePartyNode::Targets(targets)) => {
                singular_permanent_target(targets, effect_index)
            }
            _ => None,
        }),
        LocalEffectNode::ZoneChange(node) => {
            Local(singular_permanent_target(&node.target, effect_index))
        }
        LocalEffectNode::TransformPermanent(node) => {
            Local(Some(PermanentEffectReference::Source(node.permanent)))
        }
        LocalEffectNode::PlayerAction(node) => match &node.action {
            super::SingularPlayerAction::Life(_) => previous,
            // Drawing, milling and discarding move cards, never a
            // permanent: the permanent antecedent stays.
            super::SingularPlayerAction::Draw(_)
            | super::SingularPlayerAction::Mill(_)
            | super::SingularPlayerAction::Discard(_) => previous,
            // Counters on a player change no permanent.
            super::SingularPlayerAction::Counters(_) | super::SingularPlayerAction::Scry(_) => {
                previous
            }
            super::SingularPlayerAction::Sacrifice(_)
            | super::SingularPlayerAction::Venture(_)
            | super::SingularPlayerAction::Proliferate
            | super::SingularPlayerAction::CreateTokens(_)
            | super::SingularPlayerAction::ReferencedCards { .. } => Local(None),
            // A hand action changes no permanent: the subject antecedent stays.
            super::SingularPlayerAction::Hand(_)
            | super::SingularPlayerAction::ExtraTurn
            | super::SingularPlayerAction::Designation(_)
            | super::SingularPlayerAction::Skip(_)
            | super::SingularPlayerAction::Prohibition(_) => previous,
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
        // The controller's draw, discard, mill, scry and surveil move cards
        // between hidden or public card zones and introduce no permanent: a
        // lexical permanent antecedent an earlier action of the program
        // declared stays (`Put a +1/+1 counter on target creature. Draw a
        // card. That creature gains trample`), as for the same actions of
        // another player above. Their cards are a lexical result of their
        // own (the card-action scope reads them), so the ability's default
        // event or source subject is exhausted as before.
        LocalEffectNode::DrawCards(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::Surveil(_) => match previous {
            Local(Some(reference)) => Local(Some(reference)),
            _ => Local(None),
        },
        LocalEffectNode::CardAction(_)
        | LocalEffectNode::OptionalSequence(_)
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::CopyStackObject(_)
        | LocalEffectNode::Retarget(_)
        | LocalEffectNode::Amass(_)
        | LocalEffectNode::EmpowerJace(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::ControllerKeywordAction(_)
        | LocalEffectNode::Earthbend(_)
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
        // A declared set stays the plural antecedent (`prior_object_set`);
        // it is never the singular subject `it` names.
        PermanentOperandNode::BoundReference(PermanentEffectReference::TargetSet(_)) => None,
        PermanentOperandNode::BoundReference(reference) => Some(*reference),
        PermanentOperandNode::Reference(source) => Some(PermanentEffectReference::Source(*source)),
        PermanentOperandNode::Collection { .. } => None,
    }
}

/// The permanent operand of an action whose targets the shared ordinary
/// permanent-subject lowering declares and binds: only these slots can be
/// named again as a set.
fn ordinary_permanent_operand(effect: &LocalEffectNode) -> Option<&PermanentOperandNode> {
    match effect {
        LocalEffectNode::ModifyStats(node) => Some(&node.targets),
        LocalEffectNode::TemporaryKeywordGrant(node) => Some(&node.objects),
        LocalEffectNode::BecomeTypes(node) => Some(&node.objects),
        LocalEffectNode::PutCounters(node) => Some(&node.permanent),
        LocalEffectNode::TapPermanents(node) => Some(&node.targets),
        LocalEffectNode::UntapRestriction(node) => Some(&node.objects),
        LocalEffectNode::PermanentKeywordAction(node) => Some(&node.targets),
        LocalEffectNode::ControlChange(node) => Some(&node.objects),
        _ => None,
    }
}

/// The declared selector of the plural slot a `TargetSet` reference names.
fn target_set_selector(
    effects: &[LocalEffectNode],
    binding: TargetBinding,
) -> Option<TargetSelector> {
    match ordinary_permanent_operand(effects.get(binding.effect_index)?)? {
        PermanentOperandNode::Targets(targets) => targets
            .slots
            .get(binding.slot_index)
            .map(|slot| slot.selector.clone()),
        _ => None,
    }
}

/// The selector the card types of written tokens guarantee (`a 1/1 white
/// Soldier creature token` is a creature); a copy states no type.
fn created_token_selector(template: &ExplicitTokenTemplateNode) -> Option<TargetSelector> {
    let types = template
        .types
        .iter()
        .map(|kind| ObjectPredicate::CardType(*kind))
        .collect::<Vec<_>>();
    match types.len() {
        0 => None,
        1 => types.into_iter().next().map(TargetSelector::Object),
        _ => Some(TargetSelector::Object(ObjectPredicate::Intersection(types))),
    }
}

/// The object set the immediately preceding action of a program declared or
/// produced (CR 608.2c): the one plural target slot of an ordinary permanent
/// action, a set an earlier `those NOUNs` already named, or the tokens a
/// creation action created, with the number the producer stated. A later
/// `those creatures`, `they`, `each of them` or `that token` resolves against
/// it through the binding environment; any other action produces no set.
pub(in crate::oracle::canonical) fn prior_object_set(
    effects: &[LocalEffectNode],
) -> Option<ObjectSetAntecedent> {
    let effect_index = effects.len().checked_sub(1)?;
    let effect = &effects[effect_index];
    if let LocalEffectNode::CreateExplicitTokens(node) = effect {
        return Some(ObjectSetAntecedent {
            denotes: ObjectSetDenotation::Bound(PermanentEffectReference::Observed(
                TypedObservedEventObject::CreatedTokens,
            )),
            selector: node.body.described().and_then(created_token_selector),
            number: if node.body.quantity == NumericExpression::from(1) {
                GrammaticalNumber::Singular
            } else {
                GrammaticalNumber::Plural
            },
            tokens: true,
        });
    }
    let operand = ordinary_permanent_operand(effect)?;
    if matches!(operand, PermanentOperandNode::Collection { .. })
        && matches!(
            effect,
            LocalEffectNode::BecomeTypes(_)
                | LocalEffectNode::ControlChange(_)
                | LocalEffectNode::PermanentKeywordAction(_)
        )
    {
        return None;
    }
    match operand {
        PermanentOperandNode::Targets(targets) => {
            let [slot] = targets.slots.as_slice() else {
                return None;
            };
            (slot.cardinality.maximum > 1 && selector_guarantees_permanent(&slot.selector)).then(
                || ObjectSetAntecedent {
                    denotes: ObjectSetDenotation::Bound(PermanentEffectReference::TargetSet(
                        TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                    )),
                    selector: Some(slot.selector.clone()),
                    number: GrammaticalNumber::Plural,
                    tokens: false,
                },
            )
        }
        PermanentOperandNode::BoundReference(
            reference @ PermanentEffectReference::TargetSet(binding),
        ) => Some(ObjectSetAntecedent {
            denotes: ObjectSetDenotation::Bound(*reference),
            selector: target_set_selector(effects, *binding),
            number: GrammaticalNumber::Plural,
            tokens: false,
        }),
        // `Untap all creatures you control. They gain flying`: the same
        // collection, when the action changes nothing its query tests.
        PermanentOperandNode::Collection {
            objects,
            exclude_source,
        } if collection_survives(effect, &objects.predicate) => Some(ObjectSetAntecedent {
            denotes: ObjectSetDenotation::Collection {
                query: objects.clone(),
                exclude_source: *exclude_source,
            },
            selector: Some(TargetSelector::Object(objects.predicate.clone())),
            number: GrammaticalNumber::Plural,
            tokens: false,
        }),
        _ => None,
    }
}

/// True when `effect` leaves every characteristic `predicate` tests
/// unchanged, so the collection it acted on is exactly the collection the
/// same query reads after it (CR 608.2c, 611.2c). Types, colors, names,
/// supertypes, control and ownership are never changed by these actions;
/// the tapped status, counters, power and toughness, and keywords are each
/// changed by one of them and admitted only when that action is absent.
/// Anything else (a designation, a relation, a history) is refused.
fn collection_survives(effect: &LocalEffectNode, predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::IsCommander
        | ObjectPredicate::IsToken
        | ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::CharacteristicBatch(_)
        | ObjectPredicate::Color(_)
        | ObjectPredicate::ColorQuantity(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::ExactManaCost(_)
        | ObjectPredicate::ObjectKind(_)
        | ObjectPredicate::OwnedByAbilityController { .. }
        | ObjectPredicate::PermanentCard
        | ObjectPredicate::ChosenCreatureType
        | ObjectPredicate::ChosenColor => true,
        ObjectPredicate::Status(_) => !matches!(effect, LocalEffectNode::TapPermanents(_)),
        ObjectPredicate::HasCounter(_) => !matches!(effect, LocalEffectNode::PutCounters(_)),
        ObjectPredicate::HasKeyword(_) => {
            !matches!(effect, LocalEffectNode::TemporaryKeywordGrant(_))
        }
        ObjectPredicate::NumericComparison { .. } => !matches!(
            effect,
            LocalEffectNode::ModifyStats(_) | LocalEffectNode::PutCounters(_)
        ),
        ObjectPredicate::Not(operand) => collection_survives(effect, operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => operands
            .iter()
            .all(|operand| collection_survives(effect, operand)),
        ObjectPredicate::Designation(_) | ObjectPredicate::Relation(_) => false,
    }
}

/// The singular object antecedent after the actions read so far, with the
/// selector that declared it: the countered spell of a preceding counter
/// action, else the proven subject scope after the last action, read for its
/// last known information (CR 608.2h). A postposed `where X is its power`
/// reads the clause it closes; `None` leaves the enclosing antecedent.
pub(in crate::oracle::canonical) fn object_antecedent_after(
    effects: &[LocalEffectNode],
    scope: PermanentSubjectScope,
    context: &EffectRecognitionContext,
) -> Option<ObjectSubjectAntecedent> {
    let last = effects.len().checked_sub(1)?;
    if let LocalEffectNode::CounterSpell(counter) = &effects[last] {
        let [slot] = counter.target.slots.as_slice() else {
            return None;
        };
        return (slot.cardinality.maximum == 1).then(|| ObjectSubjectAntecedent {
            reference: PermanentEffectReference::Target(TargetBinding {
                effect_index: last,
                slot_index: 0,
            }),
            selector: Some(slot.selector.clone()),
        });
    }
    let mut scoped = context.clone();
    scoped.permanent_subject_scope = scope;
    // Created tokens are a set (`create X tokens, where X is its power`
    // reads the event's object, not the tokens): only a singular object of
    // the clause replaces the enclosing antecedent.
    let reference = proven_read_antecedent(&scoped).filter(|reference| {
        !matches!(
            reference,
            PermanentEffectReference::TargetSet(_)
                | PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens)
        )
    })?;
    let selector = match reference {
        PermanentEffectReference::Target(binding) => effects
            .get(binding.effect_index)
            .and_then(|effect| target_slot_selector(effect, binding.slot_index))
            .cloned(),
        PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield) => {
            effects
                .iter()
                .rev()
                .find_map(moved_onto_battlefield_selector)
                .flatten()
        }
        _ => None,
    };
    Some(ObjectSubjectAntecedent {
        reference,
        selector,
    })
}

/// The environment a permanent subject reads: the context's frame and proven
/// antecedent plus the object set the program parent installed.
fn permanent_subject_environment(
    cursor: &OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> BindingEnvironment {
    let mut environment = BindingEnvironment::for_permanent_context(context);
    environment.objects = installed_binding_environment(cursor).objects;
    environment
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
        LocalEffectNode::UntapRestriction(node) => &node.objects,
        LocalEffectNode::PermanentKeywordAction(node) => &node.targets,
        LocalEffectNode::ControlChange(node) => &node.objects,
        LocalEffectNode::Attach(node) => &node.recipient,
        LocalEffectNode::Choose(node) => {
            return node
                .permanent_targets()
                .and_then(|targets| targets.slots.get(slot_index))
                .map(|slot| &slot.selector);
        }
        // Actions that declare their targets outside a permanent operand
        // expose the same declared selector for nominal anaphora.
        LocalEffectNode::DealDamage(node) => {
            return match &node.recipient {
                DamageRecipientNode::Targets(targets) => targets.slots.get(slot_index),
                _ => None,
            }
            .map(|slot| &slot.selector);
        }
        LocalEffectNode::ZoneChange(node) => {
            return node.target.slots.get(slot_index).map(|slot| &slot.selector);
        }
        LocalEffectNode::CounterSpell(node) => {
            return node.target.slots.get(slot_index).map(|slot| &slot.selector);
        }
        LocalEffectNode::PreventDamage(node) => {
            return match &node.recipient {
                Some(super::DamagePartyNode::Targets(targets)) => targets.slots.get(slot_index),
                _ => None,
            }
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

/// `that creature` / `that permanent`: a nominal anaphor resolved by the
/// shared binding environment of this context.
fn recognize_nominal_permanent_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentOperandNode> {
    let environment = permanent_subject_environment(cursor, context);
    recognize_nominal_object_reference(cursor, &environment)
        .map(PermanentOperandNode::BoundReference)
}

/// The shared subject reader does not declare a target for a reference.
pub(in crate::oracle::canonical) fn recognize_contextual_permanent_operand(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentOperandNode> {
    cursor.enter("permanent.subject.head");
    if super::reference::attached_object_reference_starts(cursor) {
        // `enchanted creature` / `equipped creature` is a reference, never
        // a target declaration or a query (CR 301.5a, 303.4a).
        let role = super::reference::recognize_attached_object_reference(
            cursor,
            context.source_attachment,
        )?;
        return Ok(PermanentOperandNode::BoundReference(
            PermanentEffectReference::Attached(role),
        ));
    }
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
    // `those creatures`, `they`, `each of them`, `that token`: the object set
    // the preceding action declared or produced, with its stated number.
    if super::object_set_reference_starts(cursor) {
        let environment = permanent_subject_environment(cursor, context);
        if environment.objects.is_some()
            || !cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("they"))
        {
            return recognize_object_set_reference(cursor, &environment);
        }
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
        let environment = permanent_subject_environment(cursor, context);
        let reference = recognize_bound_object_reference(cursor, &environment)?;
        return Ok(match reference {
            PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
            _ => PermanentOperandNode::BoundReference(reference),
        });
    }
    // `each of up to two target creatures`, `each of two target creatures`:
    // `each of` distributes the action over the declared targets, which an
    // action on several targets already does for each (CR 115.1, 608.2c).
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target") || raw.eq_ignore_ascii_case("up"))
            | cursor
                .token_slice(cursor.position() + 2, cursor.position() + 3)
                .first()
                .is_some_and(|token| token.token_type == "quantity")
    {
        cursor.enter("permanent.subject.distributive-targets");
        cursor.consume_phrase(&["each", "of"])?;
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
    // A bare plural collection (`other creatures you control`, `attacking
    // creatures`, `tapped creatures`; #18 object-set subject): the shared
    // collection frame reads its optional source exclusion and its state
    // adjectives. This lookahead only decides whether a characteristic or
    // state head follows.
    let head_offset = usize::from(
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("other")),
    );
    if cursor
        .token_slice(
            cursor.position() + head_offset,
            cursor.position() + head_offset + 1,
        )
        .first()
        .is_some_and(|token| {
            matches!(
                token.token_type.as_str(),
                "typeWord"
                    | "negativeTypeWord"
                    | "supertypeWord"
                    | "colorWord"
                    | "stateWord"
                    | "combatStateWord"
            )
        })
        || super::subtype_collection_head_starts(cursor, name_links)
    {
        let query = recognize_object_collection(cursor)?;
        cursor.require(
            query.denotes_battlefield_permanents(),
            "a battlefield permanent collection",
        )?;
        let exclude_source = query.exclude_source;
        return Ok(PermanentOperandNode::Collection {
            objects: query,
            exclude_source,
        });
    }
    recognize_permanent_operand(cursor, name_links, name_failure, context.source.as_ref())
}
