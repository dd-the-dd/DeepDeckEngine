use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterEffectNode, DamageEffectNode, DamageRecipientNode, GainLifeEffectNode, LifeVerbForm,
    LocalEffectNode, LocalPlayerReference, LoseLifeEffectNode, NumericExpression, ObjectCardType,
    ObjectKind, PlayerTargetBinding, SelectorPredicate, TargetBinding, TargetCardinality,
    TargetPlayerLifeAction, TargetPlayerLifeEffectNode, TargetSelector, ZoneChangeAction,
    ZoneChangeDestination, ZoneChangeEffectNode, ZoneChangeRegenerationPolicy,
    recognize_equal_to_amount_expression, recognize_life_action, recognize_numeric_expression,
    recognize_player_reference, recognize_source_reference_with_typed_antecedent,
    recognize_target_list, recognize_target_slot_selector,
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

fn spell_selector_supported(predicate: &SelectorPredicate) -> bool {
    let mut spell_domains = 0;
    let valid = spell_selector_component(predicate, &mut spell_domains);
    valid && spell_domains == 1
}

fn spell_selector_component(predicate: &SelectorPredicate, spell_domains: &mut usize) -> bool {
    match predicate {
        SelectorPredicate::ObjectKind(ObjectKind::Spell) => {
            *spell_domains += 1;
            true
        }
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::CardType(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Color(_)
        | SelectorPredicate::ColorQuantity(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::NumericComparison { .. } => true,
        SelectorPredicate::Status(_)
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::ChosenCreatureType => false,
        SelectorPredicate::PermanentCard => true,
        SelectorPredicate::Not(operand) => {
            matches!(
                operand.as_ref(),
                SelectorPredicate::CardType(_)
                    | SelectorPredicate::Subtype(_)
                    | SelectorPredicate::CharacteristicBatch(_)
                    | SelectorPredicate::Color(_)
                    | SelectorPredicate::ColorQuantity(_)
                    | SelectorPredicate::HasKeyword(_)
                    | SelectorPredicate::Supertype(_)
                    | SelectorPredicate::NumericComparison { .. }
            )
        }
        SelectorPredicate::Intersection(operands) => {
            !operands.is_empty()
                && operands
                    .iter()
                    .all(|operand| spell_selector_component(operand, spell_domains))
        }
        SelectorPredicate::Union(operands) => {
            !operands.is_empty()
                && operands.iter().all(|operand| {
                    let mut branch_domains = 0;
                    spell_selector_component(operand, &mut branch_domains) && branch_domains == 0
                })
        }
        SelectorPredicate::ObjectKind(ObjectKind::Card | ObjectKind::Permanent) => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DamageState {
    Source,
    Action,
    Amount,
    Resource,
    Destination,
    Selector,
    Complete,
}

#[derive(Default)]
struct DamageMemory {
    source: Option<super::LocalObjectReference>,
    amount: Option<NumericExpression>,
    recipient: Option<DamageRecipientNode>,
}

/// `equal to EXPR` after `damage`, either before or after the recipient
/// (`deals damage equal to E to R` / `deals damage to R equal to E`). The
/// amount is the shared numeric expression; the damage source is the only
/// antecedent a possessive `its` may name inside this frame.
fn recognize_damage_equal_amount(
    cursor: &mut OracleTokenCursor<'_>,
    source: Option<super::LocalObjectReference>,
) -> MachineResult<NumericExpression> {
    cursor.enter("effect.damage.equal-amount");
    cursor.consume_phrase(&["equal", "to"])?;
    recognize_equal_to_amount_expression(cursor, source)
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
    let mut state = DamageState::Source;
    let mut memory = DamageMemory::default();
    loop {
        state = match state {
            DamageState::Source => {
                cursor.enter("effect.damage.source");
                memory.source = Some(recognize_source_reference_with_typed_antecedent(
                    cursor,
                    name_links,
                    name_failure,
                    source_antecedent,
                )?);
                DamageState::Action
            }
            DamageState::Action => {
                cursor.enter("effect.damage.action");
                cursor.consume("deals")?;
                DamageState::Amount
            }
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
                    memory.amount = Some(recognize_damage_equal_amount(cursor, memory.source)?);
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
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("you"))
                {
                    memory.recipient = Some(DamageRecipientNode::Player(
                        recognize_player_reference(cursor, None)?,
                    ));
                } else if cursor.peek().is_some_and(|token| {
                    token.eq_ignore_ascii_case("each")
                        && cursor.peek_ahead(1).is_some_and(|role| {
                            role.eq_ignore_ascii_case("player")
                                || role.eq_ignore_ascii_case("opponent")
                        })
                        || token.eq_ignore_ascii_case("your")
                }) {
                    memory.recipient = Some(DamageRecipientNode::PlayerSet(
                        super::recognize_player_set_reference(cursor)?,
                    ));
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("each"))
                {
                    cursor.consume("each")?;
                    let selector = recognize_target_slot_selector(cursor)?;
                    let TargetSelector::Object(predicate) = selector else {
                        return Err(cursor.fail("a permanent collection after `each`"));
                    };
                    cursor.enter("effect.damage.collection.validation");
                    cursor.require(
                        damageable_object_predicate(&predicate),
                        "a damageable permanent collection",
                    )?;
                    memory.recipient = Some(DamageRecipientNode::EachPermanent(predicate));
                } else {
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
                    memory.recipient = Some(DamageRecipientNode::Targets(targets));
                }
                DamageState::Complete
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
                    memory.amount = Some(recognize_damage_equal_amount(cursor, memory.source)?);
                }
                cursor.enter("effect.damage.complete");
                return Ok(LocalEffectNode::DealDamage(DamageEffectNode {
                    source: cursor.require_value(memory.source, "a populated damage source")?,
                    amount: cursor.require_value(memory.amount, "a populated damage amount")?,
                    recipient: cursor
                        .require_value(memory.recipient, "a populated damage recipient")?,
                }));
            }
        };
    }
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
        | SelectorPredicate::PermanentCard
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::NumericComparison { .. }
        | SelectorPredicate::ObjectKind(_)
        | SelectorPredicate::ChosenCreatureType
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
enum LifeState {
    Player,
    Action,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LifeAction {
    Gain,
    Lose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LifePlayer {
    Reference(LocalPlayerReference),
    Target {
        binding: PlayerTargetBinding,
        declares_target: bool,
    },
}

#[derive(Default)]
struct LifeMemory {
    player: Option<LifePlayer>,
    action: Option<LifeAction>,
    amount: Option<super::NumericExpression>,
}

pub(in crate::oracle::canonical) fn recognize_life_effect(
    cursor: &mut OracleTokenCursor<'_>,
    object_antecedent: Option<TargetBinding>,
) -> MachineResult<LocalEffectNode> {
    recognize_life_effect_with_player_target(cursor, object_antecedent, None, None)
}

pub(in crate::oracle::canonical) fn recognize_life_effect_with_player_target(
    cursor: &mut OracleTokenCursor<'_>,
    object_antecedent: Option<TargetBinding>,
    current_player_target: Option<PlayerTargetBinding>,
    player_antecedent: Option<PlayerTargetBinding>,
) -> MachineResult<LocalEffectNode> {
    let mut state = LifeState::Player;
    let mut memory = LifeMemory::default();
    loop {
        state = match state {
            LifeState::Player => {
                cursor.enter("effect.life.player");
                memory.player = Some(
                    if cursor.peek().is_some_and(|token| {
                        token.eq_ignore_ascii_case("gain") || token.eq_ignore_ascii_case("lose")
                    }) {
                        LifePlayer::Reference(LocalPlayerReference::Controller)
                    } else if cursor
                        .peek()
                        .is_some_and(|token| token.eq_ignore_ascii_case("target"))
                    {
                        cursor.enter("effect.life.player.target-marker");
                        cursor.consume("target")?;
                        cursor.enter("effect.life.player.target-domain");
                        cursor.consume("player")?;
                        LifePlayer::Target {
                            binding: current_player_target.ok_or_else(|| {
                                cursor
                                    .fail("a player-target binding supplied by the effect sequence")
                            })?,
                            declares_target: true,
                        }
                    } else if cursor
                        .peek()
                        .is_some_and(|token| token.eq_ignore_ascii_case("that"))
                        && cursor
                            .peek_ahead(1)
                            .is_some_and(|token| token.eq_ignore_ascii_case("player"))
                    {
                        cursor.enter("effect.life.player.anaphor");
                        cursor.consume_phrase(&["that", "player"])?;
                        LifePlayer::Target {
                            binding: player_antecedent.ok_or_else(|| {
                                cursor.fail("a prior player target binding for `that player`")
                            })?,
                            declares_target: false,
                        }
                    } else {
                        LifePlayer::Reference(recognize_player_reference(
                            cursor,
                            object_antecedent,
                        )?)
                    },
                );
                LifeState::Action
            }
            LifeState::Action => {
                let form = match memory.player {
                    Some(LifePlayer::Reference(LocalPlayerReference::Controller)) => {
                        LifeVerbForm::Base
                    }
                    Some(_) => LifeVerbForm::ThirdPersonSingular,
                    None => return Err(cursor.fail("a resolved life-total player")),
                };
                let action = recognize_life_action(cursor, form, object_antecedent)?;
                memory.action = Some(match action.action {
                    TargetPlayerLifeAction::Gain => LifeAction::Gain,
                    TargetPlayerLifeAction::Lose => LifeAction::Lose,
                });
                memory.amount = Some(action.amount);
                LifeState::Complete
            }
            LifeState::Complete => {
                cursor.enter("effect.life.complete");
                let player = cursor.require_value(memory.player, "a populated life player")?;
                let amount = cursor.require_value(memory.amount, "a populated life amount")?;
                let action = cursor.require_value(memory.action, "a populated life action")?;
                return match player {
                    LifePlayer::Reference(player) => match action {
                        LifeAction::Gain => Ok(LocalEffectNode::GainLife(GainLifeEffectNode {
                            player,
                            amount,
                        })),
                        LifeAction::Lose => Ok(LocalEffectNode::LoseLife(LoseLifeEffectNode {
                            player,
                            amount,
                        })),
                    },
                    LifePlayer::Target {
                        binding,
                        declares_target,
                    } => Ok(LocalEffectNode::TargetPlayerLife(
                        TargetPlayerLifeEffectNode {
                            player: binding,
                            declares_target,
                            action: match action {
                                LifeAction::Gain => TargetPlayerLifeAction::Gain,
                                LifeAction::Lose => TargetPlayerLifeAction::Lose,
                            },
                            amount,
                        },
                    )),
                };
            }
        };
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
    DestroyCollection,
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
                    Some(raw) if raw.eq_ignore_ascii_case("exile") => {
                        cursor.consume("exile")?;
                        ZoneChangeAction::Exile
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("destroy") => {
                        cursor.consume("destroy")?;
                        ZoneChangeAction::Destroy
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("return") => {
                        cursor.consume("return")?;
                        ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand)
                    }
                    _ => return Err(cursor.fail("`exile`, `destroy`, or `return`")),
                });
                ZoneChangeState::Target
            }
            ZoneChangeState::Target => {
                cursor.enter("effect.zone-change.target");
                let targets = recognize_target_list(cursor)?;
                cursor.enter("effect.zone-change.target.validation");
                cursor.require(
                    targets.slots.len() == 1,
                    "exactly one target slot for a single-object zone change",
                )?;
                cursor.require(
                    targets.slots.first().is_some_and(|slot| {
                        slot.cardinality.maximum == 1
                            || (support == ZoneChangeTargetSupport::DestroyCollection
                                && action == Some(ZoneChangeAction::Destroy))
                    }),
                    "a scalar zone change or a destroy collection supported by this parent",
                )?;
                let selector = &targets.slots[0].selector;
                if domains == ZoneChangeDomains::SpellOrBattlefield
                    && matches!(
                        action,
                        Some(ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand))
                    )
                {
                    // Preserve the prior permanent-only language/IR fastpath.
                    // Only Return opts into the bounded spell/permanent union.
                    if !permanent_target_supported(selector) {
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
                        permanent_target_supported(selector),
                        "a permanent target selector",
                    )?;
                }
                target = Some(targets);
                if matches!(
                    action,
                    Some(ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand))
                ) {
                    ZoneChangeState::DestinationRelation
                } else {
                    ZoneChangeState::Complete
                }
            }
            ZoneChangeState::DestinationRelation => {
                cursor.enter("effect.zone-change.destination.relation");
                cursor.consume("to")?;
                ZoneChangeState::DestinationObject
            }
            ZoneChangeState::DestinationObject => {
                cursor.enter("effect.zone-change.destination.object");
                cursor.consume("its")?;
                ZoneChangeState::DestinationPlayerRole
            }
            ZoneChangeState::DestinationPlayerRole => {
                cursor.enter("effect.zone-change.destination.player-role");
                let owner = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("owner's") || raw.eq_ignore_ascii_case("owner’s")
                    })
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| cursor.fail("`owner's`"))?;
                cursor.consume(&owner)?;
                ZoneChangeState::DestinationZone
            }
            ZoneChangeState::DestinationZone => {
                cursor.enter("effect.zone-change.destination.zone");
                cursor.consume("hand")?;
                ZoneChangeState::Complete
            }
            ZoneChangeState::Complete => {
                cursor.enter("effect.zone-change.complete");
                return Ok(LocalEffectNode::ZoneChange(ZoneChangeEffectNode {
                    action: cursor.require_value(action, "a populated zone-change action")?,
                    target: cursor.require_value(target, "a populated zone-change target")?,
                    regeneration: ZoneChangeRegenerationPolicy::Allowed,
                }));
            }
        };
    }
}

pub(in crate::oracle::canonical) fn permanent_target_supported(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Union(_) => false,
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            let mut permanent_domains = 0;
            permanent_predicate(predicate, &mut permanent_domains) && permanent_domains > 0
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

fn permanent_predicate(predicate: &SelectorPredicate, permanent_domains: &mut usize) -> bool {
    match predicate {
        SelectorPredicate::CardType(card_type) if card_type.is_permanent_type() => {
            *permanent_domains += 1;
            true
        }
        SelectorPredicate::IsCommander
        | SelectorPredicate::IsToken
        | SelectorPredicate::Color(_)
        | SelectorPredicate::ColorQuantity(_)
        | SelectorPredicate::Subtype(_)
        | SelectorPredicate::CharacteristicBatch(_)
        | SelectorPredicate::Supertype(_)
        | SelectorPredicate::ExactManaCost(_)
        | SelectorPredicate::Status(_)
        | SelectorPredicate::HasCounter(_)
        | SelectorPredicate::HasKeyword(_)
        | SelectorPredicate::NumericComparison { .. } => true,
        SelectorPredicate::ObjectKind(ObjectKind::Permanent) => {
            *permanent_domains += 1;
            true
        }
        // A target selector is declared before any resolution choice.
        SelectorPredicate::CardType(_)
        | SelectorPredicate::PermanentCard
        | SelectorPredicate::ChosenCreatureType
        | SelectorPredicate::ObjectKind(ObjectKind::Card | ObjectKind::Spell) => false,
        SelectorPredicate::Not(operand) => matches!(
            operand.as_ref(),
            SelectorPredicate::CardType(_)
                | SelectorPredicate::Color(_)
                | SelectorPredicate::ColorQuantity(_)
                | SelectorPredicate::Supertype(_)
                | SelectorPredicate::Subtype(_)
                | SelectorPredicate::CharacteristicBatch(_)
                | SelectorPredicate::HasKeyword(_)
                | SelectorPredicate::HasCounter(_)
                | SelectorPredicate::IsCommander
                | SelectorPredicate::IsToken
        ),
        SelectorPredicate::Intersection(operands) => {
            !operands.is_empty()
                && operands
                    .iter()
                    .all(|operand| permanent_predicate(operand, permanent_domains))
        }
        SelectorPredicate::Union(operands) => {
            !operands.is_empty()
                && operands.iter().all(|operand| {
                    let mut branch_domains = 0;
                    let valid = permanent_predicate(operand, &mut branch_domains);
                    *permanent_domains += branch_domains;
                    valid && branch_domains > 0
                })
        }
    }
}

#[cfg(test)]
#[path = "zone_change_effect_tests.rs"]
mod zone_change_tests;

#[cfg(test)]
#[path = "zone_change_domain_policy_tests.rs"]
mod zone_change_domain_policy_tests;

#[cfg(test)]
#[path = "player_damage_recipient_tests.rs"]
mod player_damage_recipient_tests;
