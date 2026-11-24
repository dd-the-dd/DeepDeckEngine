//! Typed composition over one immutable token stream. Each child returns its
//! own semantic node; the parent owns separators and complete consumption.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct SyntaxNode<T> {
    pub tokens: std::ops::Range<usize>,
    pub value: T,
}

/// A triggered ability body: one effect program, or printed modes chosen as
/// the trigger is put on the stack (CR 700.2b).
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum TriggerBodyNode {
    Program(EffectProgramNode),
    Modal(EffectSequenceModalNode),
}

impl TriggerBodyNode {
    pub(in crate::oracle::canonical) fn action_count(&self) -> usize {
        match self {
            Self::Program(program) => program.action_count(),
            Self::Modal(modal) => modal.action_count(),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct TriggeredProgramNode {
    pub event: SyntaxNode<GenericTriggerEventNode>,
    pub guard: Option<SyntaxNode<SentenceConditionNode>>,
    pub program: SyntaxNode<TriggerBodyNode>,
    /// `This ability triggers only once each turn.`
    pub once_each_turn: bool,
}

pub(in crate::oracle::canonical) fn recognize_trigger_program_tail(
    cursor: &mut OracleTokenCursor<'_>,
    event: SyntaxNode<GenericTriggerEventNode>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    source_attachment: Option<AttachedObjectRole>,
) -> MachineResult<TriggeredProgramNode> {
    let start = cursor.position();
    // The guard is read before the effect program, so only the ability's own
    // source can be a referent: no declared target or observed object is
    // bound yet, and the shared condition atoms fail explicitly otherwise.
    let mut guard_context = trigger_guard_context(&event.value);
    guard_context.frame = event_frame(&event.value);
    guard_context.source_attachment = source_attachment;
    // The guard's references (`that creature's power`) resolve against the
    // same binding environment as the body's: the frame the event binds.
    let enclosing =
        cursor.replace_binding_environment(Some(std::sync::Arc::new(super::BindingEnvironment {
            frame: guard_context.frame.clone(),
            ..super::BindingEnvironment::default()
        })));
    let guard = recognize_trigger_guard(
        cursor,
        &SentenceConditionContext {
            name_links,
            name_failure,
            effects: &guard_context,
            card_target: None,
            card_focus: false,
            departed_targets: &[],
        },
    );
    cursor.replace_binding_environment(enclosing);
    let guard = guard?.map(|value| SyntaxNode {
        tokens: start..cursor.position(),
        value,
    });
    if guard.is_some() {
        cursor.enter("trigger.program.guard-separator");
        cursor.consume(",")?;
    }
    cursor.enter("trigger.program.body");
    // Ability source and observed object have separate roles. OR branches
    // select an event; they do not multiply objects within its occurrence.
    let mut context = trigger_guard_context(&event.value);
    if let Some(object) = event.value.observed_object() {
        context = context.with_observed_event_object(object);
    }
    // An intervening `if` whose subject is the source (`if this permanent
    // has a time counter on it, remove a time counter from it`) makes the
    // source the antecedent of the body's `it` when the event bound none.
    if context.source_anaphor_binding == super::SourceAnaphorBinding::Unproved
        && guard
            .as_ref()
            .is_some_and(|guard| guard_subject_is_source(&guard.value))
    {
        context = context.with_source_event_subject(LocalObjectReference::SourceSelf);
    }
    let mut context = triggered_body_context(context, source_attachment);
    context.frame = event_frame(&event.value);
    // An intervening `if` about one permanent (`if this permanent is an
    // enchantment`) makes it the body's lexical antecedent (`it becomes`).
    if let Some(SyntaxNode {
        value:
            BooleanConditionNode::Atom(ConditionAtom::ObjectSatisfies(ReferencedObjectConditionNode {
                object: ConditionObjectReference::Permanent(reference),
                ..
            })),
        ..
    }) = &guard
    {
        context.permanent_subject_scope = PermanentSubjectScope::Local(Some(*reference));
    }
    let start = cursor.position();
    // The event owns the `that many`/`that much` quantity; the body only reads it.
    cursor.set_event_numeric_binding(event.value.numeric_binding());
    // Modes are declared with the trigger's other choices (CR 700.2b); they
    // read the event's bindings, while their sentence frames stay those the
    // shared modal lowering owns.
    let modal_context = triggered_modal_body_context(trigger_guard_context(&event.value), &context);
    let value =
        match recognize_container_body(cursor, name_links, name_failure, &context, &modal_context)?
        {
            TriggerBodyNode::Modal(modal) => {
                return Ok(TriggeredProgramNode {
                    event,
                    guard,
                    program: SyntaxNode {
                        tokens: start..cursor.position(),
                        value: TriggerBodyNode::Modal(modal),
                    },
                    once_each_turn: false,
                });
            }
            TriggerBodyNode::Program(value) => value,
        };
    let program_end = cursor.position();
    let once_each_turn = recognize_optional_trigger_limit(cursor)?;
    // A token reminder may follow the limit sentence; it still belongs to the
    // body's last catalog token creation and is validated against its
    // catalog by the shared token reminder reader. Any other parenthetical
    // stays unread and fails at completion.
    if once_each_turn
        && cursor.peek() == Some("(")
        && let Some(creation) = value
            .inner_sequence()
            .effects
            .last()
            .and_then(super::reminded_token_creation)
    {
        super::recognize_token_creation_reminder(cursor, &creation)?;
    }
    cursor.enter("trigger.program.complete");
    cursor.finish()?;
    Ok(TriggeredProgramNode {
        event,
        guard,
        program: SyntaxNode {
            tokens: start..program_end,
            value: TriggerBodyNode::Program(value),
        },
        once_each_turn,
    })
}

/// The context every triggered ability's program body is read with: the
/// parent's source and event bindings, the spell-or-battlefield zone-change
/// domains, sentence conditionals and `unless` frames lowered by the shared
/// sequence, and `This ability triggers only ...` owned by the trigger.
/// Saga chapters (CR 715.2b) and event triggers share it.
pub(in crate::oracle::canonical) fn triggered_body_context(
    context: EffectRecognitionContext,
    source_attachment: Option<AttachedObjectRole>,
) -> EffectRecognitionContext {
    let mut context = context
        .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield)
        .with_sentence_conditionals()
        .with_unless_frames();
    context.ability_restriction_owner = AbilityRestrictionOwner::Triggered;
    context.source_attachment = source_attachment;
    context
}

/// The context printed modes are read with: the trigger's own source and
/// event bindings (`base`) plus the observed object, attachment and frame of
/// its program context. Sentence frames inside a mode belong to the shared
/// modal lowering, so the program-only settings are not copied. Event
/// triggers and Saga chapters share it.
pub(in crate::oracle::canonical) fn triggered_modal_body_context(
    mut base: EffectRecognitionContext,
    program: &EffectRecognitionContext,
) -> EffectRecognitionContext {
    base.observed_event_object = program.observed_event_object;
    base.source_attachment = program.source_attachment;
    base.frame = program.frame.clone();
    base
}

/// Events whose occurrence names exactly one player that the runtime records
/// as the triggering player: the player dealt combat damage, or the player
/// whose turn step began when the step owner is not fixed to the controller.
fn event_names_player(event: &GenericTriggerEventNode) -> bool {
    matches!(
        event,
        GenericTriggerEventNode::Local(
            LocalTriggerEventNode::CombatDamageToPlayer { .. }
                | LocalTriggerEventNode::AttachedObjectEvent {
                    action: AttachedObjectEventAction::DealCombatDamageToPlayer,
                    ..
                }
        )
    ) || matches!(
        event,
        GenericTriggerEventNode::Step(StepEventNode { player, .. })
            if *player != StepPlayerScope::AbilityController
    ) || matches!(
        event,
        GenericTriggerEventNode::Local(LocalTriggerEventNode::PlayerEvent {
            actions,
            one_or_more: false,
            ..
        }) if actions.iter().all(|action| action.records_acting_player())
    ) || matches!(
        event,
        // `a creature you control deals combat damage to a player`: each
        // damage record names the damaged player (CR 510.2).
        GenericTriggerEventNode::Object(ObjectEventNode {
            verb: ObjectEventVerb::DealCombatDamageToPlayer,
            subject,
            ..
        }) if subject.multiplicity == EventMultiplicity::PerObject
    ) || event_damage_complement(event)
        .is_some_and(|complement| matches!(complement.recipient, Some(DamageRecipient::Player(_))))
        // The drawing player (CR 121.1) and the caster of a cast spell (CR
        // 601.2): both records bind the triggering player. A copied spell has
        // no caster and the source spell's caster is its controller.
        || matches!(
            event,
            GenericTriggerEventNode::Local(LocalTriggerEventNode::CardDrawn { .. })
        )
        || matches!(
            event,
            GenericTriggerEventNode::SpellCast(node) if !node.also_copy && !node.source_spell
        )
}

/// The dealt-damage complement of a source, attached or object damage event.
fn event_damage_complement(event: &GenericTriggerEventNode) -> Option<&DamageDealtComplement> {
    match event {
        GenericTriggerEventNode::Local(LocalTriggerEventNode::DamageDealt {
            complement, ..
        }) => Some(complement),
        GenericTriggerEventNode::Object(ObjectEventNode {
            verb: ObjectEventVerb::DealDamage,
            damage,
            ..
        }) => damage.as_ref(),
        _ => None,
    }
}

/// Attack events by the source or its attached creature: the runtime records
/// the attacked player (or planeswalker controller) as the triggering player.
fn event_names_defending_player(event: &GenericTriggerEventNode) -> bool {
    matches!(
        event,
        GenericTriggerEventNode::Local(
            LocalTriggerEventNode::SourceAttacked { .. }
                | LocalTriggerEventNode::AttachedObjectEvent {
                    action: AttachedObjectEventAction::Attack,
                    ..
                }
        )
    ) || matches!(
        // A modified attack of the source (`attacks alone`, `attacks and
        // isn't blocked`): the generic attack and unblocked patterns record
        // the defending player too.
        event,
        GenericTriggerEventNode::SourceEvent(node)
            if node.verbs == [super::SourceEventVerb::Attacked]
    ) || matches!(
        // One attacking creature: the generic attack pattern binds the
        // player it attacks.
        event,
        GenericTriggerEventNode::Object(super::ObjectEventNode {
            verb: super::ObjectEventVerb::Attack,
            subject,
            ..
        }) if subject.multiplicity == super::EventMultiplicity::PerObject
    )
}

/// The public zone a battlefield departure put the event's object into: a
/// death puts it into its owner's graveyard (CR 700.4). A departure to an
/// unwritten zone, or from another zone, binds no card.
pub(in crate::oracle::canonical) fn event_card_zone(
    event: &GenericTriggerEventNode,
) -> Option<CardZoneName> {
    if let GenericTriggerEventNode::SourceEvent(node) = event {
        return match node.verbs.as_slice() {
            [
                super::SourceEventVerb::Died
                | super::SourceEventVerb::PutIntoGraveyardFromBattlefield,
            ] => Some(CardZoneName::Graveyard),
            [super::SourceEventVerb::PutIntoExileFromBattlefield] => Some(CardZoneName::Exile),
            _ => None,
        };
    }
    let died = match event {
        GenericTriggerEventNode::SourceTransition(node) => {
            node.event == SourceTriggerEventKind::Died && !node.is_union()
        }
        GenericTriggerEventNode::PermanentTransition {
            kind: PermanentTransitionKind::Died,
            union_kind: None,
            ..
        } => true,
        GenericTriggerEventNode::Object(ObjectEventNode {
            subject,
            verb: ObjectEventVerb::Die | ObjectEventVerb::PutIntoGraveyardFromBattlefield,
            ..
        }) => subject.multiplicity == EventMultiplicity::PerObject,
        GenericTriggerEventNode::Local(LocalTriggerEventNode::AttachedObjectEvent {
            action: AttachedObjectEventAction::Die,
            ..
        }) => true,
        _ => false,
    };
    died.then_some(CardZoneName::Graveyard)
}

/// The object the event record binds, with the predicate the event wrote for
/// it. The entering permanent keeps its observed reference; every other
/// record binds its object as the triggering permanent: the damaged
/// permanent of a damage event, else the damage source (CR 120.1), the
/// attacker (CR 508.1), the dying permanent (CR 700.4).
fn event_object_slot(event: &GenericTriggerEventNode) -> Option<EventObjectSlot> {
    let permanent = |predicate: ObjectPredicate, subject: bool, departed: bool| {
        Some(EventObjectSlot {
            reference: PermanentEffectReference::Observed(TypedObservedEventObject::EventPermanent),
            predicate,
            subject,
            departed,
            controller: None,
        })
    };
    let creature = || ObjectPredicate::CardType(ObjectCardType::Creature);
    match event {
        GenericTriggerEventNode::PermanentTransition {
            kind,
            subjects,
            union_kind: None,
            ..
        } => {
            let predicates = subjects
                .alternatives
                .iter()
                .map(|branch| match branch {
                    PermanentDiedEventBranchNode::Filtered { predicate, .. } => {
                        Some(predicate.clone())
                    }
                    PermanentDiedEventBranchNode::Source(_) => None,
                })
                .collect::<Option<Vec<_>>>()?;
            let predicate = match predicates.as_slice() {
                [] => return None,
                [single] => single.clone(),
                _ => ObjectPredicate::Union(predicates),
            };
            Some(EventObjectSlot {
                reference: PermanentEffectReference::Observed(match kind {
                    PermanentTransitionKind::Entered => TypedObservedEventObject::EnteringPermanent,
                    PermanentTransitionKind::Died => TypedObservedEventObject::EventPermanent,
                }),
                predicate,
                subject: true,
                departed: *kind == PermanentTransitionKind::Died,
                controller: shared_branch_controller(subjects),
            })
        }
        GenericTriggerEventNode::Object(ObjectEventNode {
            subject,
            verb,
            damage,
            ..
        }) if subject.multiplicity == EventMultiplicity::PerObject => {
            // The event's subject keeps the controller relation it was
            // written with (`a creature an opponent controls`).
            let subject_slot = |departed: bool| {
                permanent(subject.query.predicate.clone(), true, departed).map(|mut slot| {
                    slot.controller = subject.query.controller;
                    slot
                })
            };
            match verb {
                ObjectEventVerb::Die => subject_slot(true),
                ObjectEventVerb::Attack
                | ObjectEventVerb::BecomeBlocked
                | ObjectEventVerb::BeDealtDamage(_)
                | ObjectEventVerb::DealCombatDamageToPlayer => subject_slot(false),
                ObjectEventVerb::DealDamage => match damage.as_ref()?.recipient.as_ref()? {
                    DamageRecipient::Permanent(recipient) => {
                        permanent(recipient.predicate.clone(), false, false)
                    }
                    DamageRecipient::Player(_) => subject_slot(false),
                },
                _ => None,
            }
        }
        // The permanent the source is attached to as the event's subject
        // (`whenever enchanted land becomes tapped`, `enchanted creature is
        // dealt damage`, `equipped creature deals damage`): the attached
        // reference, live while it is on the battlefield. A death keeps the
        // observed record of the dying permanent its parent binds.
        GenericTriggerEventNode::Local(
            LocalTriggerEventNode::AttachedObjectEvent {
                role,
                action:
                    AttachedObjectEventAction::BecomeTapped
                    | AttachedObjectEventAction::BecomeBlocked
                    | AttachedObjectEventAction::TappedForMana,
                ..
            }
            | LocalTriggerEventNode::AttachedObjectBlock { role, .. }
            | LocalTriggerEventNode::AttachedObjectTargeted { role, .. }
            | LocalTriggerEventNode::AttachedObjectDealtDamage { role, .. },
        ) => attached_event_subject(*role),
        GenericTriggerEventNode::Local(LocalTriggerEventNode::DamageDealt {
            subject: DamageDealer::Attached(role),
            complement,
        }) if !matches!(complement.recipient, Some(DamageRecipient::Permanent(_))) => {
            attached_event_subject(*role)
        }
        GenericTriggerEventNode::Local(LocalTriggerEventNode::DamageDealt {
            complement, ..
        }) => match complement.recipient.as_ref()? {
            DamageRecipient::Permanent(recipient) => {
                permanent(recipient.predicate.clone(), false, false)
            }
            DamageRecipient::Player(_) => None,
        },
        // `the upkeep of enchanted creature's controller`: the event names the
        // permanent the source enchants with that nominal (CR 303.4).
        GenericTriggerEventNode::Step(StepEventNode {
            player: StepPlayerScope::AttachedPermanentController(nominal),
            ..
        }) => Some(EventObjectSlot {
            reference: PermanentEffectReference::Attached(AttachedObjectRole::Enchanted),
            predicate: nominal.as_predicate(),
            subject: false,
            departed: false,
            controller: None,
        }),
        GenericTriggerEventNode::Local(LocalTriggerEventNode::AttachedObjectEvent {
            action:
                AttachedObjectEventAction::Attack | AttachedObjectEventAction::DealCombatDamageToPlayer,
            ..
        }) => permanent(creature(), true, false),
        // `When enchanted creature dies`: the dying permanent the source was
        // attached to, read by its last known information (CR 603.10a).
        GenericTriggerEventNode::Local(LocalTriggerEventNode::AttachedObjectEvent {
            action: AttachedObjectEventAction::Die,
            ..
        }) => permanent(creature(), true, true),
        _ => None,
    }
}

/// What the attachment guarantees about the attached permanent: an
/// Equipment equips only a creature (CR 301.5); an Aura's attached object is
/// a permanent of the kind its enchant ability names, unknown here.
fn attached_nominal(role: AttachedObjectRole) -> ObjectPredicate {
    match role {
        AttachedObjectRole::Equipped => ObjectPredicate::CardType(ObjectCardType::Creature),
        AttachedObjectRole::Enchanted => ObjectPredicate::ObjectKind(ObjectKind::Permanent),
    }
}

/// The attached permanent as the grammatical subject of the event.
fn attached_event_subject(role: AttachedObjectRole) -> Option<EventObjectSlot> {
    Some(EventObjectSlot {
        reference: PermanentEffectReference::Attached(role),
        predicate: attached_nominal(role),
        subject: true,
        departed: false,
        controller: None,
    })
}

/// The controller relation every branch of a transition event writes for
/// its object (`a creature an opponent controls dies`), when they share one.
fn shared_branch_controller(subjects: &PermanentDiedEventNode) -> Option<ObjectControllerScope> {
    let mut controllers = subjects.alternatives.iter().map(|branch| match branch {
        PermanentDiedEventBranchNode::Filtered { controller, .. } => *controller,
        PermanentDiedEventBranchNode::Source(_) => None,
    });
    let first = controllers.next()??;
    controllers
        .all(|controller| controller == Some(first))
        .then_some(first)
}

/// A source block event whose every branch has a counterpart (`blocks or
/// becomes blocked by a creature`): the runtime binds the other combatant.
fn event_names_combat_counterpart(event: &GenericTriggerEventNode) -> bool {
    let GenericTriggerEventNode::SourceEvent(node) = event else {
        return false;
    };
    !node.verbs.is_empty()
        && node.verbs.iter().all(|verb| {
            node.modifiers.iter().any(|modifier| {
                matches!(modifier, super::SourceEventModifier::Counterpart(counterpart) if counterpart.verb == *verb)
            })
        })
}

/// The references the triggering event binds (`that player`, `defending
/// player`, the other combatant, the event object, its card, its spell).
fn event_frame(event: &GenericTriggerEventNode) -> super::FrameBindings {
    super::FrameBindings {
        event_player: event_names_player(event),
        defending_player: event_names_defending_player(event),
        combat_counterpart: event_names_combat_counterpart(event),
        event_object: event_object_slot(event),
        event_card: event_card_zone(event),
        // A cast record binds its spell; a copy has no cast record.
        event_spell: matches!(
            event,
            GenericTriggerEventNode::SpellCast(node) if !node.also_copy && !node.source_spell
        ),
    }
}

/// True when every atom of an intervening `if` is about the ability's own
/// source permanent.
fn guard_subject_is_source(guard: &SentenceConditionNode) -> bool {
    match guard {
        super::BooleanConditionNode::Atom(super::ConditionAtom::ObjectSatisfies(node)) => {
            node.object
                == super::ConditionObjectReference::Permanent(
                    super::PermanentEffectReference::Source(LocalObjectReference::SourceSelf),
                )
        }
        super::BooleanConditionNode::And(operands) | super::BooleanConditionNode::Or(operands) => {
            !operands.is_empty() && operands.iter().all(guard_subject_is_source)
        }
        super::BooleanConditionNode::Not(operand) => guard_subject_is_source(operand),
        super::BooleanConditionNode::Atom(_) => false,
    }
}

/// The referents available to a trigger's intervening `if` (CR 603.4): the
/// ability source and, for a source event, the source as the event subject.
fn trigger_guard_context(event: &GenericTriggerEventNode) -> EffectRecognitionContext {
    let mut context = match event {
        GenericTriggerEventNode::SourceTransition(_)
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::SourceAttacked { .. })
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::CombatDamageToPlayer { .. })
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::DamageDealt {
            subject: DamageDealer::Source(_),
            ..
        })
        | GenericTriggerEventNode::Local(LocalTriggerEventNode::SourceDealtDamage { .. })
        | GenericTriggerEventNode::SourceCounterState(_)
        | GenericTriggerEventNode::SourceEvent(_) => {
            EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
                .with_source_event_subject(LocalObjectReference::SourceSelf)
        }
        // Any other battlefield trigger is observed by its source permanent:
        // `this creature` names that owner (CR 109.2, 113.6). The event
        // subject is someone else, so `it` is not bound to the source.
        event if event.requires_permanent_face() => {
            EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
        }
        _ => EffectRecognitionContext::default(),
    };
    if event.observed_object_is_source() {
        context.source = Some(LocalObjectReference::SourceSelf);
        context = context.with_source_event_subject(LocalObjectReference::SourceSelf);
        context.observed_source_relationship = ObservedSourceRelationship::SameEnteringObject;
    }
    context
}
