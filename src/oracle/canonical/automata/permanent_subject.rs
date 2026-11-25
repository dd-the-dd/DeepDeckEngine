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
        // `return target creature card ...` and `return up to one target
        // artifact card ... to the battlefield`: at most one declared card
        // enters; an empty declaration binds nothing (CR 115.1, 400.7).
        LocalEffectNode::CardAction(CardActionNode::Move {
            cards: CardOperandNode::Targets(targets),
            destination,
        }) if destination.zone == CardZoneName::Battlefield
            && !targets.unbounded
            && targets.cardinality.maximum == NumericAtom::Integer(1) =>
        {
            Some(Some(TargetSelector::Object(targets.predicate.clone())))
        }
        // `search your library for a basic land card, put it onto the
        // battlefield`: the one card the search may find enters (CR
        // 701.23a); the found card satisfies the search's card phrase.
        LocalEffectNode::ControllerLibrarySearch(search)
        | LocalEffectNode::OptionalLibrarySearch(search)
            if matches!(search.destination, SearchDestination::Battlefield { .. })
                && search.maximum == 1
                && search.count.is_none()
                && search.slots.is_empty() =>
        {
            Some(Some(TargetSelector::Object(search.filter.clone())))
        }
        // `put it|that card onto the battlefield`: the one bound card a
        // choice fixed (a search's found card), with no restated predicate.
        LocalEffectNode::CardAction(CardActionNode::Move {
            cards: CardOperandNode::Reference(CardCollectionReferenceNode::Selected),
            destination,
        }) if destination.zone == CardZoneName::Battlefield => Some(None),
        // `exile target creature, then return it to the battlefield`: the
        // one typed object a singular card pronoun named (CR 400.7); the
        // binding environment retypes it from that object's declaration
        // (`retype_moved_object`).
        LocalEffectNode::CardAction(CardActionNode::Move {
            cards:
                CardOperandNode::ObjectAntecedent {
                    reference: CardCollectionReferenceNode::Selected,
                    object,
                },
            destination,
        }) if destination.zone == CardZoneName::Battlefield
            && !matches!(
                object,
                PermanentEffectReference::TargetSet(_)
                    | PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens)
            ) =>
        {
            Some(None)
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
        // `return this card from your graveyard to the battlefield`: the
        // permanent the source card became (CR 400.7), named without a
        // declared predicate (`It gains haste`).
        LocalEffectNode::MoveSource(movement)
            if movement.source == LocalObjectReference::SourceSelf
                && movement.from == SourceZone::Graveyard
                && movement.to == SourceZone::Battlefield
                && movement.attached_to.is_none() =>
        {
            Some(None)
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
        // Another player's action ends the local permanent subject, except
        // the one permanent that player chose on resolution (`target opponent
        // chooses a creature they control. Destroy that creature.`).
        LocalEffectNode::Performed(_) => {
            Local(super::resolution_choice_subject(effect, effect_index))
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
        LocalEffectNode::ReferencedPermanentAction(node) => Local(Some(node.object)),
        LocalEffectNode::CreateExplicitTokens(_) => Local(Some(
            PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens),
        )),
        // CR 400.7: the effect that put one card onto the battlefield finds
        // the permanent it became (`that creature gains haste`).
        effect if moved_onto_battlefield_selector(effect).is_some() => Local(Some(
            PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield),
        )),
        LocalEffectNode::OptionalSequence(body) => optional_sequence_subject(body, previous),
        LocalEffectNode::Choose(node) => match &node.domain {
            ChoiceDomainNode::CreatureType
            | ChoiceDomainNode::Color
            | ChoiceDomainNode::CardName(_) => previous,
            ChoiceDomainNode::PermanentTargets(targets) => {
                Local(singular_permanent_target(targets, effect_index))
            }
            ChoiceDomainNode::PlayerTarget(_) | ChoiceDomainNode::Player(_) => Local(None),
            ChoiceDomainNode::Number(_) => previous,
            ChoiceDomainNode::Objects(_) => {
                Local(super::resolution_choice_subject(effect, effect_index))
            }
        },
        // A targeted source occupies the action's first slots; the recipient
        // is then no longer the action's sole singular referent.
        LocalEffectNode::DealDamage(node) => match (&node.source, &node.recipient) {
            // `Target creature deals damage equal to its power to itself`:
            // the targeted source is the action's only declared object.
            (PermanentOperandNode::Targets(targets), recipient)
                if !matches!(recipient, DamageRecipientNode::Targets(_)) =>
            {
                Local(singular_permanent_target(targets, effect_index))
            }
            (PermanentOperandNode::Targets(_), _) => Local(None),
            (_, DamageRecipientNode::Targets(targets)) => {
                Local(singular_permanent_target(targets, effect_index))
            }
            (_, DamageRecipientNode::BoundObject(reference)) => Local(Some(*reference)),
            // Damage a fixed source deals to players changes no permanent: a
            // lexical permanent antecedent stays (`deals 5 damage to target
            // creature and 2 damage to that creature's controller. If that
            // creature would die this turn, ...`), as for a draw; the
            // ability's default subject is exhausted as before.
            (
                PermanentOperandNode::Reference(_),
                DamageRecipientNode::Player(_)
                | DamageRecipientNode::PlayerSet(_)
                | DamageRecipientNode::BoundPlayer(_),
            ) => match previous {
                Local(Some(reference)) => Local(Some(reference)),
                _ => Local(None),
            },
            _ => Local(None),
        },
        // `This creature fights target creature you don't control`: the
        // second fighter is the action's only declared object when the
        // first is not declared (the lowering binds it then).
        LocalEffectNode::Fight(node) => Local(match (&node.first, &node.second) {
            (PermanentOperandNode::Targets(_), _) => None,
            (_, PermanentOperandNode::Targets(targets)) => {
                singular_permanent_target(targets, effect_index)
            }
            _ => None,
        }),
        // The single protected target is the next action's antecedent; a
        // shield over a declared source (`prevent all damage that would be
        // dealt by target creature`) names that source when no protected
        // target is declared.
        LocalEffectNode::PreventDamage(node) => Local(match prevention_declared_party(node) {
            Some(targets) => singular_permanent_target(targets, effect_index),
            None => None,
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
            | super::SingularPlayerAction::Proliferate(_)
            | super::SingularPlayerAction::CreateTokens(_)
            | super::SingularPlayerAction::ReferencedCards { .. } => Local(None),
            // A hand action changes no permanent: the subject antecedent stays.
            super::SingularPlayerAction::Hand(_)
            | super::SingularPlayerAction::ExtraTurn
            | super::SingularPlayerAction::Designation(_)
            | super::SingularPlayerAction::Skip(_)
            | super::SingularPlayerAction::Prohibition(_)
            | super::SingularPlayerAction::Status(_) => previous,
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
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::CopyStackObject(_)
        | LocalEffectNode::AdditionalPhases(_)
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

/// The permanent subject after an optional block (`you may ...`, CR
/// 608.2c): its last action's referent when that referent does not depend
/// on the block being performed nor on the block's own action positions
/// (the source, the permanent it is attached to, an event's object). A
/// referent the block's actions declare or produce exists only if the
/// block was performed and is bound inside the block, so it ends the scope
/// as any other new object does; a block that names no permanent keeps the
/// enclosing scope.
fn optional_sequence_subject(
    body: &EffectSequenceNode,
    previous: PermanentSubjectScope,
) -> PermanentSubjectScope {
    let inner = body
        .effects
        .iter()
        .enumerate()
        .fold(previous, |scope, (index, effect)| {
            permanent_subject_after_effect(effect, index, scope)
        });
    match inner {
        PermanentSubjectScope::Local(Some(reference)) if inner == previous => {
            PermanentSubjectScope::Local(Some(reference))
        }
        // The permanent the block's move put onto the battlefield is bound
        // by that move only (CR 400.7): when the block was not performed
        // the binding names no object, so a later action on it does nothing
        // (`You may put a creature card from your hand onto the battlefield.
        // That creature gains haste.`), as after an empty choice.
        PermanentSubjectScope::Local(Some(
            reference @ (PermanentEffectReference::Source(_)
            | PermanentEffectReference::Attached(_)
            | PermanentEffectReference::Observed(
                TypedObservedEventObject::EventPermanent
                | TypedObservedEventObject::EnteringPermanent
                | TypedObservedEventObject::CombatCounterpart
                | TypedObservedEventObject::MovedOntoBattlefield,
            )),
        )) => PermanentSubjectScope::Local(Some(reference)),
        PermanentSubjectScope::AbilityContext => previous,
        PermanentSubjectScope::Local(_) => PermanentSubjectScope::Local(None),
    }
}

fn prevention_declared_party(node: &PreventDamageEffectNode) -> Option<&TargetListNode> {
    super::prevention_declared_party_index(node).map(|(_, targets)| targets)
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

/// The declared selector of the plural slot a `TargetSet` reference names:
/// the one target-slot reader every declaring action shares.
fn target_set_selector(
    effects: &[LocalEffectNode],
    binding: TargetBinding,
) -> Option<TargetSelector> {
    target_slot_selector(effects.get(binding.effect_index)?, binding.slot_index).cloned()
}

/// The one plural permanent slot of a target list, as the object set a
/// later `those NOUNs`, `they` or `each of them` names (CR 115.1, 608.2c).
fn plural_target_set(targets: &TargetListNode, effect_index: usize) -> Option<ObjectSetAntecedent> {
    let [slot] = targets.slots.as_slice() else {
        return None;
    };
    (slot.cardinality.maximum > 1 && selector_guarantees_permanent(&slot.selector)).then(|| {
        ObjectSetAntecedent {
            denotes: ObjectSetDenotation::Bound(PermanentEffectReference::TargetSet(
                TargetBinding {
                    effect_index,
                    slot_index: 0,
                },
            )),
            selector: Some(slot.selector.clone()),
            number: GrammaticalNumber::Plural,
            tokens: false,
        }
    })
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
    // Several permanents chosen on resolution (CR 608.2d).
    if let Some(set) = super::resolution_choice_object_set(effect, effect_index) {
        return Some(set);
    }
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
    // `Choose any number of target creatures. Put a +1/+1 counter on each
    // of them.` / `deals 1 damage to each of up to two target creatures.
    // Those creatures can't block this turn.`: the plural slot an action
    // declared outside an ordinary permanent operand.
    let declared = match effect {
        LocalEffectNode::Choose(node) => node.permanent_targets(),
        LocalEffectNode::DealDamage(node)
            if !matches!(node.source, PermanentOperandNode::Targets(_)) =>
        {
            match &node.recipient {
                DamageRecipientNode::Targets(targets) => Some(targets),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(targets) = declared {
        return plural_target_set(targets, effect_index);
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
        PermanentOperandNode::Targets(targets) => plural_target_set(targets, effect_index),
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
        ObjectPredicate::Designation(_)
        | ObjectPredicate::Relation(_)
        | ObjectPredicate::Qualifier(_) => false,
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
    let installed = installed_binding_environment(cursor);
    environment.objects = installed.objects;
    // The selector the program parent published for the same antecedent
    // (a moved permanent retyped from the object it was, CR 400.7) when
    // the context carries none; a different antecedent is never borrowed.
    if let (Some(object), Some(published)) = (environment.object.as_mut(), installed.object)
        && object.selector.is_none()
        && object.reference == published.reference
    {
        object.selector = published.selector;
    }
    inherit_installed_action_result(&mut environment, cursor);
    environment
}

/// The one object `effect`, action `effect_index` of its program, selected
/// or moved on resolution when its selection states exactly one (CR
/// 608.2c): a permanent sacrificed by choice, a discarded or milled card,
/// the revealed or exiled top card of a library, a revealed chosen card, or
/// the card an exile-until stopped at. Its selector is what the selection
/// guarantees; none of these is still a battlefield permanent afterwards.
/// Any other action produces no such object.
pub(in crate::oracle::canonical) fn action_result_object(
    effect: &LocalEffectNode,
    effect_index: usize,
) -> Option<ActionResultObject> {
    let one = NumericExpression::from(1);
    let card = || ObjectPredicate::ObjectKind(ObjectKind::Card);
    let result = |predicate: ObjectPredicate, verb: Option<ActionResultVerb>| ActionResultObject {
        reference: ActionResultReference {
            effect_index,
            permanent: false,
        },
        selector: TargetSelector::Object(predicate),
        number: GrammaticalNumber::Singular,
        verb,
    };
    let moved_verb = |destination: &CardMoveDestinationNode| {
        (destination.zone == CardZoneName::Exile).then_some(ActionResultVerb::Exiled)
    };
    match effect {
        LocalEffectNode::PlayerAction(PlayerActionNode {
            action: SingularPlayerAction::Sacrifice(sacrifice),
            ..
        }) if sacrifice.count == one => Some(result(
            sacrifice.object.predicate.clone(),
            Some(ActionResultVerb::Sacrificed),
        )),
        LocalEffectNode::Sacrifice(SacrificeEffectNode::Selected { object, count })
            if *count == one =>
        {
            Some(result(
                object.predicate.clone(),
                Some(ActionResultVerb::Sacrificed),
            ))
        }
        LocalEffectNode::PlayerAction(PlayerActionNode {
            action: SingularPlayerAction::Discard(discard),
            ..
        })
        | LocalEffectNode::DiscardCards(discard)
            if discard.quantity == DiscardQuantity::Count(one.clone()) =>
        {
            Some(result(card(), Some(ActionResultVerb::Discarded)))
        }
        LocalEffectNode::PlayerAction(PlayerActionNode {
            action: SingularPlayerAction::Mill(count),
            ..
        }) if *count == one => Some(result(card(), Some(ActionResultVerb::Milled))),
        LocalEffectNode::MillCards(mill) if mill.count == one => {
            Some(result(card(), Some(ActionResultVerb::Milled)))
        }
        LocalEffectNode::CardAction(CardActionNode::Reveal(operand)) => {
            singular_card_operand(operand).map(|predicate| {
                result(
                    predicate.unwrap_or_else(card),
                    Some(ActionResultVerb::Revealed),
                )
            })
        }
        LocalEffectNode::CardAction(CardActionNode::Move { cards, destination })
            if destination.zone != CardZoneName::Battlefield =>
        {
            singular_card_operand(cards)
                .map(|predicate| result(predicate.unwrap_or_else(card), moved_verb(destination)))
        }
        // `exile cards from the top of your library until you exile a
        // nonland card`: the one card the exile stopped at.
        LocalEffectNode::CardAction(CardActionNode::RevealUntil {
            quantity,
            predicate,
            verb: CardCollectionVerb::Exiled,
            ..
        }) if *quantity == one => Some(result(predicate.clone(), Some(ActionResultVerb::Exiled))),
        // `manifest dread` / `manifest the top card of your library`: the
        // one card put onto the battlefield face down as a 2/2 creature (CR
        // 701.40a, 701.62a), still a permanent (`attach this Equipment to
        // that creature`).
        LocalEffectNode::ControllerKeywordAction(node)
            if node.action == ControllerKeywordAction::ManifestDread
                || (matches!(
                    node.action,
                    ControllerKeywordAction::Manifest | ControllerKeywordAction::Cloak
                ) && node
                    .quantity
                    .as_ref()
                    .is_none_or(|quantity| *quantity == one)) =>
        {
            Some(ActionResultObject {
                reference: ActionResultReference {
                    effect_index,
                    permanent: true,
                },
                selector: TargetSelector::Object(ObjectPredicate::CardType(
                    ObjectCardType::Creature,
                )),
                number: GrammaticalNumber::Singular,
                verb: None,
            })
        }
        // Another player's action produces the same object (`target
        // opponent reveals a card at random from their hand`).
        LocalEffectNode::Performed(node) => action_result_object(&node.effect, effect_index),
        _ => None,
    }
}

/// The predicate of a card operand that selects exactly one new card (the
/// top card of a library, one chosen or random card), `Some(None)` when it
/// states none. A reference to cards already observed selects nothing new.
fn singular_card_operand(operand: &CardOperandNode) -> Option<Option<ObjectPredicate>> {
    let one = NumericExpression::from(1);
    match operand {
        // `the top card of each player's library` is one card per library.
        CardOperandNode::LibraryTop(top)
            if top.count == one && !matches!(top.owner, CardLibraryOwnerNode::Players(_)) =>
        {
            Some(None)
        }
        CardOperandNode::Selection(selection)
            if selection.quantity == CardSelectionQuantityNode::Exactly(one.clone()) =>
        {
            Some(selection.predicate.clone())
        }
        CardOperandNode::HiddenChoice(choice)
            if choice.quantity == CardSelectionQuantityNode::Exactly(one.clone()) =>
        {
            Some(Some(choice.predicate.clone()))
        }
        _ => None,
    }
}

/// True when `effect` neither selects nor moves another object nor declares
/// one a later anaphor could name instead, so an earlier action result stays
/// the program's nearest result (CR 608.2c): life changes, mana, draws,
/// damage and stat changes (whose own declarations are compared by effect
/// index), and moves or reveals of the cards already observed (`put that
/// card into your hand`).
fn keeps_action_result(effect: &LocalEffectNode) -> bool {
    match effect {
        LocalEffectNode::GainLife(_)
        | LocalEffectNode::LoseLife(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::AddMana(_)
        | LocalEffectNode::DrawCards(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::DealDamage(_)
        | LocalEffectNode::ModifyStats(_)
        | LocalEffectNode::TemporaryKeywordGrant(_) => true,
        LocalEffectNode::PlayerAction(node) => matches!(
            node.action,
            SingularPlayerAction::Draw(_)
                | SingularPlayerAction::Life(_)
                | SingularPlayerAction::Counters(_)
                | SingularPlayerAction::Scry(_)
        ),
        LocalEffectNode::Performed(node) => keeps_action_result(&node.effect),
        LocalEffectNode::CardAction(
            CardActionNode::Reveal(CardOperandNode::Reference(_))
            | CardActionNode::Move {
                cards: CardOperandNode::Reference(_),
                ..
            },
        ) => true,
        _ => false,
    }
}

/// True when `reference` is the created tokens and the latest creation of
/// the program created exactly one token (`Create a token that's a copy of
/// target creature you control. Sacrifice it at the beginning of the next
/// end step.`): the one permanent a zone action on `it` names (CR 111.1,
/// 608.2c). Several created tokens are a set, never `it`.
pub(in crate::oracle::canonical) fn singular_created_token_antecedent(
    reference: PermanentEffectReference,
    effects: &[LocalEffectNode],
) -> bool {
    reference == PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens)
        && effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                LocalEffectNode::CreateExplicitTokens(node) => Some(node),
                _ => None,
            })
            .is_some_and(|node| {
                node.additional.is_empty() && node.body.quantity == NumericExpression::from(1)
            })
}

/// The action result a reference read after `effects` names: the latest
/// action that selected or moved exactly one object, when every action
/// after it keeps it the nearest result (`keeps_action_result`). The
/// program parent publishes it through the binding environment.
pub(in crate::oracle::canonical) fn program_action_result(
    effects: &[LocalEffectNode],
) -> Option<ActionResultObject> {
    for (effect_index, effect) in effects.iter().enumerate().rev() {
        if let Some(result) = action_result_object(effect, effect_index) {
            return Some(result);
        }
        if !keeps_action_result(effect) {
            return None;
        }
    }
    None
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
                .map(|slot| &slot.selector)
                .or_else(|| super::resolution_choice_selector(effect, slot_index));
        }
        // A permanent another player chose on resolution.
        LocalEffectNode::Performed(_) => {
            return super::resolution_choice_selector(effect, slot_index);
        }
        // Actions that declare their targets outside a permanent operand
        // expose the same declared selector for nominal anaphora.
        // A targeted source occupies the action's first slots (the lowering
        // binds them first); otherwise the recipient's slots are the
        // action's declared objects.
        LocalEffectNode::DealDamage(node) => {
            return match (&node.source, &node.recipient) {
                (PermanentOperandNode::Targets(targets), _)
                | (_, DamageRecipientNode::Targets(targets)) => targets.slots.get(slot_index),
                _ => None,
            }
            .map(|slot| &slot.selector);
        }
        // The second fighter's slots are bound only when the first fighter
        // declares none (the fight lowering keeps that binding then).
        LocalEffectNode::Fight(node) => {
            return match (&node.first, &node.second) {
                (PermanentOperandNode::Targets(targets), _)
                | (_, PermanentOperandNode::Targets(targets)) => targets.slots.get(slot_index),
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
            return prevention_declared_party(node)
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

/// The singular `card` noun (not a possessive) `offset` tokens after the
/// cursor. Lexical lookahead only.
fn card_noun_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor
        .token_slice(cursor.position() + offset, cursor.position() + offset + 1)
        .first()
        .filter(|token| token.token_type != "possessiveTypeWord")
        .is_some_and(|token| {
            crate::oracle::characteristic_vocabulary::lookup_object_domain(
                &token.raw,
                crate::oracle::characteristic_vocabulary::AcceptedForms::SingularOnly,
            ) == Some(ObjectKind::Card)
        })
}

/// The object a copy reads (CR 707.2): only its copiable values, in
/// whatever zone it is. A reference resolves through the one binding
/// environment like any other reference, with the zone typing of a
/// characteristic reader: besides the battlefield antecedents the shared
/// contextual operand reads, it may name a card the program declared or
/// moved (`a copy of that card`, `a copy of the exiled card`, `a copy of
/// it`) or the triggering event's object after it left the battlefield, as
/// it last existed (`a copy of that creature` after `a creature dies`, CR
/// 400.7, 608.2h). Targets, the source and created tokens stay with the
/// shared contextual operand. Missing, ambiguous and zone-incompatible
/// antecedents keep distinct diagnostics.
pub(in crate::oracle::canonical) fn recognize_copied_object_operand(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentOperandNode> {
    // A printed name or `this NOUN` is the source.
    if super::reference::source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        name_links,
        name_failure,
    ) {
        return recognize_contextual_permanent_operand(cursor, name_links, name_failure, context);
    }
    let mut environment = permanent_subject_environment(cursor, context);
    environment.card_target = installed_binding_environment(cursor).card_target;
    let first = cursor.peek().map(str::to_ascii_lowercase);
    let participle = match first.as_deref() {
        Some("the") => cursor
            .peek_ahead(1)
            .and_then(super::ActionResultVerb::from_word)
            .filter(|_| card_noun_at(cursor, 2)),
        _ => None,
    };
    // `the exiled card` / `the discarded card`: the one card an earlier
    // action of this program moved with that verb.
    if let Some(verb) = participle {
        cursor.enter("copy.object.participle");
        let reference = resolve_copied_card_antecedent(&environment, Some(verb))
            .map_err(|expected| cursor.fail(expected))?;
        cursor.advance_to(cursor.position() + 3)?;
        cursor.enter("copy.object.complete");
        return Ok(PermanentOperandNode::BoundReference(reference));
    }
    // `that card` / `the card`: a card is no permanent; only the card
    // antecedents of the program name it.
    if matches!(first.as_deref(), Some("that" | "the")) && card_noun_at(cursor, 1) {
        cursor.enter("copy.object.card-antecedent");
        let reference = resolve_copied_card_antecedent(&environment, None)
            .map_err(|expected| cursor.fail(expected))?;
        cursor.advance_to(cursor.position() + 2)?;
        cursor.enter("copy.object.complete");
        return Ok(PermanentOperandNode::BoundReference(reference));
    }
    // `it`: the proven pronoun antecedent, else a card of the program.
    if cursor
        .peek()
        .is_some_and(super::reference::is_source_anaphor)
    {
        cursor.enter("copy.object.pronoun");
        let reference = resolve_copied_pronoun_antecedent(&environment)
            .and_then(|reference| {
                copied_reference_admitted(&environment, reference).map(|()| reference)
            })
            .map_err(|expected| cursor.fail(expected))?;
        cursor.advance_to(cursor.position() + 1)?;
        cursor.enter("copy.object.complete");
        return Ok(match reference {
            PermanentEffectReference::Source(source) => PermanentOperandNode::Reference(source),
            reference => PermanentOperandNode::BoundReference(reference),
        });
    }
    // `that creature`: the nominal antecedent read for its copiable values,
    // which a departed event object keeps as it last existed. A spell is a
    // stack object no token copies (CR 707.10); the shared operand keeps it.
    if first.as_deref() == Some("that")
        && bound_object_reference_starts(cursor)
        && !cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("spell"))
    {
        let start = cursor.clone();
        let reference = recognize_nominal_object_read(cursor, &environment)?;
        cursor.enter("copy.object.zone");
        if let Err(expected) = copied_reference_admitted(&environment, reference) {
            *cursor = start;
            cursor.enter("copy.object.zone");
            return Err(cursor.fail(expected));
        }
        cursor.enter("copy.object.complete");
        return Ok(PermanentOperandNode::BoundReference(reference));
    }
    recognize_contextual_permanent_operand(cursor, name_links, name_failure, context)
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
    // `a second|third target creature` is the same slot head as `another
    // target creature` (the shared target-list lookahead).
    if cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("another")
            || raw.eq_ignore_ascii_case("up")
    }) || cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity")
        || super::expressed_target_count_starts(cursor)
        || super::ordinal_target_head_follows(cursor)
    {
        // The shared lowering reads a slot's typed count (`X target`, `any
        // number of target`); see `lower_ordinary_permanent_subject`.
        let targets =
            super::recognize_target_list_with_counts(cursor, super::TargetCountSupport::Expressed)?;
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

#[cfg(test)]
#[path = "action_result_antecedent_tests.rs"]
mod action_result_antecedent_tests;
