use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterEffectNode, DamageEffectNode, DamageRecipientNode, LocalEffectNode, NumericExpression,
    ObjectCardType, ObjectKind, SelectorPredicate, TargetCardinality, TargetSelector,
    ZoneChangeAction, ZoneChangeDestination, ZoneChangeEffectNode, ZoneChangeRegenerationPolicy,
    recognize_equal_to_amount_expression, recognize_numeric_expression, recognize_player_reference,
    recognize_source_reference_with_typed_antecedent, recognize_target_list,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterState {
    Action,
    Target,
    Complete,
}

/// Recognizes one atomic counter effect whose recipient is a spell target.
/// Target syntax and predicate composition remain owned by `TargetList` and
/// `Selector`; this machine only commits the stack-spell domain.
pub(in crate::oracle::canonical) fn recognize_counter_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalEffectNode> {
    let mut state = CounterState::Action;
    let mut target = None;
    loop {
        state = match state {
            CounterState::Action => {
                cursor.enter("effect.counter.action");
                cursor.consume("counter")?;
                CounterState::Target
            }
            CounterState::Target => {
                cursor.enter("effect.counter.target");
                // `it` after a cast event (`Whenever an opponent casts a
                // spell, counter it`): the cast spell is the only object the
                // environment names; an object antecedent keeps priority and
                // is not a spell, so it is refused rather than reinterpreted.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
                    && cursor.binding_environment().is_some_and(|environment| {
                        environment.frame.event_spell && environment.object.is_none()
                    })
                {
                    cursor.enter("effect.counter.event-spell-anaphor");
                    cursor.consume("it")?;
                    cursor.enter("effect.counter.complete");
                    return Ok(LocalEffectNode::CounterSpell(CounterEffectNode {
                        target: super::TargetListNode { slots: Vec::new() },
                        event_spell: true,
                    }));
                }
                // `that spell`: the spell the triggering cast binds.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
                    && cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("spell"))
                {
                    cursor.enter("effect.counter.event-spell");
                    cursor.require(
                        cursor
                            .binding_environment()
                            .is_some_and(|environment| environment.frame.event_spell),
                        "a cast event that records the spell `that spell` names",
                    )?;
                    cursor.consume_phrase(&["that", "spell"])?;
                    cursor.enter("effect.counter.complete");
                    return Ok(LocalEffectNode::CounterSpell(CounterEffectNode {
                        target: super::TargetListNode { slots: Vec::new() },
                        event_spell: true,
                    }));
                }
                let targets = recognize_target_list(cursor)?;
                cursor.enter("effect.counter.target-domain");
                cursor.require(
                    targets.slots.len() == 1,
                    "exactly one target slot for a counter effect",
                )?;
                let slot = targets.slots.first().expect("validated one counter target");
                cursor.require(
                    slot.cardinality == TargetCardinality::EXACTLY_ONE,
                    "exactly one required spell target",
                )?;
                cursor.require(
                    stack_domain_selector_supported(&slot.selector),
                    "a spell or stack-item selector",
                )?;
                target = Some(targets);
                CounterState::Complete
            }
            CounterState::Complete => {
                cursor.enter("effect.counter.complete");
                return Ok(LocalEffectNode::CounterSpell(CounterEffectNode {
                    target: cursor.require_value(target, "a populated counter target")?,
                    event_spell: false,
                }));
            }
        };
    }
}

pub(in crate::oracle::canonical) fn stack_domain_selector_supported(
    selector: &TargetSelector,
) -> bool {
    match selector {
        TargetSelector::Union(_) => false,
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            spell_selector_supported(predicate)
        }
        TargetSelector::StackItem(_) => true,
        TargetSelector::Any | TargetSelector::Player(_) => false,
    }
}

/// A spell target (CR 115.1): the shared domain authority, not a local
/// whitelist of predicate atoms.
fn spell_selector_supported(predicate: &SelectorPredicate) -> bool {
    predicate.is_declarable_target_in(ObjectKind::Spell)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DamageState {
    Amount,
    Resource,
    Destination,
    Selector,
    Conjunction,
    Complete,
}

#[derive(Default)]
struct DamageMemory {
    amount: Option<NumericExpression>,
    recipients: Vec<DamageRecipientNode>,
}

/// `equal to EXPR` after `damage`, either before or after the recipient
/// (`deals damage equal to E to R` / `deals damage to R equal to E`). The
/// amount is the shared numeric expression; the clause subject of the
/// binding environment (the damage source) is the only antecedent a
/// possessive `its` may name inside this frame.
fn recognize_damage_equal_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("effect.damage.equal-amount");
    let subject = cursor
        .binding_environment()
        .and_then(|environment| environment.subject);
    match subject {
        Some(super::PermanentEffectReference::Source(reference)) => {
            cursor.consume_phrase(&["equal", "to"])?;
            recognize_equal_to_amount_expression(cursor, Some(reference))
        }
        // The shared primary expression owns `equal to` and resolves `its`
        // through the subject's declared target slot.
        Some(super::PermanentEffectReference::Target(binding)) => {
            super::recognize_numeric_primary_expression(cursor, Some(binding))
        }
        _ => {
            cursor.consume_phrase(&["equal", "to"])?;
            recognize_equal_to_amount_expression(cursor, None)
        }
    }
}

pub(in crate::oracle::canonical) fn recognize_damage_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LocalEffectNode> {
    recognize_damage_effect_with_source_antecedent(cursor, name_links, name_failure, None)
}

/// The enclosing typed ability supplies source identity; a pronoun never creates it.
pub(in crate::oracle::canonical) fn recognize_damage_effect_with_source_antecedent(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    source_antecedent: Option<super::PermanentEffectReference>,
) -> MachineResult<LocalEffectNode> {
    recognize_damage_effect_with_agreement(
        cursor,
        name_links,
        name_failure,
        source_antecedent,
        super::SubjectVerbAgreement::Finite,
    )
}

/// The same damage frame; the subject's frame chooses `deals` or, after a
/// causative `have`, the bare `deal` (`have it deal 4 damage to any target`).
pub(in crate::oracle::canonical) fn recognize_damage_effect_with_agreement(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    source_antecedent: Option<super::PermanentEffectReference>,
    agreement: super::SubjectVerbAgreement,
) -> MachineResult<LocalEffectNode> {
    cursor.enter("effect.damage.source");
    let source = recognize_source_reference_with_typed_antecedent(
        cursor,
        name_links,
        name_failure,
        source_antecedent,
    )?;
    recognize_damage_predicate(
        cursor,
        super::PermanentOperandNode::Reference(source),
        agreement,
    )
    .map(LocalEffectNode::DealDamage)
}

/// `deals AMOUNT damage to RECIPIENTS` after an already recognized source:
/// the shared clause with the subject's agreement (`deals`, or the bare
/// `deal` after a causative) and the effect policy of the damage frame. The
/// subject reader (ability source or permanent subject) owns the source.
pub(in crate::oracle::canonical) fn recognize_damage_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    source: super::PermanentOperandNode,
    agreement: super::SubjectVerbAgreement,
) -> MachineResult<DamageEffectNode> {
    cursor.enter("effect.damage.action");
    let frame = super::damage_effect_clause_frame(agreement);
    let subject = super::damage_source_subject(cursor, &source);
    let clause = super::with_clause_subject(cursor, subject, |cursor| {
        super::recognize_clause(cursor, &frame).into_result()
    })?
    .ok_or_else(|| cursor.fail("a damage clause (`deals` AMOUNT `damage to` RECIPIENT)"))?;
    let super::ClauseAction::DealDamage(super::DamageClauseArguments::Effect(arguments)) =
        clause.action
    else {
        return Err(cursor.fail("the effect arguments of the damage frame"));
    };
    Ok(DamageEffectNode {
        source,
        amount: arguments.amount,
        recipient: arguments.recipient,
    })
}

/// The effect arguments of a damage clause: amount, resource and recipients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DamageEffectArguments {
    pub amount: NumericExpression,
    pub recipient: DamageRecipientNode,
}

/// `AMOUNT damage to RECIPIENTS` / `damage equal to E to R` after the verb.
pub(in crate::oracle::canonical) fn recognize_damage_effect_arguments(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageEffectArguments> {
    recognize_damage_frame(cursor)
}

/// `... and AMOUNT damage to RECIPIENT`: a gapped damage predicate whose
/// subject and verb are the previous damage action's (coordination ellipsis).
/// The parent supplies that source; this child never invents one.
pub(in crate::oracle::canonical) fn recognize_gapped_damage_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    source: super::PermanentOperandNode,
) -> MachineResult<DamageEffectNode> {
    cursor.enter("effect.damage.gapped");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|raw| !raw.eq_ignore_ascii_case("damage"))
            && (1..4).any(|offset| {
                cursor
                    .peek_ahead(offset)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("damage"))
            }),
        "an amount followed by `damage`",
    )?;
    let subject = super::damage_source_subject(cursor, &source);
    let arguments = super::with_clause_subject(cursor, subject, recognize_damage_frame)?;
    Ok(DamageEffectNode {
        source,
        amount: arguments.amount,
        recipient: arguments.recipient,
    })
}

fn recognize_damage_frame(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageEffectArguments> {
    let mut state = DamageState::Amount;
    let mut memory = DamageMemory::default();
    loop {
        state = match state {
            DamageState::Amount => {
                cursor.enter("effect.damage.amount");
                // `deals damage equal to ...` defers the amount to its
                // `equal to` clause; otherwise the amount precedes `damage`.
                if !cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("damage"))
                {
                    // The shared arithmetic grammar: literals, X, event-bound
                    // `that much`, and their arithmetic (CR 120.3).
                    memory.amount = Some(recognize_numeric_expression(cursor, None)?);
                }
                DamageState::Resource
            }
            DamageState::Resource => {
                cursor.enter("effect.damage.resource");
                cursor.consume("damage")?;
                if memory.amount.is_none()
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("equal"))
                {
                    memory.amount = Some(recognize_damage_equal_amount(cursor)?);
                }
                DamageState::Destination
            }
            DamageState::Destination => {
                cursor.enter("effect.damage.destination");
                cursor.consume("to")?;
                DamageState::Selector
            }
            DamageState::Selector => {
                cursor.enter("effect.damage.selector");
                memory
                    .recipients
                    .push(recognize_damage_recipient_group(cursor)?);
                DamageState::Conjunction
            }
            DamageState::Conjunction => {
                // `and each player`: another recipient group of the same
                // damage, read transactionally. A group must end at a frame
                // boundary; otherwise `and` belongs to the enclosing sequence.
                let mut trial = cursor.clone();
                trial.enter("effect.damage.recipient-conjunction");
                if trial
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && trial.consume("and").is_ok()
                    && let Ok(group) = recognize_damage_recipient_group(&mut trial)
                    && damage_recipient_boundary(&trial)
                {
                    *cursor = trial;
                    memory.recipients.push(group);
                    DamageState::Conjunction
                } else {
                    DamageState::Complete
                }
            }
            DamageState::Complete => {
                if memory.amount.is_none() {
                    cursor.enter("effect.damage.postposed-amount");
                    cursor.require(
                        cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("equal")),
                        "`equal to` and the amount of damage",
                    )?;
                    memory.amount = Some(recognize_damage_equal_amount(cursor)?);
                }
                cursor.enter("effect.damage.complete");
                let recipient = match memory.recipients.len() {
                    0 => return Err(cursor.fail("a populated damage recipient")),
                    1 => memory.recipients.remove(0),
                    _ => {
                        cursor.require(
                            !memory
                                .recipients
                                .iter()
                                .any(|group| matches!(group, DamageRecipientNode::Targets(_))),
                            "target-free recipient groups in a damage recipient conjunction",
                        )?;
                        // `that creature and each other creature`: `other`
                        // names the sibling group's object, not the source.
                        let names_object = memory
                            .recipients
                            .iter()
                            .any(|group| matches!(group, DamageRecipientNode::BoundObject(_)));
                        let other_collection = memory.recipients.iter().any(|group| {
                            matches!(group, DamageRecipientNode::EachPermanent(collection)
                                if collection.exclude_source)
                        });
                        cursor.require(
                            !(names_object && other_collection),
                            "an `other` collection whose antecedent is the source, not a sibling recipient",
                        )?;
                        DamageRecipientNode::All(memory.recipients)
                    }
                };
                return Ok(DamageEffectArguments {
                    amount: cursor.require_value(memory.amount, "a populated damage amount")?,
                    recipient,
                });
            }
        };
    }
}

/// Tokens that may follow a complete recipient group inside a damage frame.
fn damage_recipient_boundary(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_none_or(|raw| {
        matches!(raw, "." | "," | ";")
            || raw.eq_ignore_ascii_case("and")
            || raw.eq_ignore_ascii_case("equal")
            || raw.eq_ignore_ascii_case("where")
    })
}

/// One recipient group: `you`, a player set (`each opponent`), a permanent
/// collection (`each [other] creature [you don't control]`) or targets.
fn recognize_damage_recipient_group(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageRecipientNode> {
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("you"))
    {
        return Ok(DamageRecipientNode::Player(recognize_player_reference(
            cursor, None,
        )?));
    }
    // A player or object the enclosing frame and program already bound.
    if super::bound_player_reference_starts(cursor) {
        let environment = super::installed_binding_environment(cursor);
        return super::recognize_bound_player_reference(cursor, &environment)
            .map(DamageRecipientNode::BoundPlayer);
    }
    if super::bound_object_reference_starts(cursor) {
        let environment = super::installed_binding_environment(cursor);
        return super::recognize_bound_object_reference(cursor, &environment)
            .map(DamageRecipientNode::BoundObject);
    }
    if cursor.peek().is_some_and(|token| {
        token.eq_ignore_ascii_case("each")
            && cursor.peek_ahead(1).is_some_and(|role| {
                role.eq_ignore_ascii_case("player") || role.eq_ignore_ascii_case("opponent")
            })
            || token.eq_ignore_ascii_case("your")
    }) {
        return Ok(DamageRecipientNode::PlayerSet(
            super::recognize_player_set_reference(cursor)?,
        ));
    }
    // `each of two target creatures`, `each of up to three targets`: each
    // declared target is dealt the full amount (CR 115.1, 120.1). The
    // distributive determiner owns no collection; the target list follows.
    let distributive_targets = cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("each"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("of"))
        && (cursor.peek_ahead(2).is_some_and(|token| {
            token.eq_ignore_ascii_case("target") || token.eq_ignore_ascii_case("up")
        }) || cursor
            .token_slice(cursor.position() + 2, cursor.position() + 3)
            .first()
            .is_some_and(|token| token.token_type == "quantity"));
    if distributive_targets {
        cursor.enter("effect.damage.distributive-targets");
        cursor.consume_phrase(&["each", "of"])?;
    } else if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("each"))
    {
        cursor.consume("each")?;
        // The shared collection grammar owns head coordination, a shared
        // trailing controller (`each creature and planeswalker your
        // opponents control`) and the `other` source exclusion.
        let objects = super::recognize_object_collection(cursor)?;
        let exclude_source = objects.exclude_source;
        cursor.enter("effect.damage.collection.validation");
        cursor.require(
            objects.denotes_battlefield_permanents(),
            "a collection explicitly anchored in the battlefield-permanent domain",
        )?;
        cursor.require(
            damageable_object_predicate(&objects.predicate),
            "a damageable permanent collection",
        )?;
        return Ok(DamageRecipientNode::EachPermanent(
            super::DamageCollectionNode {
                objects,
                exclude_source,
            },
        ));
    }
    let targets = recognize_target_list(cursor)?;
    cursor.enter("effect.damage.selector.validation");
    cursor.require(
        targets.slots.len() == 1,
        "exactly one target slot for a single-recipient damage effect",
    )?;
    cursor.require(
        targets
            .slots
            .first()
            .is_some_and(|slot| damage_target_supported(&slot.selector)),
        "a player, any target, or a damageable permanent selector",
    )?;
    Ok(DamageRecipientNode::Targets(targets))
}

pub(in crate::oracle::canonical) fn damage_target_supported(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Any | TargetSelector::Player(_) => true,
        TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(damage_target_supported)
        }
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            damageable_object_predicate(predicate)
        }
        TargetSelector::StackItem(_) => false,
    }
}

fn damageable_object_predicate(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::CardType(
            ObjectCardType::Creature | ObjectCardType::Planeswalker | ObjectCardType::Battle,
        ) => true,
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::CardType(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Color(_)
        | SelectorPredicate::ColorQuantity(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::Status(_)
        | SelectorPredicate::Designation(_)
        | SelectorPredicate::OwnedByAbilityController { .. }
        | SelectorPredicate::PermanentCard
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::NumericComparison { .. }
        | SelectorPredicate::ObjectKind(_)
        | SelectorPredicate::ChosenCreatureType
        | SelectorPredicate::ChosenColor
        | SelectorPredicate::Not(_) => false,
        SelectorPredicate::Intersection(operands) => {
            operands.iter().any(damageable_object_predicate)
        }
        SelectorPredicate::Union(operands) => {
            !operands.is_empty() && operands.iter().all(damageable_object_predicate)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneChangeState {
    Action,
    Target,
    DestinationRelation,
    DestinationObject,
    DestinationPlayerRole,
    DestinationZone,
    Complete,
}

/// Recognizes an atomic targeted battlefield zone change. Target-list grammar
/// remains shared; this machine owns the action, permanent-domain constraint,
/// and any explicit destination relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneChangeTargetSupport {
    Scalar,
    /// The parent lowers every target slot of the list and plural slots as
    /// one target collection (destroy, exile, or return to hand).
    TargetCollection,
}

/// Independent from target cardinality: a parent must have a proved lowerer
/// for every object domain it explicitly enables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneChangeDomains {
    #[default]
    Battlefield,
    SpellOrBattlefield,
}

pub(in crate::oracle::canonical) fn recognize_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalEffectNode> {
    recognize_zone_change_effect_with_support(cursor, ZoneChangeTargetSupport::Scalar)
}

/// A parent may opt into the existing plural destroy operand. All other
/// consumers and actions retain the scalar contract by default.
pub(in crate::oracle::canonical) fn recognize_zone_change_effect_with_support(
    cursor: &mut OracleTokenCursor<'_>,
    support: ZoneChangeTargetSupport,
) -> MachineResult<LocalEffectNode> {
    recognize_zone_change_effect_with_policy(cursor, support, ZoneChangeDomains::Battlefield)
}

/// Shared recognition with a caller-owned domain policy. Destroy and Exile
/// remain battlefield-only under every policy; only Return may use spells.
pub(in crate::oracle::canonical) fn recognize_zone_change_effect_with_policy(
    cursor: &mut OracleTokenCursor<'_>,
    support: ZoneChangeTargetSupport,
    domains: ZoneChangeDomains,
) -> MachineResult<LocalEffectNode> {
    let mut state = ZoneChangeState::Action;
    let mut action = None;
    let mut target = None;
    loop {
        state = match state {
            ZoneChangeState::Action => {
                cursor.enter("effect.zone-change.action");
                action = Some(match cursor.peek() {
                    Some(raw) if is_verb_form(&raw, VerbLemma::Exile, VerbForm::Base) => {
                        cursor.consume(
                            crate::oracle::canonical::automata::verb_entry(VerbLemma::Exile)
                                .surface(VerbForm::Base),
                        )?;
                        ZoneChangeAction::Exile
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("destroy") => {
                        cursor.consume("destroy")?;
                        ZoneChangeAction::Destroy
                    }
                    Some(raw) if is_verb_form(&raw, VerbLemma::Return, VerbForm::Base) => {
                        cursor.consume(
                            crate::oracle::canonical::automata::verb_entry(VerbLemma::Return)
                                .surface(VerbForm::Base),
                        )?;
                        ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand)
                    }
                    // `put` in its object-to-zone frame (the clause chooses
                    // it over the counters frame); the library position is
                    // read with the destination.
                    _ if crate::oracle::canonical::automata::lemma_at(
                        cursor,
                        VerbLemma::Put,
                        crate::oracle::canonical::automata::VerbAgreement::Base,
                    ) =>
                    {
                        crate::oracle::canonical::automata::consume_lemma(
                            cursor,
                            VerbLemma::Put,
                            crate::oracle::canonical::automata::VerbAgreement::Base,
                        )?;
                        ZoneChangeAction::Move(ZoneChangeDestination::OwnerLibrary(
                            super::LibraryPosition::Top,
                        ))
                    }
                    _ => return Err(cursor.fail("`exile`, `destroy`, `return`, or `put`")),
                });
                ZoneChangeState::Target
            }
            ZoneChangeState::Target => {
                cursor.enter("effect.zone-change.target");
                let targets = if support == ZoneChangeTargetSupport::TargetCollection {
                    // The shared verb operand: a reference, declared targets
                    // (with typed counts), card targets from an explicit
                    // origin, a library position or a quantified query.
                    let operand = recognize_verb_object_operand(
                        cursor,
                        VerbObjectOperandExpectation::OBJECT_TO_ZONE,
                    )?;
                    match operand {
                        VerbObjectOperandNode::Targets(targets) => targets,
                        operand => {
                            let action =
                                cursor.require_value(action, "a populated zone-change action")?;
                            return recognize_untargeted_zone_change(cursor, action, operand);
                        }
                    }
                } else {
                    recognize_target_list(cursor)?
                };
                cursor.enter("effect.zone-change.target.validation");
                // CR 601.2c: one instruction may name several target slots
                // (`target A and target B`) and each slot has its
                // own cardinality. Only a parent that lowers the resulting
                // target collection opts in; others keep one scalar object.
                cursor.require(
                    targets.slots.len() == 1
                        || support == ZoneChangeTargetSupport::TargetCollection,
                    "exactly one target slot for a single-object zone change",
                )?;
                cursor.require(
                    targets.slots.iter().all(|slot| {
                        slot.cardinality.maximum == 1
                            || support == ZoneChangeTargetSupport::TargetCollection
                    }),
                    "a scalar zone change or a target collection supported by this parent",
                )?;
                for slot in targets.slots.iter().skip(1) {
                    // A later slot's `other` relates it to the earlier target
                    // slots (CR 115.3): the shared slot lowering marks its
                    // candidates `otherThanEarlierTargets`, which the card
                    // composition resolves against the earlier declarations.
                    // The spell/permanent union stays a single-slot form.
                    cursor.require(
                        slot.selector.is_permanent_target(),
                        "a permanent target selector",
                    )?;
                }
                let selector = &targets.slots[0].selector;
                if domains == ZoneChangeDomains::SpellOrBattlefield
                    && targets.slots.len() == 1
                    && targets.slots[0].cardinality.maximum == 1
                    && matches!(
                        action,
                        Some(ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand))
                    )
                {
                    // Preserve the prior permanent-only language/IR fastpath.
                    // Only Return opts into the bounded spell/permanent union.
                    if !selector.is_permanent_target() {
                        super::validate_spell_or_permanent_target(selector).map_err(|error| {
                            cursor.enter(match error {
                                super::TargetDomainProjectionError::UnprovedNumericStackFilter => "effect.zone-change.target.numeric-stack-support",
                                super::TargetDomainProjectionError::TooManyAlternatives => "effect.zone-change.target.domain-complexity-support",
                                _ => "effect.zone-change.target.domain-support",
                            });
                            cursor.fail("a positively grounded spell/permanent return target without unproved numeric stack filters")
                        })?;
                    }
                } else {
                    cursor.require(
                        selector.is_permanent_target(),
                        "a permanent target selector",
                    )?;
                }
                target = Some(targets);
                match action {
                    Some(ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand)) => {
                        ZoneChangeState::DestinationRelation
                    }
                    Some(ZoneChangeAction::Move(ZoneChangeDestination::OwnerLibrary(_))) => {
                        cursor.enter("effect.zone-change.library.scalar");
                        cursor.require(
                            !target_collection_is_plural(target.as_ref()),
                            "one target permanent put into its owner's library",
                        )?;
                        cursor.enter("effect.zone-change.library.position");
                        let position = if cursor.consume_phrase(&["on", "top", "of"]).is_ok() {
                            super::LibraryPosition::Top
                        } else {
                            cursor.consume_phrase(&["on", "the", "bottom", "of"])?;
                            super::LibraryPosition::Bottom
                        };
                        action = Some(ZoneChangeAction::Move(ZoneChangeDestination::OwnerLibrary(
                            position,
                        )));
                        cursor.enter("effect.zone-change.library.owner");
                        cursor.consume("its")?;
                        let owner = cursor
                            .peek()
                            .filter(|raw| {
                                raw.eq_ignore_ascii_case("owner's")
                                    || raw.eq_ignore_ascii_case("owner\u{2019}s")
                            })
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| cursor.fail("`owner's`"))?;
                        cursor.consume(&owner)?;
                        cursor.consume("library")?;
                        ZoneChangeState::Complete
                    }
                    _ => ZoneChangeState::Complete,
                }
            }
            ZoneChangeState::DestinationRelation => {
                cursor.enter("effect.zone-change.destination.relation");
                cursor.consume("to")?;
                ZoneChangeState::DestinationObject
            }
            ZoneChangeState::DestinationObject => {
                cursor.enter("effect.zone-change.destination.object");
                // The possessive agrees with the target collection: one
                // scalar object is `its`, a collection is `their`.
                cursor.consume(if target_collection_is_plural(target.as_ref()) {
                    "their"
                } else {
                    "its"
                })?;
                ZoneChangeState::DestinationPlayerRole
            }
            ZoneChangeState::DestinationPlayerRole => {
                cursor.enter("effect.zone-change.destination.player-role");
                let plural = target_collection_is_plural(target.as_ref());
                let owner = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("owner's")
                            || raw.eq_ignore_ascii_case("owner’s")
                            || (plural
                                && (raw.eq_ignore_ascii_case("owners'")
                                    || raw.eq_ignore_ascii_case("owners’")))
                    })
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| cursor.fail("`owner's`"))?;
                cursor.consume(&owner)?;
                ZoneChangeState::DestinationZone
            }
            ZoneChangeState::DestinationZone => {
                cursor.enter("effect.zone-change.destination.zone");
                if target_collection_is_plural(target.as_ref())
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("hands"))
                {
                    cursor.consume("hands")?;
                } else {
                    cursor.consume("hand")?;
                }
                ZoneChangeState::Complete
            }
            ZoneChangeState::Complete => {
                cursor.enter("effect.zone-change.complete");
                return Ok(LocalEffectNode::ZoneChange(ZoneChangeEffectNode {
                    action: cursor.require_value(action, "a populated zone-change action")?,
                    target: cursor.require_value(target, "a populated zone-change target")?,
                    regeneration: ZoneChangeRegenerationPolicy::Allowed,
                    until: None,
                }));
            }
        };
    }
}

/// The zone action over a non-target operand of the shared verb operand:
/// a bound reference, a universal collection, a resolution choice, declared
/// card targets from an explicit origin, or a library position. Each pair
/// maps to the one typed node whose lowering already executes it; any other
/// pair fails explicitly on this state instead of being approximated.
fn recognize_untargeted_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
    action: ZoneChangeAction,
    operand: VerbObjectOperandNode,
) -> MachineResult<LocalEffectNode> {
    use super::ReferencedPermanentAction as Referenced;
    let plural = operand.is_plural();
    match (action, operand) {
        (ZoneChangeAction::Destroy, VerbObjectOperandNode::Reference(object)) => {
            cursor.enter("effect.zone-change.reference.destroy");
            let cannot_regenerate = super::recognize_scalar_regeneration_amendment(cursor)?;
            Ok(LocalEffectNode::ReferencedPermanentAction(
                super::ReferencedPermanentActionNode {
                    object,
                    cannot_regenerate,
                    action: Referenced::Destroy,
                },
            ))
        }
        (ZoneChangeAction::Exile, VerbObjectOperandNode::Reference(object)) => {
            cursor.enter("effect.zone-change.reference.exile");
            Ok(LocalEffectNode::ReferencedPermanentAction(
                super::ReferencedPermanentActionNode {
                    object,
                    cannot_regenerate: false,
                    action: Referenced::Exile,
                },
            ))
        }
        (
            ZoneChangeAction::Destroy,
            VerbObjectOperandNode::Collection {
                objects,
                exclude_source,
            },
        ) => {
            cursor.enter("effect.zone-change.collection.destroy");
            let cannot_regenerate = super::recognize_plural_regeneration_amendment(cursor)?;
            Ok(LocalEffectNode::DestroyCollection(
                super::DestroyPermanentCollectionEffectNode {
                    objects,
                    exclude_source,
                    cannot_regenerate,
                },
            ))
        }
        (
            ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand),
            VerbObjectOperandNode::Collection { objects, .. },
        ) => {
            cursor.enter("effect.zone-change.collection.return");
            recognize_owner_hand_destination(cursor, plural)?;
            Ok(LocalEffectNode::SelectedPermanents(
                super::SelectedPermanentsEffectNode {
                    action: super::SelectedPermanentAction::ReturnToOwnerHand,
                    operand: super::PermanentSelectionOperandNode {
                        bounds: None,
                        query: objects,
                    },
                },
            ))
        }
        (
            ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand),
            VerbObjectOperandNode::Selection(operand),
        ) => {
            cursor.enter("effect.zone-change.selection.return");
            recognize_owner_hand_destination(cursor, plural)?;
            Ok(LocalEffectNode::SelectedPermanents(
                super::SelectedPermanentsEffectNode {
                    action: super::SelectedPermanentAction::ReturnToOwnerHand,
                    operand,
                },
            ))
        }
        (
            ZoneChangeAction::Exile | ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand),
            VerbObjectOperandNode::CardTargets(target),
        ) => {
            cursor.enter("effect.zone-change.card-targets.destination");
            let verb = if action == ZoneChangeAction::Exile {
                "exile"
            } else {
                "return"
            };
            let destination = super::recognize_card_action_destination(cursor, verb)?;
            Ok(LocalEffectNode::CardAction(super::CardActionNode::Move {
                cards: super::CardOperandNode::Targets(target),
                destination,
            }))
        }
        (ZoneChangeAction::Exile, VerbObjectOperandNode::LibraryTop(top)) => {
            cursor.enter("effect.zone-change.library-position.exile");
            let destination = super::recognize_card_action_destination(cursor, "exile")?;
            Ok(LocalEffectNode::CardAction(super::CardActionNode::Move {
                cards: super::CardOperandNode::LibraryTop(top),
                destination,
            }))
        }
        (_, VerbObjectOperandNode::Targets(_)) => {
            Err(cursor.fail("declared targets read by the targeted zone change"))
        }
        _ => {
            cursor.enter("effect.zone-change.operand-capability");
            Err(cursor.fail(
                "an operand this zone action lowers: a bound reference to destroy or exile, a collection to destroy or return, a chosen permanent to return, card targets or a library position",
            ))
        }
    }
}

/// `to its|their owner's|owners' hand(s)` (CR 400.3): the possessive
/// agrees with the operand's number.
fn recognize_owner_hand_destination(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<()> {
    cursor.enter("effect.zone-change.destination.relation");
    cursor.consume("to")?;
    cursor.enter("effect.zone-change.destination.object");
    let pronoun = cursor
        .peek()
        .filter(|raw| {
            raw.eq_ignore_ascii_case("its") || (plural && raw.eq_ignore_ascii_case("their"))
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| cursor.fail("`its`, or `their` for a plural operand"))?;
    cursor.consume(&pronoun)?;
    cursor.enter("effect.zone-change.destination.player-role");
    let owner = cursor
        .peek()
        .filter(|raw| {
            ["owner's", "owner\u{2019}s"]
                .iter()
                .any(|form| raw.eq_ignore_ascii_case(form))
                || (plural
                    && ["owners'", "owners\u{2019}"]
                        .iter()
                        .any(|form| raw.eq_ignore_ascii_case(form)))
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| cursor.fail("`owner's`"))?;
    cursor.consume(&owner)?;
    cursor.enter("effect.zone-change.destination.zone");
    if plural
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("hands"))
    {
        cursor.consume("hands")
    } else {
        cursor.consume("hand")
    }
}

/// True when the parsed target list denotes more than one object: several
/// slots or one slot whose maximum exceeds one.
fn target_collection_is_plural(target: Option<&super::TargetListNode>) -> bool {
    target.is_some_and(|target| {
        target.slots.len() > 1 || target.slots.iter().any(|slot| slot.cardinality.maximum > 1)
    })
}

#[path = "verb_object_operand.rs"]
mod verb_object_operand;
pub(in crate::oracle::canonical) use verb_object_operand::*;

#[cfg(test)]
#[path = "zone_change_effect_tests.rs"]
mod zone_change_tests;

#[cfg(test)]
#[path = "zone_change_domain_policy_tests.rs"]
mod zone_change_domain_policy_tests;

#[cfg(test)]
#[path = "player_damage_recipient_tests.rs"]
mod player_damage_recipient_tests;
