use super::super::*;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::oracle::canonical) struct LoweredEffectSequence {
    pub decisions: Vec<Value>,
    pub effects: Vec<Value>,
}

pub(in crate::oracle::canonical) enum EffectSequenceLoweringAttempt {
    Lowered(LoweredEffectSequence),
    UnsupportedSupportLimit(MachineFailure),
    FailedSupport(MachineFailure),
    FailedInvariant(MachineFailure),
}

/// Other parents retain their existing declaration-free contract. Card
/// programs may lower an X reference provisionally because the same spell
/// face can supply it only after composition. This context never declares X;
/// the pipeline must validate its actual producer before returning canonical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EffectSequenceLoweringContext {
    #[default]
    NoExternalX,
    ValidateXAfterCardComposition,
}

use crate::oracle::contextual_binding::{
    RESOLUTION_CREATURE_TYPE_DECISION, WHERE_X_BINDING_ID, references_resolution_creature_type,
};

/// Root of every id an effect of a program creates while it resolves (scry
/// and discard choices, sacrifice decisions, objects it binds). A container
/// that lowers several programs side by side (the modes of a modal spell)
/// rescopes the ids recognized by `is_effect_resolution_id`.
pub(in crate::oracle::canonical) fn effect_resolution_scope(effect_index: usize) -> String {
    format!("effect{effect_index}")
}

/// Whether `id` lies in some effect's resolution scope (`effect{N}` or
/// `effect{N}:...`).
pub(in crate::oracle::canonical) fn is_effect_resolution_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("effect") else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest[digits..].chars().next().is_none_or(|next| next == ':')
}

enum EffectSequenceLoweringFailure {
    UnsupportedSupportLimit(MachineFailure),
    FailedSupport(MachineFailure),
    FailedInvariant(MachineFailure),
}

impl From<MachineFailure> for EffectSequenceLoweringFailure {
    fn from(failure: MachineFailure) -> Self {
        Self::FailedInvariant(failure)
    }
}

fn lower_duration(duration: &EffectDurationNode) -> Option<Value> {
    Some(match duration {
        EffectDurationNode::UntilEndOfCurrentTurn => {
            json!({ "kind": "untilEndOfCurrentTurn" })
        }
        EffectDurationNode::UntilNextTurn {
            player: LocalPlayerReference::Controller,
        } => json!({
            "kind": "untilNextTurn",
            "player": controller(),
        }),
        EffectDurationNode::UntilNextTurn {
            player:
                LocalPlayerReference::ControllerOf(_)
                | LocalPlayerReference::ControllerOfQualified(_)
                | LocalPlayerReference::Bound(_),
        } => return None,
        // No continuous-effect primitive reads the end-of-next-turn window.
        EffectDurationNode::UntilEndOfNextTurn { .. } => return None,
        // Only the keyword grant has an indefinite primitive so far.
        EffectDurationNode::Indefinite => return None,
        EffectDurationNode::WhileSourceControlled => json!({ "kind": "whileSourceControlled" }),
        EffectDurationNode::WhileSourceTapped => json!({ "kind": "whileSourceTapped" }),
        EffectDurationNode::WhileSourceOnBattlefield => {
            json!({ "kind": "whileSourceOnBattlefield" })
        }
    })
}

/// The untap-step window of an untap restriction (CR 502.3, 611.2b): its
/// kind and, for one named player's next untap step, that player.
fn lower_untap_restriction_window(
    cursor: &OracleTokenCursor<'_>,
    restriction: &UntapRestrictionNode,
    declared_player_targets: &[(PlayerTargetBinding, String)],
) -> Result<Value, EffectSequenceLoweringFailure> {
    let kind = match restriction.duration {
        UntapRestrictionDuration::NextUntapStep => "nextUntapStep",
        UntapRestrictionDuration::WhileSourceControlled => "whileSourceControlled",
        UntapRestrictionDuration::WhileSourceTapped => "whileSourceTapped",
        UntapRestrictionDuration::WhileSourceOnBattlefield => "whileSourceOnBattlefield",
    };
    let mut window = json!({ "kind": kind });
    if let UntapStepOwner::Player(player) = &restriction.step_owner {
        let player = match player {
            LocalPlayerReference::Controller => Some(controller()),
            LocalPlayerReference::Bound(BoundPlayerReference::EventPlayer) => {
                Some(json!({ "kind": "triggeringPlayer" }))
            }
            LocalPlayerReference::Bound(BoundPlayerReference::Bound(binding)) => {
                declared_player_targets.iter().find_map(|(key, id)| {
                    (key.effect_index == binding.0).then(|| chosen_target(id))
                })
            }
            _ => None,
        };
        window["player"] = player.ok_or_else(|| {
            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("an untap step owner the runtime resolves (you, the event player or a declared player)"),
            )
        })?;
    }
    Ok(window)
}

/// The player a requirement names as the one to attack (CR 508.1d): the
/// controller, the event player, a player an earlier action declared, or a
/// player target this action declares. Any other relation is refused.
fn lower_requirement_defender(
    cursor: &OracleTokenCursor<'_>,
    defender: &PlayerEffectSubject,
    player_target_bindings: &[PlayerTargetBinding],
    declared_player_targets: &mut Vec<(PlayerTargetBinding, String)>,
    decisions: &mut Vec<Value>,
) -> Result<Value, EffectSequenceLoweringFailure> {
    match defender {
        PlayerEffectSubject::Controller => Ok(controller()),
        PlayerEffectSubject::EventPlayer => Ok(json!({ "kind": "triggeringPlayer" })),
        PlayerEffectSubject::Bound(binding) => declared_player_targets
            .iter()
            .find_map(|(key, id)| (key.effect_index == binding.0).then(|| chosen_target(id)))
            .ok_or_else(|| {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("a declared player as the requirement's defender"),
                )
            }),
        PlayerEffectSubject::DeclaredTarget { target, binding } => {
            let key = PlayerTargetBinding {
                effect_index: binding.0,
            };
            let [slot] = target.slots.as_slice() else {
                return Err(EffectSequenceLoweringFailure::FailedInvariant(
                    cursor.fail("one declared player target slot"),
                ));
            };
            let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                .ok_or_else(|| cursor.fail("a supported player target selector"))?;
            let ordinal = player_target_bindings
                .iter()
                .position(|value| *value == key)
                .ok_or_else(|| cursor.fail("a declared player-target binding"))?;
            let target_id = player_target_id_for_ordinal(ordinal, player_target_bindings.len());
            decisions.push(target_decision(&target_id, candidates, 1, 1));
            declared_player_targets.push((key, target_id.clone()));
            Ok(chosen_target(&target_id))
        }
        PlayerEffectSubject::ObjectRelation { .. } => {
            Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                "a requirement defender the runtime resolves (you, the event player or a declared player)",
            )))
        }
    }
}

pub(in crate::oracle::canonical) fn target_id_for_ordinal(
    base_id: &str,
    ordinal: usize,
    target_count: usize,
) -> String {
    if target_count == 1 {
        base_id.to_string()
    } else {
        format!("{base_id}{}", ordinal + 1)
    }
}

fn card_target_id_for_ordinal(ordinal: usize, target_count: usize) -> String {
    if target_count == 1 {
        "targetCard".to_string()
    } else {
        format!("targetCard{}", ordinal + 1)
    }
}

fn player_target_id_for_ordinal(ordinal: usize, target_count: usize) -> String {
    if target_count == 1 {
        "targetPlayer".to_string()
    } else {
        format!("targetPlayer{}", ordinal + 1)
    }
}

/// Lowers an ordered sequence of independent local actions. Target-producing
/// actions register their slot IDs once; later anaphoric actions reuse those
/// exact IDs through `TargetBinding` rather than creating another decision.
pub(in crate::oracle::canonical) fn lower_local_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
) -> MachineResult<LoweredEffectSequence> {
    lower_local_effect_sequence_attempt(
        cursor,
        sequence,
        EffectSequenceLoweringContext::NoExternalX,
    )
    .map_err(|failure| match failure {
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure)
        | EffectSequenceLoweringFailure::FailedSupport(failure)
        | EffectSequenceLoweringFailure::FailedInvariant(failure) => failure,
    })
}

pub(in crate::oracle::canonical) fn try_lower_local_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
) -> EffectSequenceLoweringAttempt {
    try_lower_local_effect_sequence_with_context(
        cursor,
        sequence,
        EffectSequenceLoweringContext::NoExternalX,
    )
}

pub(in crate::oracle::canonical) fn try_lower_local_effect_sequence_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    context: EffectSequenceLoweringContext,
) -> EffectSequenceLoweringAttempt {
    match lower_local_effect_sequence_attempt(cursor, sequence, context) {
        Ok(lowered) => EffectSequenceLoweringAttempt::Lowered(lowered),
        Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure)) => {
            EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure)
        }
        Err(EffectSequenceLoweringFailure::FailedSupport(failure)) => {
            EffectSequenceLoweringAttempt::FailedSupport(failure)
        }
        Err(EffectSequenceLoweringFailure::FailedInvariant(failure)) => {
            EffectSequenceLoweringAttempt::FailedInvariant(failure)
        }
    }
}

/// The same lowering over a caller-owned card-collection scope: an outcome
/// body reads the cards its producing instruction observed (CR 608.2c).
pub(in crate::oracle::canonical) fn try_lower_local_effect_sequence_with_collections(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    context: EffectSequenceLoweringContext,
    collections: &mut CardCollectionMemory,
) -> EffectSequenceLoweringAttempt {
    match lower_local_effect_sequence_with_collections(cursor, sequence, context, collections) {
        Ok(lowered) => EffectSequenceLoweringAttempt::Lowered(lowered),
        Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure)) => {
            EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure)
        }
        Err(EffectSequenceLoweringFailure::FailedSupport(failure)) => {
            EffectSequenceLoweringAttempt::FailedSupport(failure)
        }
        Err(EffectSequenceLoweringFailure::FailedInvariant(failure)) => {
            EffectSequenceLoweringAttempt::FailedInvariant(failure)
        }
    }
}

/// Whether the next action that reads or replaces the card-collection scope
/// reads it through a lexical reference (`that card`, `it`, `them`).
pub(super) fn reads_card_collection_reference(following: &[LocalEffectNode]) -> bool {
    following.iter().find_map(|effect| match effect {
        LocalEffectNode::CardAction(
            CardActionNode::Move { cards, .. }
            | CardActionNode::Reveal(cards)
            | CardActionNode::Copy { cards, .. }
            | CardActionNode::Permit(CardPermissionNode { cards, .. }),
        ) => Some(matches!(
            cards,
            CardOperandNode::Reference(_)
                | CardOperandNode::ObjectAntecedent { .. }
                // `from among those cards`: a selection reads the observed
                // collection it chooses from.
                | CardOperandNode::Selection(_)
        )),
        LocalEffectNode::CardAction(_) => Some(false),
        _ => None,
    }) == Some(true)
}

fn lower_local_effect_sequence_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    context: EffectSequenceLoweringContext,
) -> Result<LoweredEffectSequence, EffectSequenceLoweringFailure> {
    lower_local_effect_sequence_with_collections(
        cursor,
        sequence,
        context,
        &mut CardCollectionMemory::default(),
    )
}

fn lower_local_effect_sequence_with_collections(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    context: EffectSequenceLoweringContext,
    collections: &mut CardCollectionMemory,
) -> Result<LoweredEffectSequence, EffectSequenceLoweringFailure> {
    // Bindings an earlier instruction of the same resolution produced before
    // this sequence (an `If you do` producer): a delayed action may capture
    // them like a result of this sequence's own earlier actions.
    let inherited_bindings = collections.allocated_binding_ids();
    // An action another player performs is lowered as the ordinary action;
    // its performer is then put on every primitive it produced.
    let performed;
    let (sequence, performers) = if sequence
        .effects
        .iter()
        .any(|effect| matches!(effect, LocalEffectNode::Performed(_)))
    {
        let mut unwrapped = sequence.clone();
        let mut performers = Vec::with_capacity(unwrapped.effects.len());
        for effect in &mut unwrapped.effects {
            if let LocalEffectNode::Performed(node) = effect {
                performers.push(Some(node.performer.clone()));
                *effect = (*node.effect).clone();
            } else {
                performers.push(None);
            }
        }
        performed = unwrapped;
        (&performed, performers)
    } else {
        (sequence, Vec::new())
    };
    let mut pending_performer: Option<(usize, usize, PerformerNode)> = None;
    // The choices of optional deployments onto the battlefield, in order.
    let mut deployed_choices = Vec::<String>::new();
    let permanent_target_base_ids = sequence
        .effects
        .iter()
        .flat_map(|effect| match effect {
            LocalEffectNode::ModifyStats(_)
            | LocalEffectNode::BecomeTypes(_)
            | LocalEffectNode::PutCounters(_)
            | LocalEffectNode::MoveCounters(_)
            | LocalEffectNode::TapPermanents(_)
            | LocalEffectNode::PermanentKeywordAction(_)
            | LocalEffectNode::ControlChange(_)
            | LocalEffectNode::TransformPermanent(_)
            | LocalEffectNode::TemporaryKeywordGrant(_) => ordinary_action_targets(effect)
                .into_iter()
                .flat_map(|targets| targets.slots.iter())
                .filter_map(|slot| {
                    lower_target_selector_candidates(&slot.selector).map(|(id, _)| id)
                })
                .collect::<Vec<_>>(),
            LocalEffectNode::Attach(attach) => [&attach.attachment, &attach.recipient]
                .into_iter()
                .filter_map(|operand| match operand {
                    PermanentOperandNode::Targets(targets) => Some(targets),
                    _ => None,
                })
                .flat_map(|targets| targets.slots.iter())
                .filter_map(|slot| {
                    lower_target_selector_candidates(&slot.selector).map(|(id, _)| id)
                })
                .collect::<Vec<_>>(),
            LocalEffectNode::Choose(choice) => choice
                .permanent_targets()
                .into_iter()
                .flat_map(|targets| targets.slots.iter())
                .filter_map(|slot| {
                    lower_target_selector_candidates(&slot.selector).map(|(id, _)| id)
                })
                .collect::<Vec<_>>(),
            LocalEffectNode::DealDamage(damage) => {
                let mut ids = operand_target_base_ids(&damage.source);
                ids.extend(match damage.fixed_source() {
                    Some(_) => default_damage_target_id(damage),
                    None => match &damage.recipient {
                        DamageRecipientNode::Targets(targets) => {
                            Some(lower_single_target(targets).0)
                        }
                        _ => None,
                    },
                });
                ids
            }
            LocalEffectNode::Fight(fight) => {
                let mut ids = operand_target_base_ids(&fight.first);
                ids.extend(operand_target_base_ids(&fight.second));
                ids
            }
            LocalEffectNode::RuleChange(rule) => {
                let mut ids = rule
                    .objects()
                    .map(operand_target_base_ids)
                    .unwrap_or_default();
                if let RuleChangeNode::CombatRequirement(CombatRequirementNode {
                    direction:
                        CombatRequirementDirection::Block {
                            attacker: Some(attacker),
                        },
                    ..
                }) = rule
                {
                    ids.extend(operand_target_base_ids(attacker));
                }
                ids
            }
            LocalEffectNode::PreventDamage(prevent) => prevent
                .parties()
                .into_iter()
                .filter_map(|party| damage_party_targets(party))
                .filter_map(|targets| lower_target_slot_candidates(&targets.slots[0]))
                .map(|(id, _)| id)
                .collect(),
            LocalEffectNode::ZoneChange(zone_change) => zone_change
                .target
                .slots
                .iter()
                .filter_map(|slot| lower_zone_change_slot_candidates(zone_change, slot))
                .map(|(id, _, _, _)| id)
                .collect(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut decisions = Vec::new();
    let mut effects = Vec::new();
    let mut target_bindings = Vec::<(TargetBinding, String)>::new();
    let mut used_target_base_ids = Vec::<&str>::new();
    let card_target_count = sequence
        .effects
        .iter()
        .filter(|effect| matches!(effect, LocalEffectNode::TargetCardZoneChange(_)))
        .count();
    let mut card_target_ordinal = 0_usize;
    let local_player_target_binding = |effect_index: usize, effect: &LocalEffectNode| match effect {
        LocalEffectNode::Choose(ChooseInstructionNode {
            domain: ChoiceDomainNode::PlayerTarget(_),
            ..
        }) => Some(PlayerTargetBinding { effect_index }),
        LocalEffectNode::TargetPlayerLife(life) if life.declares_target => Some(life.player),
        LocalEffectNode::PlayerAction(PlayerActionNode {
            player: PlayerEffectSubject::DeclaredTarget { binding, .. },
            ..
        }) => Some(PlayerTargetBinding {
            effect_index: binding.0,
        }),
        LocalEffectNode::ControllerLibrarySearch(search)
            if matches!(search.origin.holder, SearchedPlayer::DeclaredTarget(_)) =>
        {
            Some(PlayerTargetBinding { effect_index })
        }
        // `attacks target opponent this turn if able`: the defender is a
        // player target this action declares.
        LocalEffectNode::RuleChange(RuleChangeNode::CombatRequirement(CombatRequirementNode {
            direction:
                CombatRequirementDirection::Attack {
                    defender: Some(PlayerEffectSubject::DeclaredTarget { binding, .. }),
                },
            ..
        })) => Some(PlayerTargetBinding {
            effect_index: binding.0,
        }),
        _ => None,
    };
    let player_target_bindings = sequence
        .effects
        .iter()
        .enumerate()
        .flat_map(|(effect_index, effect)| {
            // A declared performer (`target opponent VERBs`) is written
            // before its action's own targets.
            let performer = match performers.get(effect_index) {
                Some(Some(
                    PerformerNode::Player(PlayerEffectSubject::DeclaredTarget { binding, .. })
                    | PerformerNode::DeclaredPlayers { binding, .. },
                )) => Some(PlayerTargetBinding {
                    effect_index: binding.0,
                }),
                _ => None,
            };
            [performer, local_player_target_binding(effect_index, effect)]
        })
        .flatten()
        .fold(Vec::new(), |mut bindings, binding| {
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
            bindings
        });
    let mut declared_player_targets = Vec::<(PlayerTargetBinding, String)>::new();
    let mut card_target_bindings = Vec::<(TargetBinding, String)>::new();
    let mut conditional_scopes = ConditionalScopes::new(cursor, sequence)?;

    // JSON positions where each action's lowering begins, for scoping `where X`.
    let mut action_starts = Vec::with_capacity(sequence.effects.len() + 1);
    // A reference to the creature type chosen on resolution must follow its
    // choice in program order (CR 608.2d); it is never borrowed backwards.
    let mut creature_type_chosen = false;
    let mut unbound_creature_type_from = 0_usize;
    // The same order holds for a color chosen on resolution, and every
    // player chosen on resolution is lowered as the value its choice binds.
    let mut color_chosen = false;
    let mut resolution_players = Vec::<String>::new();
    // Permanents and numbers chosen on resolution, read through bindings;
    // `the rest` of a choice is acted on once every chooser has chosen.
    let mut chosen_object_bindings = Vec::<String>::new();
    let mut chosen_numbers = Vec::<String>::new();
    let mut pending_choice_remainder = Vec::<Value>::new();

    for (effect_index, effect) in sequence.effects.iter().enumerate() {
        program_iteration_lowering::bind_iteration_members(
            &sequence.iterations,
            effect_index,
            &mut target_bindings,
        );
        declare_query_possessor_targets(
            cursor,
            &mut effects,
            &mut decisions,
            &mut declared_player_targets,
        )?;
        apply_performer(
            cursor,
            &mut effects,
            pending_performer.take(),
            // `Exile target card ... . Its owner creates ...`: a declared
            // card target is an object a performer's relation names too.
            &[target_bindings.as_slice(), card_target_bindings.as_slice()].concat(),
            &declared_player_targets,
        )?;
        effects.append(&mut pending_choice_remainder);
        if let Some(Some(performer)) = performers.get(effect_index) {
            declare_performer_target(
                cursor,
                performer,
                &player_target_bindings,
                &mut decisions,
                &mut declared_player_targets,
            )?;
            pending_performer = Some((effects.len(), effect_index, performer.clone()));
        }
        if !creature_type_chosen
            && effects[unbound_creature_type_from..]
                .iter()
                .any(references_resolution_creature_type)
        {
            cursor.enter("lower.choose.creature-type.binding");
            return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
                "a `choose a creature type` action before the chosen-type reference",
            )));
        }
        if !color_chosen
            && effects[unbound_creature_type_from..]
                .iter()
                .any(super::resolution_choice::references_resolution_color)
        {
            cursor.enter("lower.choose.color.binding");
            return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
                "a `choose a color` action before the chosen-color reference",
            )));
        }
        unbound_creature_type_from = effects.len();
        conditional_scopes.at_boundary(
            cursor,
            sequence,
            effect_index,
            collections,
            &target_bindings,
            &card_target_bindings,
            PreviousActionResult {
                action_starts: &action_starts,
                effects: &mut effects,
            },
        )?;
        action_starts.push((effects.len(), decisions.len()));
        // A postposed definition supplies X to the actions of its sentence and
        // of the later sentences, so those actions lower X provisionally and
        // the binding replaces it below.
        let context = if sequence
            .where_x
            .iter()
            .any(|binding| binding.reaches(effect_index))
        {
            EffectSequenceLoweringContext::ValidateXAfterCardComposition
        } else {
            context
        };
        // These children introduce a different object/collection antecedent.
        // Until they expose their result through this common typed interface,
        // never resolve a later pronoun back to an unrelated earlier look.
        if matches!(
            effect,
            LocalEffectNode::ControllerLibrarySearch(_)
                | LocalEffectNode::OptionalLibrarySearch(_)
                | LocalEffectNode::DrawCards(_)
                | LocalEffectNode::OptionalDrawCards(_)
                | LocalEffectNode::MillCards(_)
                | LocalEffectNode::Scry(_)
                | LocalEffectNode::Surveil(_)
                | LocalEffectNode::CreateTokens(_)
                | LocalEffectNode::CreateNamedTokens(_)
                | LocalEffectNode::CreateExplicitTokens(_)
                | LocalEffectNode::TargetCardZoneChange(_)
                | LocalEffectNode::ZoneChange(_)
                | LocalEffectNode::OptionalCardDeployment(_)
                | LocalEffectNode::MoveCards(_)
                | LocalEffectNode::ZoneBatch(_)
        ) {
            collections.clear_antecedent();
        }
        if matches!(
            effect,
            LocalEffectNode::PlayerAction(PlayerActionNode {
                action: SingularPlayerAction::Draw(_)
                    | SingularPlayerAction::Mill(_)
                    | SingularPlayerAction::Scry(_)
                    | SingularPlayerAction::CreateTokens(_),
                ..
            })
        ) {
            collections.clear_antecedent();
        }
        // An operand chosen while the action resolves (CR 608.2d): its
        // choice runs right before the action, which reads the bound result.
        let chosen = super::chosen_operand::lower_action_chosen_operand(
            effect,
            effect_index,
            match context {
                EffectSequenceLoweringContext::NoExternalX => TargetlessDeclarationSupport::None,
                EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                    TargetlessDeclarationSupport::XValue
                }
            },
        );
        if !matches!(chosen, Ok(None)) {
            cursor.enter("lower.chosen-operand");
        }
        if let Some(chosen) = chosen.map_err(|expected| {
            EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
        })? {
            if !collections.claim_instruction_ids(&chosen.choice) {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("a program-unique decision for the operand chosen on resolution"),
                ));
            }
            effects.extend(chosen.choice);
            let key = resolution_choice_binding(effect_index);
            target_bindings.push((key, chosen.binding.clone()));
            target_bindings.push((
                super::permanent_subject::target_set_binding_key(key),
                chosen.binding.clone(),
            ));
            chosen_object_bindings.push(chosen.binding);
        }
        match effect {
            LocalEffectNode::PlayerAction(action) => {
                cursor.enter("lower.player-action.subject");
                let target_id = match &action.player {
                    PlayerEffectSubject::DeclaredTarget { binding, .. } => {
                        let key = PlayerTargetBinding {
                            effect_index: binding.0,
                        };
                        let ordinal = player_target_bindings
                            .iter()
                            .position(|value| *value == key)
                            .ok_or_else(|| cursor.fail("a declared player subject target slot"))?;
                        player_target_id_for_ordinal(ordinal, player_target_bindings.len())
                    }
                    PlayerEffectSubject::Bound(binding) => declared_player_targets
                        .iter()
                        .find_map(|(key, id)| (key.effect_index == binding.0).then_some(id.clone()))
                        .ok_or_else(|| {
                            cursor.fail("an earlier declared player subject target slot")
                        })?,
                    PlayerEffectSubject::Controller
                    | PlayerEffectSubject::EventPlayer
                    | PlayerEffectSubject::ObjectRelation { .. } => String::new(),
                };
                let antecedent = match &action.player {
                    PlayerEffectSubject::Bound(binding) => Some(BoundPlayerLowering {
                        binding: *binding,
                        target_id: &target_id,
                    }),
                    _ => None,
                };
                let lowered = if let PlayerEffectSubject::ObjectRelation { relation, object } =
                    &action.player
                {
                    // CR 108.3, 109.4: the related player of an object bound
                    // earlier in this program; never a new declaration.
                    cursor.enter("lower.player-action.object-relation");
                    let object = lower_bound_permanent_reference(
                        cursor,
                        *object,
                        &[target_bindings.as_slice(), card_target_bindings.as_slice()].concat(),
                    )?;
                    LoweredPlayerSubject {
                        player: json!({
                            "kind": match relation {
                                ObjectPlayerRelation::Controller => "controllerOf",
                                ObjectPlayerRelation::Owner => "ownerOf",
                            },
                            "object": object,
                        }),
                        decisions: Vec::new(),
                        declared_player: None,
                    }
                } else {
                    lower_player_effect_subject(&action.player, &target_id, antecedent).map_err(
                        |_| {
                            cursor.fail(
                                "a valid declared or previously bound singular player subject",
                            )
                        },
                    )?
                };
                decisions.extend(lowered.decisions);
                if let Some((binding, id)) = lowered.declared_player {
                    declared_player_targets.push((
                        PlayerTargetBinding {
                            effect_index: binding.0,
                        },
                        id,
                    ));
                }
                cursor.enter("lower.player-action.action");
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                let (kind, field, amount) = match &action.action {
                    SingularPlayerAction::Draw(count) => ("drawCards", "count", count),
                    SingularPlayerAction::Mill(count) => ("mill", "count", count),
                    // CR 122.1: counters on the acting player.
                    SingularPlayerAction::Counters(node) => {
                        ("addPlayerCounters", "count", &node.count)
                    }
                    // CR 119.5: the difference is gained or lost.
                    SingularPlayerAction::SetLifeTotal(total) => ("setLifeTotal", "amount", total),
                    SingularPlayerAction::Life(life) => (
                        match life.action {
                            TargetPlayerLifeAction::Gain => "gainLife",
                            TargetPlayerLifeAction::Lose => "loseLife",
                        },
                        "amount",
                        &life.amount,
                    ),
                    SingularPlayerAction::Discard(discard) => {
                        if let Some(count) = discard.quantity.count() {
                            validate_targetless_numeric_expression(count, declarations).map_err(
                                |error| {
                                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                        cursor.fail(error.expectation()),
                                    )
                                },
                            )?;
                        }
                        effects.extend(
                            lower_player_discard_action_for_player(
                                discard,
                                PlayerDiscardLoweringIds {
                                    target_player: "unusedPlayerActionTarget",
                                    random_selection: &format!("playerDiscard{effect_index}"),
                                },
                                lowered.player,
                            )
                            .map_err(|_| {
                                cursor.fail("a lowerable discard count for the player subject")
                            })?,
                        );
                        continue;
                    }
                    SingularPlayerAction::Proliferate(times) => {
                        let proliferation =
                            json!({"kind": "proliferate", "player": lowered.player});
                        if *times == NumericExpression::from(1_i64) {
                            effects.push(proliferation);
                        } else {
                            // `proliferate twice|X times`: separate actions.
                            cursor.enter("lower.player-action.proliferate.repetition");
                            let count = lower_sequence_bound_amount(
                                cursor,
                                times,
                                sequence,
                                &target_bindings,
                                context,
                            )?;
                            effects.push(json!({
                                "kind": "repeatEffects", "count": count,
                                "effects": [proliferation],
                            }));
                        }
                        continue;
                    }
                    // `[PLAYER] pay(s) RESOURCE` (CR 608.2c): `cost.rs`.
                    SingularPlayerAction::Pay(payment) => {
                        cursor.enter("lower.player-action.pay");
                        let amount = match payment.amount_expression() {
                            Some(expression) => Some(lower_sequence_bound_amount(
                                cursor,
                                expression,
                                sequence,
                                &target_bindings,
                                context,
                            )?),
                            None => None,
                        };
                        let payment = lower_payment_instruction(
                            payment,
                            lowered.player,
                            amount,
                            &super::resolution_object_choice::chosen_number_decision(effect_index),
                        )
                        .map_err(|expected| cursor.fail(expected))?;
                        if !collections.claim_instruction_ids(&payment) {
                            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail(
                                    "a program-unique decision for the chosen payment amount",
                                ),
                            ));
                        }
                        effects.extend(payment);
                        continue;
                    }
                    // CR 500.7: the acting player takes the next turn.
                    SingularPlayerAction::ExtraTurn => {
                        cursor.enter("lower.player-action.extra-turn");
                        effects.push(json!({"kind": "grantExtraTurn", "player": lowered.player}));
                        continue;
                    }
                    SingularPlayerAction::Designation(action) => {
                        cursor.enter("lower.player-action.designation");
                        effects.push(lower_player_designation_action(*action, lowered.player));
                        continue;
                    }
                    SingularPlayerAction::Skip(skip) => {
                        cursor.enter("lower.player-action.skip");
                        effects.push(lower_skip_turn_part(*skip, lowered.player));
                        continue;
                    }
                    SingularPlayerAction::Prohibition(prohibition) => {
                        cursor.enter("lower.player-action.prohibition");
                        effects.push(lower_player_prohibition(*prohibition, lowered.player));
                        continue;
                    }
                    SingularPlayerAction::Status(status) => {
                        cursor.enter("lower.player-action.status");
                        effects.extend(
                            lower_player_status_action(status, lowered.player)
                                .map_err(|expected| cursor.fail(expected))?,
                        );
                        continue;
                    }
                    SingularPlayerAction::Scry(count) => {
                        cursor.enter("lower.player-action.scry");
                        let amount = lower_sequence_bound_amount(
                            cursor,
                            count,
                            sequence,
                            &target_bindings,
                            context,
                        )?;
                        effects.extend(
                            lower_library_partition_effects_for_player(
                                LibraryPartitionAction::Scry,
                                count,
                                amount,
                                lowered.player,
                                Some(&effect_resolution_scope(effect_index)),
                            )
                            .map_err(EffectSequenceLoweringFailure::FailedSupport)?,
                        );
                        continue;
                    }
                    SingularPlayerAction::CreateTokens(body) => {
                        cursor.enter("lower.player-action.create-tokens");
                        let quantity = lower_sequence_bound_amount(
                            cursor,
                            &body.quantity,
                            sequence,
                            &target_bindings,
                            context,
                        )?;
                        let mut creation =
                            lower_token_creation_body(body, lowered.player, quantity)
                                .map_err(|expected| cursor.fail(expected))?;
                        // `that player creates a token ... The token is
                        // goaded`: bound only when the next action names
                        // the created tokens (`prior_object_set`).
                        if created_tokens_are_next_antecedent(sequence, effect_index)
                            && creation["kind"] == "createTokens"
                        {
                            creation["bind"] = json!(CREATED_TOKENS_BINDING);
                        }
                        effects.push(creation);
                        continue;
                    }
                    SingularPlayerAction::Hand(PlayerHandVerb::Discard) => {
                        cursor.enter("lower.player-action.discard-hand");
                        effects.push(json!({"kind": "discardHand", "player": lowered.player}));
                        continue;
                    }
                    SingularPlayerAction::Hand(PlayerHandVerb::Reveal) => {
                        cursor.enter("lower.player-action.reveal-hand");
                        let id = collections.observe_revealed_hand();
                        effects.push(json!({
                            "kind": "revealHand",
                            "player": lowered.player.clone(),
                            "duration": {"kind": "untilEndOfCurrentTurn"},
                        }));
                        effects.push(json!({
                            "kind": "bind",
                            "id": id,
                            "value": {
                                "kind": "cardsInZone",
                                "zone": {"kind": "hand", "player": lowered.player},
                            },
                        }));
                        continue;
                    }
                    SingularPlayerAction::ReferencedCards { verb, cards } => {
                        cursor.enter("lower.player-action.referenced-cards");
                        let cards = collections
                            .referenced_cards(cursor, *cards)
                            .map_err(EffectSequenceLoweringFailure::FailedSupport)?;
                        effects.push(match verb {
                            ReferencedCardVerb::Discard => json!({
                                "kind": "discardCards",
                                "player": lowered.player,
                                "cards": cards,
                            }),
                            // Each card goes to its owner's exile (CR 400.3).
                            ReferencedCardVerb::Exile => json!({
                                "kind": "moveCards",
                                "cards": cards,
                                "to": {
                                    "kind": "exile",
                                    "player": {"kind": "ownerOf", "object": {"kind": "candidate"}},
                                    "faceDown": false,
                                },
                            }),
                        });
                        continue;
                    }
                    SingularPlayerAction::Venture(venture) => {
                        let mut effect =
                            json!({"kind": "ventureDungeon", "player": lowered.player});
                        if let Some(dungeon) = &venture.dungeon {
                            effect["procedure"] = json!(format!("venture into {dungeon}"));
                        }
                        effects.push(effect);
                        continue;
                    }
                    SingularPlayerAction::Sacrifice(sacrifice) => {
                        effects.extend(
                            lower_player_sacrifice_effects(
                                sacrifice,
                                lowered.player,
                                &format!(
                                    "{}:playerSacrifice",
                                    effect_resolution_scope(effect_index)
                                ),
                                declarations,
                            )
                            .map_err(|error| {
                                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                    cursor.fail(error.expectation()),
                                )
                            })?,
                        );
                        continue;
                    }
                };
                let mut amount = lower_sequence_bound_amount(
                    cursor,
                    amount,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                // `mills half their library`: the acting player's zone.
                bind_acting_player(&mut amount, &lowered.player);
                let mut effect = json!({"kind": kind, "player": lowered.player});
                effect[field] = amount;
                if let SingularPlayerAction::Counters(node) = &action.action {
                    effect["counter"] = json!(node.counter.as_str());
                }
                // `draw N cards and reveal them`: the drawn cards are the
                // antecedent of the next card action, never otherwise bound.
                if let SingularPlayerAction::Draw(count) = &action.action
                    && matches!(action.player, PlayerEffectSubject::Controller)
                    && drawn_cards_are_next_antecedent(sequence, effect_index)
                {
                    let maximum = match count {
                        NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                        _ => None,
                    };
                    effect["bind"] = json!(collections.observe_drawn_cards(maximum));
                }
                // `mill N cards. Put a card from among them ...`: the milled
                // cards are the observed collection of the next card action.
                if let SingularPlayerAction::Mill(count) = &action.action
                    && milled_cards_are_next_antecedent(sequence, effect_index)
                {
                    let maximum = match count {
                        NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                        _ => None,
                    };
                    effect["bind"] = json!(collections.observe_milled_cards(maximum));
                }
                effects.push(effect);
            }
            LocalEffectNode::CardAction(action) => {
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                let first_decision = decisions.len();
                // A card pronoun may name a target an earlier action declared:
                // a permanent, or a card a targeted card move declared.
                collections.bind_object_references(
                    &[target_bindings.as_slice(), card_target_bindings.as_slice()].concat(),
                );
                let mut lowered = collections
                    .lower_action(cursor, action, declarations, &mut decisions)
                    .map_err(EffectSequenceLoweringFailure::FailedSupport)?;
                // `exile target creature card with mana value 3 or less from
                // your graveyard. Create a token that's a copy of it.`: the
                // one card this action declared is the program's card target
                // (`declared_card_target`), recorded under its first slot as
                // the historical targeted card move records it.
                if let Some((true, _)) = declared_card_target(effect)
                    && let [id] = decisions[first_decision..]
                        .iter()
                        .filter(|decision| {
                            decision["kind"] == "chooseTargets"
                                && decision["candidates"]["kind"] == "cards"
                        })
                        .filter_map(|decision| decision["id"].as_str())
                        .collect::<Vec<_>>()
                        .as_slice()
                {
                    card_target_bindings.push((
                        TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                        (*id).to_string(),
                    ));
                }
                // `exile all cards from your hand. ... return those cards`:
                // the cards a zone-wide exile moved are the observed exile
                // collection of the next card action (CR 400.7: the cards
                // they became), bound only when read.
                if let CardActionNode::Move {
                    cards: CardOperandNode::ZoneCollection(query),
                    destination,
                } = action
                    && destination.zone == CardZoneName::Exile
                    && (reads_card_collection_reference(&sequence.effects[effect_index + 1..])
                        || collections
                            .continuation_reads_cards_after(effect_index, sequence.effects.len()))
                    && let Some(movement) = lowered
                        .iter_mut()
                        .rev()
                        .find(|value| value["kind"] == "moveCards")
                    && movement.get("bind").is_none()
                {
                    // `exiles a card from their hand. You may play that
                    // card`: a chosen literal count keeps its number.
                    movement["bind"] = json!(match &query.quantity {
                        Some(CardSelectionQuantityNode::Exactly(NumericExpression::Atom(
                            NumericAtom::Integer(1),
                        ))) => collections.observe_exiled_object(),
                        Some(CardSelectionQuantityNode::Exactly(NumericExpression::Atom(
                            NumericAtom::Integer(count),
                        ))) => collections.observe_exiled_objects(Some(*count)),
                        _ => collections.observe_exiled_objects(None),
                    });
                }
                effects.extend(lowered);
                // `look at target player's hand`: the declared zone owner is
                // the player a later `that player` names (CR 115.1, 608.2c).
                if let [id] = decisions[first_decision..]
                    .iter()
                    .filter_map(|decision| decision["id"].as_str())
                    .filter(|id| id.starts_with("targetZonePlayer"))
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    let key = PlayerTargetBinding { effect_index };
                    if !declared_player_targets
                        .iter()
                        .any(|(declared, _)| *declared == key)
                    {
                        declared_player_targets.push((key, (*id).to_string()));
                    }
                }
            }
            // `exile all graveyards`: the shared card mover over a zone query.
            LocalEffectNode::ZoneBatch(batch) => {
                cursor.enter("lower.zone-batch");
                effects.push(lower_zone_batch_effect(batch));
            }
            // `exile this creature|Saga|NAME`: the source object moves to
            // exile wherever it is (a permanent, or the resolving spell,
            // CR 608.2n). A later card action reading `it` names the object
            // it became in exile (CR 400.7), recorded only when read.
            LocalEffectNode::MoveSource(movement)
                if movement.source == LocalObjectReference::SourceSelf
                    && movement.from == SourceZone::Battlefield
                    && movement.to == SourceZone::Exile
                    && movement.face_change == SourceFaceChange::Preserve =>
            {
                cursor.enter("lower.move-source.exile");
                let bind = (reads_card_collection_reference(&sequence.effects[effect_index + 1..])
                    || collections
                        .continuation_reads_cards_after(effect_index, sequence.effects.len()))
                .then(|| collections.observe_exiled_object());
                effects.push(lower_source_move_effect(
                    movement,
                    self_ref(),
                    bind.as_deref(),
                ));
            }
            // `shuffle NAME into its owner's library` (CR 701.24b): the move
            // into its owner's library, then the shuffle of the library that
            // received it. A resolving spell's own move is recorded for its
            // finishing step (CR 608.2n), which shuffles that library.
            LocalEffectNode::MoveSource(movement)
                if movement.source == LocalObjectReference::SourceSelf
                    && movement.to == SourceZone::Library =>
            {
                cursor.enter("lower.move-source.shuffle-into-library");
                let id = collections.allocate_selection_id("shuffledSource");
                effects.push(lower_source_move_effect(movement, self_ref(), Some(&id)));
                effects.push(lower_shuffle_libraries_holding(json!({
                    "kind": "boundObjects",
                    "binding": id,
                })));
            }
            // `return this card from your graveyard to the battlefield
            // attached to OBJECT`: the entry, then the attachment of the
            // object it became to the permanent the shared operand names
            // (CR 303.4f, 301.5c); an illegal attachment leaves it unattached.
            LocalEffectNode::MoveSource(movement)
                if movement.source == LocalObjectReference::SourceSelf
                    && movement.to == SourceZone::Battlefield
                    && movement.attached_to.is_some() =>
            {
                cursor.enter("lower.move-source.attach-on-entry");
                let recipient = movement
                    .attached_to
                    .as_ref()
                    .expect("an attachment recipient checked by the guard");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    recipient,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let [recipient] = <[Value; 1]>::try_from(objects)
                    .map_err(|_| cursor.fail("one permanent to attach the entering object to"))?;
                let id = collections.allocate_selection_id("attachedEntrant");
                effects.push(lower_source_move_effect(movement, self_ref(), Some(&id)));
                effects.push(json!({
                    "kind": "attachPermanent",
                    "attachment": {"kind": "boundObjects", "binding": id},
                    "to": recipient,
                }));
            }
            LocalEffectNode::OptionalSequence(body) => {
                let before = collections.clone();
                let mut lowered = lower_local_effect_sequence_with_collections(
                    cursor,
                    body,
                    context,
                    collections,
                )?;
                // Nested mandatory declarations need a common declaration-ID
                // allocator before they can safely cross this new scope.
                cursor.require(
                    lowered.decisions.is_empty(),
                    "an optional sentence without new target declarations",
                )?;
                effects.extend(collections.export_optional_outputs(
                    cursor,
                    &before,
                    &mut lowered.effects,
                )?);
                // `you may pay any amount of R`: no separate decision (`cost.rs`).
                if optional_payment_admits_zero(body) {
                    effects.extend(lowered.effects);
                } else {
                    effects.push(json!({"kind":"optionalEffects", "player":controller(), "effects":lowered.effects}));
                }
            }
            LocalEffectNode::CounterSpell(counter) if counter.event_spell => {
                // The spell the triggering cast record binds (CR 701.6a).
                effects.push(json!({
                    "kind": "counterSpell",
                    "spell": {"kind": "triggeringStackObject"},
                }));
            }
            LocalEffectNode::CounterSpell(counter) => {
                cursor.enter("lower.counter.target");
                let [slot] = counter.target.slots.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one stack-object target slot for the counter action"),
                    ));
                };
                let (target_id, candidates, action) = lower_counter_action(slot, false);
                if decisions
                    .iter()
                    .any(|decision| decision["id"].as_str() == Some(target_id))
                {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one stack-object target declaration per sequence"),
                    ));
                }
                decisions.push(target_decision(
                    target_id,
                    candidates,
                    i64::from(slot.cardinality.minimum),
                    i64::from(slot.cardinality.maximum),
                ));
                effects.push(action);
                if slot.cardinality == TargetCardinality::EXACTLY_ONE {
                    target_bindings.push((
                        TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                        target_id.to_string(),
                    ));
                }
            }
            // CR 500.8: phases added to this turn after a named phase.
            LocalEffectNode::AdditionalPhases(node) => {
                effects.push(lower_additional_phases(node));
            }
            // CR 608.2c: the rule drafter resolves the repeated process.
            LocalEffectNode::RepeatProcess(node) => {
                effects.push(program_iteration_lowering::lower_repeat_process(
                    cursor, node,
                )?);
            }
            LocalEffectNode::Retarget(node) => {
                let id = format!("retargetObject{effect_index}");
                let (decision, effect) = lower_retarget_action(cursor, node, &id)?;
                decisions.push(decision);
                effects.push(effect);
                target_bindings.push((
                    TargetBinding {
                        effect_index,
                        slot_index: 0,
                    },
                    id,
                ));
            }
            LocalEffectNode::TemporaryKeywordGrant(grant) => {
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &grant.objects,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let on_battlefield = operand_on_battlefield(&grant.objects, sequence);
                for object in objects {
                    effects.extend(
                        lower_keyword_grant_to_object(
                            &grant.keywords,
                            &grant.abilities,
                            &grant.duration,
                            &object,
                            on_battlefield,
                        )
                        .ok_or_else(|| {
                            cursor.fail("a supported keyword-grant duration and granted rule")
                        })?,
                    );
                }
            }
            LocalEffectNode::PutCounters(node)
                if node.direction == CounterChangeDirection::Remove =>
            {
                cursor.enter("lower.remove-counters.object");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &node.permanent,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                for object in objects {
                    let removal = lower_counter_removal(object, node).ok_or_else(|| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                            "a literal counter removal from the ability's own object (the runtime removes counters only from its source)",
                        ))
                    })?;
                    effects.push(removal);
                }
            }
            LocalEffectNode::MoveCounters(node) => {
                // CR 122.5: remove, recording whether it was removed, then
                // put the same counters on the destination only if so.
                cursor.enter("lower.move-counters.source");
                let sources = lower_ordinary_permanent_subject(
                    cursor,
                    &node.removal.permanent,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let [source] = sources.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one object the counters are moved from"),
                    ));
                };
                let mut removal = lower_counter_removal(source.clone(), &node.removal)
                    .ok_or_else(|| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                            "a literal counter removal from the ability's own object (the runtime removes counters only from its source)",
                        ))
                    })?;
                let binding = action_result_binding(effect_index);
                removal["bind"] = json!(binding);
                cursor.enter("lower.move-counters.destination");
                let amount = lower_sequence_bound_amount(
                    cursor,
                    &node.removal.count,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                let destinations = lower_ordinary_permanent_subject(
                    cursor,
                    &node.destination,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let [destination] = destinations.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one object the counters are moved onto"),
                    ));
                };
                effects.push(removal);
                effects.push(json!({
                    "kind": "conditional",
                    "condition": { "kind": "bindingNotEmpty", "binding": binding },
                    "then": [{
                        "kind": "putCounters",
                        "permanent": destination,
                        "counter": node.removal.counter.as_str(),
                        "count": amount,
                    }],
                    "else": [],
                }));
            }
            // CR 601.2d: the division is announced with the targets; each
            // declared target then receives its own share.
            LocalEffectNode::PutCounters(node)
                if node.direction == CounterChangeDirection::Divided =>
            {
                cursor.enter("lower.distribute-counters");
                let amount = lower_sequence_bound_amount(
                    cursor,
                    &node.count,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &node.permanent,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let (decision, effect) =
                    lower_divided_counters(&objects, node.counter.as_str(), amount).ok_or_else(
                        || cursor.fail("one declared target slot receiving the divided counters"),
                    )?;
                decisions.push(decision);
                effects.push(effect);
            }
            LocalEffectNode::PutCounters(node) => {
                // CR 701.10e: a doubled object's count is read from that same
                // object, so it is bound per lowered recipient below.
                let recipient_count = match &node.count {
                    NumericExpression::CountersOnRecipient { counter } => {
                        cursor.enter("lower.double-counters.recipient");
                        if matches!(node.permanent, PermanentOperandNode::Collection { .. }) {
                            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail(
                                    "one doubled permanent; a per-member counter count over a collection has no runtime binding",
                                ),
                            ));
                        }
                        Some(counter.as_str())
                    }
                    _ => None,
                };
                let amount = match recipient_count {
                    Some(_) => Value::Null,
                    None => lower_sequence_bound_amount(
                        cursor,
                        &node.count,
                        sequence,
                        &target_bindings,
                        context,
                    )?,
                };
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &node.permanent,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                // Listed or chosen kinds: every further placement's own
                // count, lowered once for all recipients.
                let further = match &node.kinds {
                    CounterKindsNode::Single => Vec::new(),
                    CounterKindsNode::Listed(placements)
                    | CounterKindsNode::ChosenAmong(placements) => {
                        cursor.enter("lower.put-counters.kinds");
                        if recipient_count.is_some() {
                            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail("one counter kind for a recipient-relative count"),
                            ));
                        }
                        placements
                            .iter()
                            .map(|placement| {
                                lower_sequence_bound_amount(
                                    cursor,
                                    &placement.count,
                                    sequence,
                                    &target_bindings,
                                    context,
                                )
                                .map(|count| (placement.counter.as_str(), count))
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    }
                };
                for object in objects {
                    let count = match recipient_count {
                        Some(counter) => json!({
                            "kind": "counterCountOf", "object": object.clone(), "counter": counter,
                        }),
                        None => amount.clone(),
                    };
                    match &node.kinds {
                        CounterKindsNode::Single => {
                            effects.push(json!({"kind":"putCounters", "permanent":object, "counter":node.counter.as_str(), "count":count}));
                        }
                        // CR 122.1: each placement on the same object, in
                        // the written order.
                        CounterKindsNode::Listed(_) => {
                            effects.push(json!({"kind":"putCounters", "permanent":object.clone(), "counter":node.counter.as_str(), "count":count}));
                            for (counter, count) in &further {
                                effects.push(json!({"kind":"putCounters", "permanent":object.clone(), "counter":counter, "count":count}));
                            }
                        }
                        // CR 608.2d: the controller chooses one placement
                        // while the action resolves.
                        CounterKindsNode::ChosenAmong(_) => {
                            let mut options =
                                vec![json!({"counter":node.counter.as_str(), "count":count})];
                            options.extend(
                                further.iter().map(
                                    |(counter, count)| json!({"counter":counter, "count":count}),
                                ),
                            );
                            effects.push(json!({"kind":"putChosenCounters", "permanent":object, "options":options}));
                        }
                    }
                }
            }
            LocalEffectNode::ControllerLibrarySearch(search) => match &search.origin.holder {
                SearchedPlayer::Searcher => {
                    let lowered = lower_controller_library_search_effects(cursor, search)?;
                    collections.register_collection_binding_ids(
                        &lowered,
                        &lower_collection_binding(search.binding),
                    );
                    effects.extend(lowered);
                    let later = &sequence.effects[effect_index + 1..];
                    observe_bound_library_search(cursor, collections, search, later)?;
                }
                // `Search target player's library ...`: the holder is one
                // declared player target of this action (CR 115.1).
                SearchedPlayer::DeclaredTarget(target) => {
                    cursor.enter("lower.library-search.declared-holder");
                    let id = declare_player_target(
                        cursor,
                        target,
                        PlayerTargetBinding { effect_index },
                        &player_target_bindings,
                        &mut decisions,
                        &mut declared_player_targets,
                    )?;
                    let lowered =
                        lower_controller_search_of_holder(cursor, search, chosen_target(&id))?;
                    collections.register_collection_binding_ids(
                        &lowered,
                        &lower_collection_binding(search.binding),
                    );
                    effects.extend(lowered);
                    let later = &sequence.effects[effect_index + 1..];
                    observe_bound_library_search(cursor, collections, search, later)?;
                }
                // `Search that player's library ...`, `Search its controller's
                // graveyard, hand, and library ...`: the holder is a player
                // this program already binds (CR 108.3, 109.4).
                SearchedPlayer::Bound(subject) => {
                    cursor.enter("lower.library-search.bound-holder");
                    let holder = lower_bound_player_subject(
                        cursor,
                        subject,
                        &target_bindings,
                        &declared_player_targets,
                    )?;
                    let lowered = lower_controller_search_of_holder(cursor, search, holder)?;
                    collections.register_collection_binding_ids(
                        &lowered,
                        &lower_collection_binding(search.binding),
                    );
                    effects.extend(lowered);
                    let later = &sequence.effects[effect_index + 1..];
                    observe_bound_library_search(cursor, collections, search, later)?;
                }
            },
            LocalEffectNode::OptionalLibrarySearch(search) => {
                cursor.enter("lower.optional-library-search.player");
                let Some(player) =
                    optional_search_player(search, &target_bindings, &declared_player_targets)
                else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                        "a searching player this program binds: the controller, a declared target's controller, or a bound player",
                    )));
                };
                let lowered = lower_library_search_effects(cursor, search, player.clone())?;
                collections.register_collection_binding_ids(
                    &lowered,
                    &lower_collection_binding(search.binding),
                );
                effects.push(json!({
                    "kind": "optionalEffects",
                    "player": player,
                    "effects": lowered,
                }));
            }
            LocalEffectNode::OptionalCardDeployment(deployment) => {
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                let lowered = lower_optional_card_deployment(
                    deployment,
                    declarations,
                    &collections.allocate_selection_id("optionalCardDeployment"),
                )
                .map_err(|error| {
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail(error.expectation()),
                    )
                })?;
                // The deployed card is the antecedent of a later reference
                // (`that creature gains haste`); the move binds it only when
                // such a reference reads it (resolved below).
                if let Some(choice) = lowered
                    .iter()
                    .find(|effect| effect["kind"] == "chooseCards")
                    .and_then(|effect| effect["id"].as_str())
                {
                    deployed_choices.push(choice.to_string());
                }
                effects.extend(lowered);
            }
            LocalEffectNode::OptionalDrawCards(draw) => {
                cursor.enter("lower.optional-draw.player");
                if let Err(failure) = cursor.require(
                    draw.player == LocalPlayerReference::Controller,
                    "a controller-owned optional draw; referenced controllers need an explicit snapshot",
                ) {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure));
                }
                cursor.enter("lower.optional-draw.count");
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                let maximum = lower_targetless_numeric_expression(&draw.maximum, declarations)
                    .map_err(|error| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            cursor.fail(error.expectation()),
                        )
                    })?;
                if draw.up_to {
                    let id = format!("optionalDrawCount{effect_index}");
                    effects.push(json!({
                        "kind": "chooseNumber", "id": id.clone(), "player": controller(),
                        "minimum": integer(0), "maximum": maximum,
                    }));
                    effects.push(json!({
                        "kind": "drawCards", "player": controller(), "count": decision_result(&id),
                    }));
                } else {
                    effects.push(json!({
                        "kind": "optionalEffects", "player": controller(),
                        "effects": [{ "kind": "drawCards", "player": controller(), "count": maximum }],
                    }));
                }
            }
            LocalEffectNode::BecomeTypes(change) => {
                cursor.enter("lower.become-types.object");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &change.objects,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                // `becomes a copy of OBJECT`: the copied object is one more
                // operand of the same shared permanent-subject lowering.
                let copy = match &change.copy_of {
                    Some(copied) => {
                        cursor.enter("lower.become-types.copy");
                        let copied = lower_ordinary_permanent_subject(
                            cursor,
                            copied,
                            effect_index,
                            false,
                            &permanent_target_base_ids,
                            &mut used_target_base_ids,
                            &mut decisions,
                            &mut target_bindings,
                        )?;
                        let [copied] = <[Value; 1]>::try_from(copied)
                            .map_err(|_| cursor.fail("one copied object"))?;
                        Some(copied)
                    }
                    None => None,
                };
                for object in objects {
                    effects.extend(
                        lower_become_types_to_object_in_scope(
                            change,
                            &object,
                            copy.as_ref(),
                            &format!("characteristicObjects{effect_index}"),
                            Some(TargetBinding {
                                effect_index,
                                slot_index: 0,
                            }),
                        )
                        .map_err(|expected| cursor.fail(expected))?,
                    );
                }
            }
            LocalEffectNode::ModifyStats(modifier) => {
                // `another target creature gets +N/+N` after a preceding target
                // declaration: the permanent subject marks the typed relation
                // the card composition resolves (`distinctFrom`).
                cursor.require(
                    [&modifier.delta.power, &modifier.delta.toughness].into_iter().all(|amount| {
                        !matches!(amount, NumericExpression::Atom(NumericAtom::Integer(value)) if i32::try_from(*value).is_err())
                    }),
                    "a power/toughness delta representable by runtime i32 quantities",
                )?;
                if modifier.duration == EffectDurationNode::Indefinite {
                    // CR 611.2a: no stated duration; the stat change has no
                    // indefinite primitive yet.
                    cursor.enter("lower.modify-stats.indefinite-duration");
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("a stated duration for the power/toughness change"),
                    ));
                }
                let duration = lower_duration(&modifier.duration)
                    .ok_or_else(|| cursor.fail("a duration independent of a target binding"))?;
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &modifier.targets,
                    effect_index,
                    modifier.exclude_source_from_first_target,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let power = lower_sequence_bound_amount(
                    cursor,
                    &modifier.delta.power,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                let toughness = lower_sequence_bound_amount(
                    cursor,
                    &modifier.delta.toughness,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                for object in objects {
                    effects.push(json!({
                        "kind": "modifyPowerToughness",
                        "object": object.clone(),
                        "power": power.clone(),
                        "toughness": toughness.clone(),
                        "duration": duration.clone(),
                    }));
                    effects.extend(
                        lower_keyword_grant_to_object(
                            &modifier.keywords,
                            &modifier.abilities,
                            &modifier.duration,
                            &object,
                            operand_on_battlefield(&modifier.targets, sequence),
                        )
                        .ok_or_else(|| {
                            cursor.fail("a supported keyword-grant duration and granted rule")
                        })?,
                    );
                }
            }
            LocalEffectNode::UntapRestriction(restriction) => {
                cursor.enter("lower.untap-restriction.object");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &restriction.objects,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                // CR 502.3: the permanent stays tapped during its
                // controller's untap step for the stated window.
                let duration =
                    lower_untap_restriction_window(cursor, restriction, &declared_player_targets)?;
                for object in objects {
                    // A plural operand (`those creatures`, `up to two target
                    // creatures`, `all nonblue creatures`) restricts each of
                    // its permanents.
                    if matches!(
                        object["kind"].as_str(),
                        Some("chosenTargets" | "eachPermanent")
                    ) {
                        let id = format!("effect{effect_index}:untapRestricted");
                        effects.push(json!({
                            "kind": "forEachObject",
                            "objects": object,
                            "bind": id.clone(),
                            "effects": [{
                                "kind": "installUntapRestriction",
                                "permanent": {"kind": "boundValue", "id": id},
                                "duration": duration.clone(),
                            }],
                        }));
                        continue;
                    }
                    effects.push(json!({
                        "kind": "installUntapRestriction",
                        "permanent": object,
                        "duration": duration.clone(),
                    }));
                }
            }
            // CR 701.28a: each operand permanent transforms; the source keeps
            // the IR of the targetless lowering.
            LocalEffectNode::TransformPermanent(transform) => {
                cursor.enter("lower.transform.object");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &transform.permanent,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                for object in objects {
                    effects.push(json!({"kind": "transformPermanent", "object": object}));
                }
            }
            LocalEffectNode::RuleChange(rule) => {
                cursor.enter("lower.rule-change.object");
                let objects = match rule.objects() {
                    Some(operand) => lower_ordinary_permanent_subject(
                        cursor,
                        operand,
                        effect_index,
                        false,
                        &permanent_target_base_ids,
                        &mut used_target_base_ids,
                        &mut decisions,
                        &mut target_bindings,
                    )?,
                    None => Vec::new(),
                };
                let lowered = match rule {
                    RuleChangeNode::CombatRequirement(node) => {
                        let bound = target_bindings.len();
                        let attacker = match &node.direction {
                            CombatRequirementDirection::Block {
                                attacker: Some(attacker),
                            } => {
                                cursor.enter("lower.rule-change.requirement.attacker");
                                let lowered = lower_ordinary_permanent_subject(
                                    cursor,
                                    attacker,
                                    effect_index,
                                    false,
                                    &permanent_target_base_ids,
                                    &mut used_target_base_ids,
                                    &mut decisions,
                                    &mut target_bindings,
                                )?;
                                // As for a fight's second fighter, a declared
                                // attacker is never the action's antecedent
                                // when its creatures declare their own.
                                if matches!(node.creatures, PermanentOperandNode::Targets(_)) {
                                    target_bindings.truncate(bound);
                                }
                                let [attacker] = lowered.as_slice() else {
                                    return Err(EffectSequenceLoweringFailure::FailedSupport(
                                        cursor.fail("exactly one attacker the requirement names"),
                                    ));
                                };
                                Some(attacker.clone())
                            }
                            _ => None,
                        };
                        let defender = match &node.direction {
                            CombatRequirementDirection::Attack {
                                defender: Some(defender),
                            } => {
                                cursor.enter("lower.rule-change.requirement.defender");
                                Some(lower_requirement_defender(
                                    cursor,
                                    defender,
                                    &player_target_bindings,
                                    &mut declared_player_targets,
                                    &mut decisions,
                                )?)
                            }
                            _ => None,
                        };
                        cursor.enter("lower.rule-change.requirement.window");
                        let window = match &node.window {
                            CombatRequirementWindow::ThisCombat => json!({ "kind": "thisCombat" }),
                            CombatRequirementWindow::Duration(
                                EffectDurationNode::UntilEndOfCurrentTurn,
                            ) => json!({ "kind": "untilEndOfCurrentTurn" }),
                            CombatRequirementWindow::Duration(_) => {
                                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                    cursor.fail(
                                        "a requirement window the runtime ends (`this turn`, `this combat`)",
                                    ),
                                ));
                            }
                        };
                        objects
                            .into_iter()
                            .map(|creatures| {
                                let mut effect = json!({
                                    "kind": "installCombatRequirement",
                                    "requirement": match node.direction {
                                        CombatRequirementDirection::Attack { .. } => "attack",
                                        CombatRequirementDirection::Block { .. } => "block",
                                    },
                                    "creatures": creatures,
                                    "window": window.clone(),
                                });
                                if let Some(attacker) = &attacker {
                                    effect["attacker"] = attacker.clone();
                                }
                                if let Some(defender) = &defender {
                                    effect["defender"] = defender.clone();
                                }
                                effect
                            })
                            .collect::<Vec<_>>()
                    }
                    RuleChangeNode::ActivationProhibition { duration, .. } => {
                        cursor.enter("lower.rule-change.activation-prohibition");
                        let duration = lower_duration(duration)
                            .filter(|duration| {
                                matches!(
                                    duration["kind"].as_str(),
                                    Some("untilEndOfCurrentTurn" | "untilNextTurn")
                                )
                            })
                            .ok_or_else(|| {
                                cursor.fail("an activation-prohibition duration the runtime ends")
                            })?;
                        let mut lowered = Vec::new();
                        for object in objects {
                            if matches!(
                                object["kind"].as_str(),
                                Some("chosenTargets" | "eachPermanent")
                            ) {
                                let id = format!("effect{effect_index}:activationProhibited");
                                lowered.push(json!({
                                    "kind": "forEachObject",
                                    "objects": object,
                                    "bind": id.clone(),
                                    "effects": [{
                                        "kind": "prohibitPermanentActivations",
                                        "object": {"kind": "boundValue", "id": id},
                                        "duration": duration.clone(),
                                    }],
                                }));
                                continue;
                            }
                            lowered.push(json!({
                                "kind": "prohibitPermanentActivations",
                                "object": object,
                                "duration": duration.clone(),
                            }));
                        }
                        lowered
                    }
                    RuleChangeNode::LifeTotalLock { duration } => {
                        cursor.enter("lower.rule-change.life-total-lock");
                        let duration = lower_duration(duration)
                            .filter(|duration| {
                                matches!(
                                    duration["kind"].as_str(),
                                    Some("untilEndOfCurrentTurn" | "untilNextTurn")
                                )
                            })
                            .ok_or_else(|| {
                                cursor.fail("a life-total lock duration the runtime ends")
                            })?;
                        vec![json!({
                            "kind": "lockLifeTotal",
                            "player": controller(),
                            "duration": duration,
                        })]
                    }
                };
                effects.extend(lowered);
            }
            LocalEffectNode::TapPermanents(tap) => {
                cursor.enter("lower.tap-permanents.object");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &tap.targets,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                for object in objects {
                    effects.push(lower_tap_state_change(tap.untap, object));
                }
            }
            LocalEffectNode::ControlChange(change) => {
                cursor.enter("lower.control-change.object");
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &change.objects,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                match &change.change {
                    ControlChange::Gain { duration } => {
                        cursor.enter("lower.control-change.duration");
                        // The runtime control layer expires only at cleanup of
                        // the current turn; any other window stays explicit.
                        let duration = match duration {
                            None => None,
                            Some(
                                duration @ (EffectDurationNode::UntilEndOfCurrentTurn
                                | EffectDurationNode::WhileSourceControlled
                                | EffectDurationNode::WhileSourceTapped
                                | EffectDurationNode::WhileSourceOnBattlefield),
                            ) => lower_duration(duration),
                            Some(_) => {
                                return Err(
                                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                        cursor.fail(
                                            "an indefinite, until-end-of-turn or source-state control duration",
                                        ),
                                    ),
                                );
                            }
                        };
                        for object in objects {
                            let mut effect = json!({
                                "kind": "gainControlPermanent",
                                "permanent": object,
                                "controller": controller(),
                            });
                            if let Some(duration) = &duration {
                                effect["duration"] = duration.clone();
                            }
                            effects.push(effect);
                        }
                    }
                    ControlChange::Exchange => {
                        cursor.enter("lower.control-change.exchange-pair");
                        let [objects] = objects.as_slice() else {
                            return Err(EffectSequenceLoweringFailure::FailedInvariant(
                                cursor.fail("one declared pair of permanents"),
                            ));
                        };
                        cursor.require(
                            objects["kind"] == "chosenTargets",
                            "one declared pair of permanents",
                        )?;
                        effects.push(json!({"kind": "exchangeControl", "objects": objects}));
                    }
                }
            }
            LocalEffectNode::CopyStackObject(copy) if copy.event_spell || copy.resolving_spell => {
                // The spell the triggering cast record binds, or the
                // resolving spell itself (CR 707.10); `twice` is two ordinary
                // copies of it, `for each ...` a count read at resolution.
                let except = lower_stack_copy_exceptions(cursor, &copy.exceptions)
                    .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
                cursor.enter("lower.copy-stack-object.actions");
                let actions = lower_stack_copy_actions(
                    copy,
                    json!({"kind": "triggeringStackObject"}),
                    controller(),
                    except.as_ref(),
                )
                .map_err(|expected| {
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
                })?;
                effects.extend(actions);
            }
            LocalEffectNode::CopyStackObject(copy) => {
                cursor.enter("lower.copy-stack-object.target");
                let [slot] = copy.target.slots.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one stack-object target slot for the copy action"),
                    ));
                };
                // The stack-object target slot (id and candidates) is the one
                // the counter action declares; only the action differs.
                let (target_id, mut candidates, _) = lower_counter_action(slot, false);
                if let Some(scope) = copy.controller {
                    candidates["controller"] = lower_object_controller_scope(scope);
                }
                if decisions
                    .iter()
                    .any(|decision| decision["id"].as_str() == Some(target_id))
                {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one stack-object target declaration per sequence"),
                    ));
                }
                decisions.push(target_decision(
                    target_id,
                    candidates,
                    i64::from(slot.cardinality.minimum),
                    i64::from(slot.cardinality.maximum),
                ));
                let except = lower_stack_copy_exceptions(cursor, &copy.exceptions)
                    .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
                cursor.enter("lower.copy-stack-object.actions");
                let actions = lower_stack_copy_actions(
                    copy,
                    chosen_target(target_id),
                    controller(),
                    except.as_ref(),
                )
                .map_err(|expected| {
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
                })?;
                effects.extend(actions);
                // The singular copied stack object is the antecedent a later
                // condition reads (`copy target spell. If it's ...`).
                if slot.cardinality == TargetCardinality::EXACTLY_ONE {
                    target_bindings.push((
                        TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                        target_id.to_string(),
                    ));
                }
            }
            LocalEffectNode::PermanentKeywordAction(keyword_action) => {
                let quantity = match &keyword_action.quantity {
                    Some(quantity) => Some(lower_sequence_bound_amount(
                        cursor,
                        quantity,
                        sequence,
                        &target_bindings,
                        context,
                    )?),
                    None => None,
                };
                cursor.enter(match keyword_action.action {
                    PermanentKeywordAction::Regenerate => "lower.regenerate.object",
                    PermanentKeywordAction::Goad => "lower.goad.object",
                    _ => "lower.keyword-action.object",
                });
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    &keyword_action.targets,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                for (ordinal, object) in objects.into_iter().enumerate() {
                    let scope = format!(
                        "effect{effect_index}:{}{ordinal}",
                        keyword_action.action.name()
                    );
                    effects.extend(
                        lower_permanent_keyword_action(
                            keyword_action.action,
                            object,
                            quantity.clone(),
                            &scope,
                        )
                        .map_err(|expected| {
                            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail(&expected),
                            )
                        })?,
                    );
                }
            }
            LocalEffectNode::ControllerKeywordAction(keyword_action) => {
                let quantity = match &keyword_action.quantity {
                    Some(quantity) => Some(lower_sequence_bound_amount(
                        cursor,
                        quantity,
                        sequence,
                        &target_bindings,
                        context,
                    )?),
                    None => None,
                };
                let times = match &keyword_action.times {
                    Some(times) => Some(lower_sequence_bound_amount(
                        cursor,
                        times,
                        sequence,
                        &target_bindings,
                        context,
                    )?),
                    None => None,
                };
                cursor.enter("lower.keyword-action.controller");
                effects.extend(repeat_keyword_action(
                    lower_controller_keyword_action(
                        keyword_action.action,
                        quantity,
                        &effect_resolution_scope(effect_index),
                    )
                    .map_err(|expected| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            cursor.fail(&expected),
                        )
                    })?,
                    times,
                ));
            }
            LocalEffectNode::Attach(attach) => {
                cursor.enter("lower.attach.attachment");
                // Only the destination is an antecedent for a later `it` /
                // `that creature` (the parser binds slot 0 of the destination
                // list), so a declared attached object records no binding.
                let mut attachment_bindings = Vec::new();
                let attachments = lower_ordinary_permanent_subject(
                    cursor,
                    &attach.attachment,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    if matches!(attach.attachment, PermanentOperandNode::Targets(_)) {
                        &mut attachment_bindings
                    } else {
                        &mut target_bindings
                    },
                )?;
                cursor.enter("lower.attach.recipient");
                let recipients = lower_ordinary_permanent_subject(
                    cursor,
                    &attach.recipient,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let ([attachment], [recipient]) = (attachments.as_slice(), recipients.as_slice())
                else {
                    return Err(EffectSequenceLoweringFailure::FailedSupport(
                        cursor.fail("one attached object and one attachment destination"),
                    ));
                };
                // Any typed reference to one object is an attach operand: the
                // source, a declared target, the entering object the event
                // observed (`attach this Equipment to it`, CR 400.7) or an
                // object bound earlier. An object is never attached to itself
                // (CR 701.3c): two operands naming the same reference (`attach
                // it to it`) leave the recipient ambiguous and are refused.
                cursor.enter("lower.attach.distinct-operands");
                cursor.require(
                    attachment != recipient,
                    "an attachment and a recipient that name different objects",
                )?;
                effects.push(json!({
                    "kind": "attachPermanent",
                    "attachment": attachment.clone(),
                    "to": recipient.clone(),
                }));
            }
            LocalEffectNode::PreventDamage(prevent) => {
                cursor.enter("lower.prevent.parties");
                let mut parties = Vec::with_capacity(3);
                for (party_index, party) in prevent.parties().into_iter().enumerate() {
                    parties.push(match party {
                        None => None,
                        Some(DamagePartyNode::Targets(targets)) => {
                            let slot = &targets.slots[0];
                            let (base_id, candidates) = lower_target_slot_candidates(slot)
                                .ok_or_else(|| {
                                    cursor.fail("a supported damage prevention party query")
                                })?;
                            let ordinal = used_target_base_ids
                                .iter()
                                .filter(|candidate| **candidate == base_id)
                                .count();
                            let target_count = permanent_target_base_ids
                                .iter()
                                .filter(|candidate| **candidate == base_id)
                                .count();
                            let target_id = target_id_for_ordinal(base_id, ordinal, target_count);
                            used_target_base_ids.push(base_id);
                            decisions.push(target_decision(
                                &target_id,
                                candidates,
                                i64::from(slot.cardinality.minimum),
                                i64::from(slot.cardinality.maximum),
                            ));
                            // The shield's one declared party (the protected
                            // target, else the targeted source) is the
                            // antecedent a later `that creature` names.
                            if prevention_declared_party_index(prevent)
                                .is_some_and(|(declared, _)| declared == party_index)
                                && targets.slots.len() == 1
                                && slot.cardinality == TargetCardinality::EXACTLY_ONE
                            {
                                target_bindings.push((
                                    TargetBinding {
                                        effect_index,
                                        slot_index: 0,
                                    },
                                    target_id.clone(),
                                ));
                            }
                            Some(chosen_target(&target_id))
                        }
                        Some(DamagePartyNode::Object(PermanentOperandNode::BoundReference(
                            reference,
                        ))) => Some(lower_bound_permanent_reference(
                            cursor,
                            *reference,
                            &target_bindings,
                        )?),
                        Some(party) => Some(
                            lower_nontargeted_damage_party(party)
                                .map_err(|error| cursor.fail(error.expectation()))?,
                        ),
                    });
                }
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                cursor.enter("lower.prevent.amount");
                let amount = lower_prevention_amount(prevent, declarations).map_err(|error| {
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail(error.expectation()),
                    )
                })?;
                effects.push(lower_damage_shield_parties(prevent, amount, parties));
            }
            LocalEffectNode::Choose(choice) => {
                // `choose target ...` is the target declaration itself (CR
                // 115.1, 601.2c): it registers the slot and its binding and
                // adds no resolution step.
                match &choice.domain {
                    ChoiceDomainNode::PermanentTargets(targets) => {
                        cursor.enter("lower.choose.permanent-targets");
                        lower_ordinary_permanent_subject(
                            cursor,
                            &PermanentOperandNode::Targets(targets.clone()),
                            effect_index,
                            false,
                            &permanent_target_base_ids,
                            &mut used_target_base_ids,
                            &mut decisions,
                            &mut target_bindings,
                        )?;
                    }
                    ChoiceDomainNode::PlayerTarget(targets) => {
                        cursor.enter("lower.choose.player-target");
                        let binding = PlayerTargetBinding { effect_index };
                        let [slot] = targets.slots.as_slice() else {
                            return Err(EffectSequenceLoweringFailure::FailedInvariant(
                                cursor.fail("one declared player target slot"),
                            ));
                        };
                        let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                            .ok_or_else(|| cursor.fail("a supported player target selector"))?;
                        let ordinal = player_target_bindings
                            .iter()
                            .position(|value| *value == binding)
                            .ok_or_else(|| cursor.fail("a declared player-target binding"))?;
                        let target_id =
                            player_target_id_for_ordinal(ordinal, player_target_bindings.len());
                        decisions.push(target_decision(&target_id, candidates, 1, 1));
                        declared_player_targets.push((binding, target_id));
                    }
                    ChoiceDomainNode::CreatureType => {
                        // CR 608.2d: the controller chooses while the program
                        // resolves; later `of the chosen type` reads this id.
                        cursor.enter("lower.choose.creature-type");
                        if creature_type_chosen {
                            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail("one creature-type choice per effect program"),
                            ));
                        }
                        creature_type_chosen = true;
                        effects.push(json!({
                            "kind": "chooseCreatureType",
                            "id": RESOLUTION_CREATURE_TYPE_DECISION,
                            "player": controller(),
                        }));
                    }
                    ChoiceDomainNode::Color => {
                        // CR 608.2d: one color decision per program, read
                        // later as `the chosen color`.
                        cursor.enter("lower.choose.color");
                        if color_chosen {
                            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail("one color choice per effect program"),
                            ));
                        }
                        color_chosen = true;
                        effects.push(super::resolution_choice::lower_color_choice());
                    }
                    ChoiceDomainNode::CardName(domain) => {
                        // CR 608.2d: a name chosen while the program
                        // resolves; `with the chosen name` reads its id.
                        cursor.enter("lower.choose.card-name");
                        effects.push(super::chosen_card_name::lower_resolution_card_name_choice(
                            domain,
                        ));
                    }
                    ChoiceDomainNode::Objects(choice) => {
                        // CR 608.2d: permanents chosen on resolution, not
                        // targets (CR 115.1). A later `it` / `those
                        // creatures` reads the binding the choice writes.
                        cursor.enter("lower.choose.objects");
                        let declarations = match context {
                            EffectSequenceLoweringContext::NoExternalX => {
                                TargetlessDeclarationSupport::None
                            }
                            EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                                TargetlessDeclarationSupport::XValue
                            }
                        };
                        let lowered = super::resolution_object_choice::lower_object_choice(
                            choice,
                            effect_index,
                            declarations,
                        )
                        .map_err(|expected| {
                            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail(expected),
                            )
                        })?;
                        effects.extend(lowered.choice);
                        pending_choice_remainder = lowered.remainder;
                        if let Some(binding) = lowered.chosen {
                            let key = resolution_choice_binding(effect_index);
                            target_bindings.push((key, binding.clone()));
                            target_bindings.push((
                                super::permanent_subject::target_set_binding_key(key),
                                binding.clone(),
                            ));
                            chosen_object_bindings.push(binding);
                        }
                    }
                    ChoiceDomainNode::Number(node) => {
                        // CR 608.2d: a number chosen on resolution; `that
                        // many` binds it through the action-result quantity.
                        cursor.enter("lower.choose.number");
                        let id =
                            super::resolution_object_choice::chosen_number_decision(effect_index);
                        effects.push(super::resolution_object_choice::lower_number_choice(
                            *node, &id,
                        ));
                        chosen_numbers.push(id);
                    }
                    ChoiceDomainNode::Player(node) => {
                        // A player chosen on resolution, not a target (CR
                        // 115.1): a later `that player` reads its binding.
                        cursor.enter("lower.choose.player");
                        let id =
                            super::resolution_choice::resolution_player_choice_id(effect_index);
                        effects.push(super::resolution_choice::lower_player_choice(*node, &id));
                        declared_player_targets
                            .push((PlayerTargetBinding { effect_index }, id.clone()));
                        resolution_players.push(id);
                    }
                }
            }
            LocalEffectNode::UntapTarget(untap) => {
                cursor.enter("lower.untap-target.permanent-binding");
                let reference =
                    lower_bound_permanent_reference(cursor, untap.permanent, &target_bindings)?;
                effects.push(json!({"kind":"untapPermanent", "permanent":reference}));
            }
            LocalEffectNode::ReferencedPermanentAction(destroy) => {
                cursor.enter("lower.destroy-reference.permanent-binding");
                let mut reference =
                    lower_bound_permanent_reference(cursor, destroy.object, &target_bindings)?;
                // The reference names one permanent (`it`, `that NOUN`): the
                // one token a creation bound is read as that single object.
                if reference["kind"] == "boundObjects" {
                    reference["kind"] = json!("singleBoundObject");
                }
                let kind = match destroy.action {
                    ReferencedPermanentAction::Destroy => "destroyPermanent",
                    ReferencedPermanentAction::Exile => "exilePermanent",
                    ReferencedPermanentAction::Sacrifice => "sacrificePermanent",
                    // CR 400.3: the permanent goes to its owner's library.
                    ReferencedPermanentAction::PutOnOwnerLibrary(placement) => {
                        let mut operation = json!({
                            "kind": "putPermanentOnOwnerLibrary",
                            "permanent": reference,
                            "position": match placement {
                                OwnerLibraryPlacement::Top
                                | OwnerLibraryPlacement::BeneathTopCards(_) => "top",
                                OwnerLibraryPlacement::Bottom => "bottom",
                                OwnerLibraryPlacement::OwnerChoiceTopOrBottom => {
                                    "ownerChoiceTopOrBottom"
                                }
                            },
                        });
                        // `ORDINAL from the top`: the cards that stay above it.
                        if let OwnerLibraryPlacement::BeneathTopCards(above) = placement {
                            operation["cardsAbove"] = lower_numeric_atom(above);
                        }
                        effects.push(operation);
                        continue;
                    }
                    // CR 400.3: the permanent goes to its owner's hand.
                    ReferencedPermanentAction::ReturnToOwnerHand => {
                        effects.push(json!({
                            "kind": "returnToOwnersHand",
                            "permanent": reference,
                        }));
                        continue;
                    }
                    // CR 701.24a: put into the owner's library, then that
                    // library is shuffled (its owner by last known
                    // information once the permanent has left).
                    ReferencedPermanentAction::ShuffleIntoOwnerLibrary => {
                        effects.push(json!({
                            "kind": "putPermanentOnOwnerLibrary",
                            "permanent": reference.clone(),
                            "position": "bottom",
                        }));
                        effects.push(json!({
                            "kind": "shuffleZone",
                            "zone": {
                                "kind": "library",
                                "player": {"kind": "ownerOf", "object": reference},
                            },
                        }));
                        continue;
                    }
                };
                let mut lowered = json!({"kind": kind, "permanent": reference});
                if destroy.cannot_regenerate {
                    lowered["cannotRegenerate"] = json!(true);
                }
                effects.push(lowered);
            }
            LocalEffectNode::DealDamage(damage) if damage.fixed_source().is_none() => {
                // An object subject deals the damage (CR 120.1): its slots are
                // declared first, then the recipient's, in written order.
                cursor.enter("lower.damage.object-source");
                let sources = lower_ordinary_permanent_subject(
                    cursor,
                    &damage.source,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let [source] = sources.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::FailedSupport(
                        cursor.fail("exactly one object dealing the damage"),
                    ));
                };
                // Any typed reference to one object deals the damage (CR
                // 120.1): a declared target, the permanent the source is
                // attached to, the event's object, an object bound earlier
                // (a created token, the card a move put onto the
                // battlefield) or the source itself. The runtime resolves
                // each with its own departure rule (CR 608.2b, 608.2h).
                let source = &cursor.require_value(
                    super::permanent_subject::single_object_reference(source.clone()),
                    "one object dealing the damage: a target, an attached, event, bound or source object",
                )?;
                // `up to one other target creature ... each deal damage
                // equal to their power`: a declared source that may be
                // absent deals its damage once per declared object, zero or
                // one time (CR 608.2c); its amount reads that member.
                let optional_member = optional_declared_source(sequence, damage, &target_bindings);
                let amount = match &optional_member {
                    Some(member) => lower_numeric_expression_for_targets(
                        &damage.amount,
                        std::slice::from_ref(member),
                    )
                    .map_err(|_| {
                        cursor.enter("sequence.target-numeric.optional-member");
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            cursor.fail("an amount that reads only the optional declared source"),
                        )
                    })?,
                    None => lower_sequence_bound_amount(
                        cursor,
                        &damage.amount,
                        sequence,
                        &target_bindings,
                        context,
                    )?,
                };
                let first_damage = effects.len();
                let recipients = match &damage.recipient {
                    DamageRecipientNode::Targets(targets) => {
                        cursor.enter("lower.damage.object-source.recipient");
                        // `other` names the object dealing the damage; the
                        // shared slot marks exclude the ability source and
                        // earlier targets, never an attached object.
                        cursor
                            .require(
                                source["kind"].as_str() == Some("chosenTarget")
                                    || targets.slots.iter().all(|slot| !slot.exclude_source),
                                "an `other` recipient relation to a damage source that is not a declared target",
                            )
                            .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
                        let (base_id, candidates, minimum, maximum) = lower_single_target(targets);
                        let ordinal = used_target_base_ids
                            .iter()
                            .filter(|candidate| **candidate == base_id)
                            .count();
                        let target_count = permanent_target_base_ids
                            .iter()
                            .filter(|candidate| **candidate == base_id)
                            .count();
                        let target_id = target_id_for_ordinal(base_id, ordinal, target_count);
                        used_target_base_ids.push(base_id);
                        decisions.push(target_decision(&target_id, candidates, minimum, maximum));
                        // A singular recipient is the antecedent a later
                        // reference names (`... to that creature`).
                        if let [slot] = targets.slots.as_slice()
                            && slot.cardinality == TargetCardinality::EXACTLY_ONE
                            && slot.count.is_none()
                        {
                            target_bindings.push((
                                TargetBinding {
                                    effect_index,
                                    slot_index: 0,
                                },
                                target_id.clone(),
                            ));
                        }
                        vec![damage_target_recipient(&target_id, maximum)]
                    }
                    DamageRecipientNode::All(groups) => groups
                        .iter()
                        .map(|group| {
                            lower_fixed_damage_recipient(
                                group,
                                context,
                                &target_bindings,
                                &declared_player_targets,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| {
                            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail(error.expectation()),
                            )
                        })?,
                    group => {
                        vec![
                            lower_fixed_damage_recipient(
                                group,
                                context,
                                &target_bindings,
                                &declared_player_targets,
                            )
                            .map_err(|error| {
                                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                    cursor.fail(error.expectation()),
                                )
                            })?,
                        ]
                    }
                };
                for mut recipient in recipients {
                    // `each other creature`: `other` is relative to the object
                    // dealing the damage, never to the resolving spell.
                    if recipient["excludeSource"] == json!(true) {
                        recipient
                            .as_object_mut()
                            .expect("a collection recipient is an object")
                            .remove("excludeSource");
                        recipient["exclude"] =
                            super::permanent_subject::exclusion_of_single_object(source);
                    }
                    effects.push(json!({
                        "kind": "dealDamage",
                        "source": source.clone(),
                        "amount": amount.clone(),
                        "recipient": recipient,
                    }));
                }
                if let Some(member) = optional_member {
                    let bind = format!("iteratedObject{effect_index}");
                    let singular = chosen_target(&member.target_id);
                    let mut body = effects.split_off(first_damage);
                    for effect in &mut body {
                        replace_json_value(
                            effect,
                            &singular,
                            &json!({"kind": "boundValue", "id": bind}),
                        );
                    }
                    effects.push(json!({
                        "kind": "forEachObject",
                        "objects": {"kind": "chosenTargets", "id": member.target_id},
                        "bind": bind,
                        "effects": body,
                    }));
                }
            }
            LocalEffectNode::Fight(fight) => {
                // CR 701.14a: each fighter deals damage equal to its power to
                // the other. The second fighter's slots never become anaphors.
                cursor.enter("lower.fight.first");
                let first = lower_ordinary_permanent_subject(
                    cursor,
                    &fight.first,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                let bound = target_bindings.len();
                cursor.enter("lower.fight.second");
                let second = lower_ordinary_permanent_subject(
                    cursor,
                    &fight.second,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
                // The second fighter's slot is the action's antecedent only
                // when the first fighter declares none (the parser publishes
                // the same slot, `permanent_subject_after_effect`).
                if matches!(fight.first, PermanentOperandNode::Targets(_)) {
                    target_bindings.truncate(bound);
                }
                let ([first], [second]) = (first.as_slice(), second.as_slice()) else {
                    return Err(EffectSequenceLoweringFailure::FailedSupport(
                        cursor.fail("exactly one permanent on each side of a fight"),
                    ));
                };
                effects.push(json!({
                    "kind": "fightPermanents",
                    "first": first,
                    "second": second,
                }));
            }
            LocalEffectNode::DealDamage(damage) if damage.recipient.is_bound() => {
                // The source is fixed; every recipient group is a fixed
                // group or a reference the binding environment resolved.
                cursor.enter("lower.damage.bound-recipient");
                let source = match damage.fixed_source() {
                    Some(LocalObjectReference::SourceSelf) => self_ref(),
                    None => {
                        return Err(EffectSequenceLoweringFailure::FailedSupport(
                            cursor.fail("a fixed damage source for a bound recipient"),
                        ));
                    }
                };
                let amount = lower_sequence_bound_amount(
                    cursor,
                    &damage.amount,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                let groups = match &damage.recipient {
                    DamageRecipientNode::All(groups) => groups.iter().collect::<Vec<_>>(),
                    group => vec![group],
                };
                for group in groups {
                    let recipient = lower_fixed_damage_recipient(
                        group,
                        context,
                        &target_bindings,
                        &declared_player_targets,
                    )
                    .map_err(|error| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            cursor.fail(error.expectation()),
                        )
                    })?;
                    effects.push(json!({
                        "kind": "dealDamage",
                        "source": source.clone(),
                        "amount": amount.clone(),
                        "recipient": recipient,
                    }));
                }
            }
            LocalEffectNode::DealDamage(damage)
                if matches!(&damage.recipient, DamageRecipientNode::Targets(_)) =>
            {
                let base_id = default_damage_target_id(damage)
                    .ok_or_else(|| cursor.fail("a targeted damage recipient"))?;
                let ordinal = used_target_base_ids
                    .iter()
                    .filter(|candidate| **candidate == base_id)
                    .count();
                let target_count = permanent_target_base_ids
                    .iter()
                    .filter(|candidate| **candidate == base_id)
                    .count();
                let target_id = target_id_for_ordinal(base_id, ordinal, target_count);
                used_target_base_ids.push(base_id);
                // An amount read from an earlier target of the program
                // (`damage equal to that permanent's mana value`) is lowered
                // against the sequence's bindings.
                let lowered = if damage.amount.requires_target_binding() {
                    let amount = lower_sequence_bound_amount(
                        cursor,
                        &damage.amount,
                        sequence,
                        &target_bindings,
                        context,
                    )?;
                    super::damage::lower_targeted_damage_effect_with_amount(
                        damage, &target_id, amount,
                    )
                } else {
                    lower_targeted_damage_effect(damage, &target_id)
                }
                .ok_or_else(|| cursor.fail("a targeted damage effect"))?;
                decisions.push(lowered.decision);
                effects.push(lowered.effect);
                // A singular damage target is the antecedent a later reference
                // (`that creature`, `its controller`) resolves to.
                if let DamageRecipientNode::Targets(targets) = &damage.recipient
                    && let [slot] = targets.slots.as_slice()
                {
                    let binding = TargetBinding {
                        effect_index,
                        slot_index: 0,
                    };
                    if slot.cardinality == TargetCardinality::EXACTLY_ONE {
                        target_bindings.push((binding, target_id.clone()));
                    } else if slot.cardinality.maximum > 1 {
                        // A plural recipient slot is the set a later `those
                        // creatures` names (`TargetSet`).
                        target_bindings.push((
                            super::permanent_subject::target_set_binding_key(binding),
                            target_id.clone(),
                        ));
                    }
                }
            }
            LocalEffectNode::ZoneChange(zone_change) => {
                // `other` after an earlier target can express a relation to
                // that target. A new collection must not approximate it as
                // source exclusion while distinct-target constraints are absent.
                let first = &zone_change.target.slots[0];
                if (first.cardinality.maximum > 1 || zone_change.target.slots.len() > 1)
                    && first.exclude_source
                    && decisions
                        .iter()
                        .any(|decision| decision["kind"] == "chooseTargets")
                {
                    cursor.enter("lower.zone-change.another-target-relation");
                    return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
                        "a represented distinct-target relation for the plural other-target operand",
                    )));
                }
                let (slot_decisions, target) =
                    lower_zone_change_targets(cursor, zone_change, |base_id, _| {
                        let ordinal = used_target_base_ids
                            .iter()
                            .filter(|candidate| **candidate == base_id)
                            .count();
                        let target_count = permanent_target_base_ids
                            .iter()
                            .filter(|candidate| **candidate == base_id)
                            .count();
                        used_target_base_ids.push(base_id);
                        target_id_for_ordinal(base_id, ordinal, target_count)
                    })?;
                let scalar_id = match (
                    slot_decisions.as_slice(),
                    zone_change.target.slots.as_slice(),
                ) {
                    ([decision], [slot])
                        if slot.cardinality.minimum == 1 && slot.cardinality.maximum == 1 =>
                    {
                        decision["id"].as_str().map(str::to_string)
                    }
                    _ => None,
                };
                let single_scalar = zone_change.target.slots.len() == 1
                    && zone_change.target.slots[0].cardinality.maximum == 1;
                decisions.extend(slot_decisions);
                let mut operation = lower_targeted_zone_change_node(cursor, zone_change, target)?;
                // A later card action reading `that card`/`it` names the object
                // this exile produced; record that result only when it is read.
                if zone_change.action == ZoneChangeAction::Exile
                    && (reads_card_collection_reference(&sequence.effects[effect_index + 1..])
                        || collections
                            .continuation_reads_cards_after(effect_index, sequence.effects.len()))
                {
                    // A declared collection (`exile any number of target
                    // ..., then return those cards`) is observed the same
                    // way: the exile binds every card it actually moved.
                    operation["bind"] = json!(if single_scalar {
                        collections.observe_exiled_object()
                    } else {
                        let literal_maximum = match zone_change.target.slots.as_slice() {
                            [slot] if slot.count.is_none() => {
                                Some(i64::from(slot.cardinality.maximum))
                            }
                            _ => None,
                        };
                        collections.observe_exiled_objects(literal_maximum)
                    });
                }
                effects.push(operation);
                if let Some(target_id) = scalar_id {
                    target_bindings.push((
                        TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                        target_id,
                    ));
                }
            }
            LocalEffectNode::TargetCardZoneChange(zone_change) => {
                let target_id = card_target_id_for_ordinal(card_target_ordinal, card_target_count);
                card_target_ordinal += 1;
                let mut decision = json!({
                    "id": target_id.clone(),
                    "kind": "chooseTargets",
                    "minimum": lower_target_cardinality_bound(zone_change.cardinality.minimum),
                    "maximum": lower_target_cardinality_bound(zone_change.cardinality.maximum),
                    "candidates": lower_target_card_move_candidates(zone_change),
                });
                if zone_change.source_zone == TargetCardSourceZone::SingleGraveyard
                    && zone_change.cardinality != ExpressedTargetCardinality::EXACTLY_ONE
                {
                    decision["selectionConstraint"] = json!({
                        "kind": "sameZoneOwner",
                        "zone": "graveyard",
                    });
                }
                decisions.push(decision);
                let mut operation = lower_target_card_move_effect(zone_change, &target_id)
                    .ok_or_else(|| cursor.fail("a supported targeted card move"))?;
                // A later card action reading `it`/`that card` names the card
                // this exile produced (CR 400.7); bound only when read.
                if zone_change.action == CardZoneChangeAction::Exile
                    && (reads_card_collection_reference(&sequence.effects[effect_index + 1..])
                        || collections
                            .continuation_reads_cards_after(effect_index, sequence.effects.len()))
                {
                    operation["bind"] = json!(match zone_change.cardinality.maximum {
                        NumericAtom::Integer(1) => collections.observe_exiled_object(),
                        NumericAtom::Integer(maximum) => {
                            collections.observe_exiled_objects(Some(maximum))
                        }
                        NumericAtom::VariableX => collections.observe_exiled_objects(None),
                    });
                }
                effects.push(operation);
                if zone_change.cardinality == ExpressedTargetCardinality::EXACTLY_ONE {
                    card_target_bindings.push((
                        TargetBinding {
                            effect_index,
                            slot_index: 0,
                        },
                        target_id,
                    ));
                }
            }
            LocalEffectNode::CreateNamedTokens(creation) => {
                cursor.enter("lower.create-named-token.controller");
                let token_controller = match creation.player {
                    LocalPlayerReference::Controller => controller(),
                    LocalPlayerReference::ControllerOfQualified(_)
                    | LocalPlayerReference::Bound(_) => {
                        cursor.enter("sequence.player-reference.qualified-controller-support");
                        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                            "a proved nominal/controller snapshot binding for this program parent")));
                    }
                    LocalPlayerReference::ControllerOf(binding) => {
                        let target_id = target_bindings
                            .iter()
                            .find_map(|(candidate, target_id)| {
                                (*candidate == binding).then_some(target_id)
                            })
                            .ok_or_else(|| {
                                cursor.fail(
                                    "an earlier permanent target for the token-controller anaphor",
                                )
                            })?;
                        json!({
                            "kind": "controllerOf",
                            "object": chosen_target(target_id),
                        })
                    }
                };
                cursor.enter("lower.create-named-token.quantity");
                effects.push(lower_named_token_creation(
                    cursor,
                    &creation.creation,
                    token_controller,
                    false,
                )?);
            }
            LocalEffectNode::GainLife(life) => {
                effects.push(lower_sequence_bound_life(
                    cursor,
                    "gainLife",
                    &life.player,
                    &life.amount,
                    sequence,
                    &target_bindings,
                    context,
                )?);
            }
            LocalEffectNode::LoseLife(life) => {
                effects.push(lower_sequence_bound_life(
                    cursor,
                    "loseLife",
                    &life.player,
                    &life.amount,
                    sequence,
                    &target_bindings,
                    context,
                )?);
            }
            LocalEffectNode::TargetPlayerLife(life) => {
                cursor.enter("lower.target-player-life.binding");
                if life.declares_target
                    && !declared_player_targets
                        .iter()
                        .any(|(binding, _)| binding == &life.player)
                {
                    let ordinal = player_target_bindings
                        .iter()
                        .position(|binding| binding == &life.player)
                        .ok_or_else(|| cursor.fail("a declared player-target binding"))?;
                    let target_id =
                        player_target_id_for_ordinal(ordinal, player_target_bindings.len());
                    decisions.push(target_decision(
                        &target_id,
                        json!({ "kind": "players" }),
                        1,
                        1,
                    ));
                    declared_player_targets.push((life.player, target_id));
                }
                let target_id = declared_player_targets
                    .iter()
                    .find_map(|(binding, target_id)| (*binding == life.player).then_some(target_id))
                    .ok_or_else(|| cursor.fail("an earlier declared player target binding"))?;
                let amount = match lower_numeric_expression(&life.amount) {
                    Some(amount) => amount,
                    None => lower_sequence_bound_amount(
                        cursor,
                        &life.amount,
                        sequence,
                        &target_bindings,
                        context,
                    )?,
                };
                effects.push(json!({
                    "kind": match life.action {
                        TargetPlayerLifeAction::Gain => "gainLife",
                        TargetPlayerLifeAction::Lose => "loseLife",
                    },
                    "player": chosen_target(target_id),
                    "amount": amount,
                }));
            }
            LocalEffectNode::Earthbend(earthbend) => {
                // CR 701.65a: the keyword action targets a land its performer
                // controls. The sequence owns that target declaration, as for
                // any other targeted action; the action keeps its primitives.
                cursor.enter("lower.earthbend.quantity");
                cursor.require(
                    !numeric_atom_uses_x(&earthbend.quantity),
                    "an Earthbend quantity independent of an unavailable X binding",
                )?;
                cursor.enter("lower.earthbend.target");
                let target_id = earthbend_target_id(effect_index);
                cursor.require(
                    !used_target_base_ids.contains(&target_id.as_str()),
                    "a unique Earthbend target declaration",
                )?;
                let lowered = lower_earthbend_action(
                    earthbend,
                    &target_id,
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Land"),
                    }),
                    1,
                    1,
                );
                decisions.push(lowered.decision);
                effects.extend(lowered.effects);
            }
            LocalEffectNode::CreateExplicitTokens(node) if node.body.object_operand().is_some() => {
                // `create a token that's a copy of OBJECT` / `create a Role
                // token attached to OBJECT`: the object is an operand of the
                // shared permanent-subject lowering, so a target is declared
                // like any other action's target.
                let operand = node.body.object_operand().expect("an object operand");
                cursor.enter("lower.token-copy.player");
                cursor.require(
                    node.player == LocalPlayerReference::Controller,
                    "the controller creating the token copies",
                )?;
                cursor.enter("lower.token-copy.quantity");
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                let quantity =
                    lower_targetless_numeric_expression(&node.body.quantity, declarations)
                        .map_err(|error| {
                            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail(error.expectation()),
                            )
                        })?;
                cursor.enter("lower.token-copy.object");
                let objects = match operand {
                    // A copy reads copiable values in any zone (CR 707.2): a
                    // bound copied object may be a card target an earlier card
                    // action of this program declared.
                    PermanentOperandNode::BoundReference(reference) => {
                        vec![lower_bound_permanent_reference(
                            cursor,
                            *reference,
                            &[target_bindings.as_slice(), card_target_bindings.as_slice()].concat(),
                        )?]
                    }
                    _ => lower_ordinary_permanent_subject(
                        cursor,
                        operand,
                        effect_index,
                        false,
                        &permanent_target_base_ids,
                        &mut used_target_base_ids,
                        &mut decisions,
                        &mut target_bindings,
                    )?,
                };
                let [object] = <[Value; 1]>::try_from(objects)
                    .map_err(|_| cursor.fail("one copied object"))?;
                let mut creation = lower_token_creation_with_object(
                    &node.body,
                    Some(object),
                    controller(),
                    quantity,
                )
                .map_err(|expected| cursor.fail(expected))?;
                if created_tokens_are_next_antecedent(sequence, effect_index) {
                    creation["bind"] = json!(CREATED_TOKENS_BINDING);
                }
                effects.push(creation);
            }
            effect => {
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                cursor.enter("lower.targetless-effect");
                let mut lowered = lower_targetless_local_effects(
                    effect,
                    declarations,
                    Some(&effect_resolution_scope(effect_index)),
                )
                .map_err(|error| {
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail(error.expectation()),
                    )
                })?;
                // `create ... tokens. They gain ...`: the created tokens are
                // bound only when the next action names them.
                if matches!(effect, LocalEffectNode::CreateExplicitTokens(_))
                    && created_tokens_are_next_antecedent(sequence, effect_index)
                    && let [creation] = lowered.as_mut_slice()
                    && creation["kind"] == "createTokens"
                {
                    creation["bind"] = json!(CREATED_TOKENS_BINDING);
                }
                // `draw N cards and reveal them`: the drawn cards are the
                // antecedent of the next card action, never otherwise bound.
                if let LocalEffectNode::DrawCards(draw) = effect
                    && draw.player == LocalPlayerReference::Controller
                    && drawn_cards_are_next_antecedent(sequence, effect_index)
                    && let [draw_effect] = lowered.as_mut_slice()
                    && draw_effect["kind"] == "drawCards"
                {
                    let maximum = match &draw.count {
                        NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                        _ => None,
                    };
                    draw_effect["bind"] = json!(collections.observe_drawn_cards(maximum));
                }
                // `exile all creatures. Return those cards ...`: the
                // permanents a collection exile moved are the observed exile
                // collection of the next card action (CR 400.7: the cards
                // they became), bound only when read.
                if let LocalEffectNode::SelectedPermanents(selected) = effect
                    && selected.action == SelectedPermanentAction::Exile
                    && selected.operand.bounds.is_none()
                    && selected.until.is_none()
                    && (reads_card_collection_reference(&sequence.effects[effect_index + 1..])
                        || collections
                            .continuation_reads_cards_after(effect_index, sequence.effects.len()))
                    && let [exile] = lowered.as_mut_slice()
                    && exile["kind"] == "exilePermanent"
                {
                    exile["bind"] = json!(collections.observe_exiled_objects(None));
                }
                effects.extend(lowered);
            }
        }
    }

    declare_query_possessor_targets(
        cursor,
        &mut effects,
        &mut decisions,
        &mut declared_player_targets,
    )?;
    apply_performer(
        cursor,
        &mut effects,
        pending_performer.take(),
        &[target_bindings.as_slice(), card_target_bindings.as_slice()].concat(),
        &declared_player_targets,
    )?;
    effects.append(&mut pending_choice_remainder);
    conditional_scopes.at_boundary(
        cursor,
        sequence,
        sequence.effects.len(),
        collections,
        &target_bindings,
        &card_target_bindings,
        PreviousActionResult {
            action_starts: &action_starts,
            effects: &mut effects,
        },
    )?;
    action_starts.push((effects.len(), decisions.len()));
    let result_object_plans = plan_action_result_objects(sequence, &action_starts, &mut effects);
    install_action_result_bindings(cursor, sequence, &action_starts, &mut effects)?;
    install_action_result_quantities(cursor, &action_starts, &mut effects)?;
    install_action_result_object_counts(&action_starts, &mut effects).map_err(|expected| {
        cursor.enter("lower.action-result.count");
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
    })?;
    bind_deployed_card_when_read(cursor, &mut effects, &deployed_choices, collections)?;
    bind_returned_source_when_read(cursor, &mut effects, collections)?;
    bind_battlefield_search_when_read(cursor, sequence, &mut effects, collections)?;
    effect_alternatives::apply_alternative_frames(
        cursor,
        sequence,
        &mut action_starts,
        &mut effects,
    )?;
    apply_delayed_frames(
        cursor,
        &sequence.delayed,
        &target_bindings,
        &inherited_bindings,
        &mut action_starts,
        &mut effects,
    )?;
    apply_unless_frames(
        cursor,
        sequence,
        &sequence.unless,
        &mut action_starts,
        &decisions,
        &target_bindings,
        &declared_player_targets,
        &mut effects,
    )?;
    program_iteration_lowering::apply_iteration_frames(
        cursor,
        &sequence.iterations,
        &mut target_bindings,
        &mut action_starts,
        &mut decisions,
        &mut effects,
    )?;
    // Before the conditional instructions are wrapped, while every action
    // start is a top-level position: a binding precedes the sentences and
    // conditions that read it.
    apply_where_x_bindings(
        cursor,
        &mut action_starts,
        &mut decisions,
        sequence,
        &target_bindings,
        &mut conditional_scopes.conditions,
        &mut effects,
    )?;
    conditional_scopes.wrap(
        cursor,
        sequence,
        &action_starts,
        &mut decisions,
        context == EffectSequenceLoweringContext::ValidateXAfterCardComposition,
        &mut effects,
    )?;
    resolve_optional_outcomes(cursor, &mut effects)?;
    for effect in &mut effects {
        super::resolution_choice::rebind_resolution_players(effect, &resolution_players);
        super::resolution_object_choice::rebind_chosen_objects(effect, &chosen_object_bindings);
    }
    cursor.enter("lower.choose.objects.consumer");
    if !super::resolution_object_choice::every_chosen_object_is_read(
        &effects,
        &chosen_object_bindings,
    ) {
        return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
            "an executable binding consumer for objects chosen on resolution",
        )));
    }
    cursor.enter("lower.choose.number.consumer");
    if !super::resolution_object_choice::every_chosen_number_is_read(&effects, &chosen_numbers) {
        return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
            "a later `that many` / `that much` reading the number chosen on resolution",
        )));
    }
    cursor.enter("lower.choose.player.consumer");
    if !super::resolution_choice::every_resolution_player_is_read(&effects, &resolution_players) {
        return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
            "a later instruction of the program that reads the player chosen on resolution",
        )));
    }
    cursor.enter("lower.ability-loss.later-grant");
    if !super::become_types::no_grant_after_ability_loss(&effects) {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("a layer-6 timestamp order between losing all abilities and a later grant"),
        ));
    }
    super::resolve_relation_target_anchors(cursor, &mut effects, &target_bindings)
        .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
    super::resolve_bound_player_anchors(cursor, &mut effects, &declared_player_targets)
        .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
    cursor.enter("lower.action-result.object");
    resolve_action_result_objects(&result_object_plans, &mut effects).map_err(|expected| {
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
    })?;

    Ok(LoweredEffectSequence { decisions, effects })
}

/// Per-sentence state of the conditional instructions of one sequence
/// (CR 608.2c). Each condition is lowered where its `If` sentence begins, with
/// the bindings visible there; each branch starts from the same
/// card-collection scope, and a scope that differs after the instruction does
/// not escape it. A replacing (`instead`) conditional opens its scope where
/// the replaced sentence begins: its `If` branch starts from the scope before
/// the replaced actions, which it replaces.
struct ConditionalScopes {
    conditions: Vec<Option<Value>>,
    /// A condition that reads a target its own branch declares (`destroy
    /// target creature if it's tapped`): targets are declared at casting
    /// (CR 601.2c), before any condition is checked, so the condition is
    /// lowered once its branch's targets are known, with the position-bound
    /// inputs read where its `If` sentence begins.
    deferred: Vec<Option<DeferredCondition>>,
    before: Vec<Option<CardCollectionMemory>>,
    after_then: Vec<Option<CardCollectionMemory>>,
}

/// The inputs of a deferred condition read where its branch begins.
struct DeferredCondition {
    card_focus: Option<Value>,
    previous_action_result: Option<String>,
}

impl ConditionalScopes {
    fn new(
        cursor: &mut OracleTokenCursor<'_>,
        sequence: &EffectSequenceNode,
    ) -> Result<Self, EffectSequenceLoweringFailure> {
        cursor.enter("lower.sentence-conditional.ranges");
        let mut previous_end = 0;
        for conditional in &sequence.conditionals {
            cursor.require(
                previous_end <= conditional.start()
                    && conditional.then_first < conditional.then_end
                    && conditional.else_end.is_none_or(|end| {
                        conditional.then_end < end || end < conditional.then_first
                    })
                    && conditional.end() <= sequence.effects.len(),
                "ordered, nonempty and disjoint conditional sentence ranges",
            )?;
            previous_end = conditional.end();
        }
        // Later lowering failures belong to the action being lowered, not
        // to the conditional ranges checked here.
        cursor.enter("lower.effect-sequence.actions");
        let count = sequence.conditionals.len();
        Ok(Self {
            conditions: vec![None; count],
            deferred: (0..count).map(|_| None).collect(),
            before: vec![None; count],
            after_then: vec![None; count],
        })
    }

    fn at_boundary(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        sequence: &EffectSequenceNode,
        effect_index: usize,
        collections: &mut CardCollectionMemory,
        permanent_targets: &[(TargetBinding, String)],
        card_targets: &[(TargetBinding, String)],
        mut previous: PreviousActionResult<'_>,
    ) -> Result<(), EffectSequenceLoweringFailure> {
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            if conditional.then_end == effect_index
                && let Some(deferred) = self.deferred[index].take()
            {
                let bindings = ConditionObjectBindings {
                    permanent_targets,
                    card_targets,
                    card_focus: deferred.card_focus,
                    previous_action_result: deferred.previous_action_result,
                };
                let mut condition =
                    lower_sentence_condition(cursor, &conditional.condition, &bindings)
                        .map_err(EffectSequenceLoweringFailure::FailedSupport)?;
                bind_action_result_objects(
                    previous.action_starts,
                    previous.effects.as_mut_slice(),
                    conditional.then_first,
                    &mut condition,
                )
                .map_err(|expected| {
                    cursor.enter("lower.action-result.condition");
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
                })?;
                bind_condition_action_result_quantities(
                    cursor,
                    previous.action_starts,
                    previous.effects.as_mut_slice(),
                    &mut condition,
                )?;
                self.conditions[index] = Some(condition);
            }
        }
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            if conditional.end() == effect_index {
                let reference = self.after_then[index]
                    .as_ref()
                    .or(self.before[index].as_ref())
                    .expect("a conditional scope opens before it closes");
                let before = self.before[index]
                    .as_ref()
                    .expect("a conditional scope opens before it closes");
                if !collections.same_scope(reference) || !collections.same_scope(before) {
                    // A branch-local card antecedent is not proven after the
                    // instruction; a later reference fails explicitly.
                    collections.clear_antecedent();
                } else {
                    collections.keep_common_location_facts(&[reference, before]);
                }
            }
        }
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            if conditional.otherwise_end().is_some() && conditional.then_end == effect_index {
                self.after_then[index] = Some(collections.clone());
                let before = self.before[index]
                    .as_ref()
                    .expect("a conditional scope opens before its other branch");
                collections.restore_scope(before);
            }
        }
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            // The replaced sentence is the branch written first; the
            // replacing branch starts from the scope before it.
            if conditional.replaced_first() == Some(effect_index) {
                self.before[index] = Some(collections.clone());
            }
            if conditional.replaced_first().is_some() && conditional.then_first == effect_index {
                self.after_then[index] = Some(collections.clone());
                let before = self.before[index]
                    .as_ref()
                    .expect("a replaced sentence opens its conditional scope");
                collections.restore_scope(before);
            }
        }
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            if conditional.then_first == effect_index {
                let previous_action_result =
                    if sentence_condition_reads_previous_action(&conditional.condition) {
                        previous.bind(cursor, effect_index)?
                    } else {
                        None
                    };
                let card_focus = collections.singular_focus_object();
                let bindings = ConditionObjectBindings {
                    permanent_targets,
                    card_targets,
                    card_focus: card_focus.clone(),
                    previous_action_result: previous_action_result.clone(),
                };
                let mut condition =
                    match lower_sentence_condition(cursor, &conditional.condition, &bindings) {
                        Ok(condition) => condition,
                        // The condition may read a target its own branch
                        // declares; it is lowered again once that branch's
                        // targets are bound, and fails there if still unbound.
                        Err(_) if conditional.replaced_first().is_none() => {
                            self.deferred[index] = Some(DeferredCondition {
                                card_focus,
                                previous_action_result,
                            });
                            self.before[index] = Some(collections.clone());
                            continue;
                        }
                        Err(failure) => {
                            return Err(EffectSequenceLoweringFailure::FailedSupport(failure));
                        }
                    };
                // `if a land card was milled this way` reads the action before it.
                bind_action_result_objects(
                    previous.action_starts,
                    previous.effects.as_mut_slice(),
                    effect_index,
                    &mut condition,
                )
                .map_err(|expected| {
                    cursor.enter("lower.action-result.condition");
                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
                })?;
                bind_condition_action_result_quantities(
                    cursor,
                    previous.action_starts,
                    previous.effects.as_mut_slice(),
                    &mut condition,
                )?;
                self.conditions[index] = Some(condition);
                if conditional.replaced_first().is_none() {
                    self.before[index] = Some(collections.clone());
                }
            }
        }
        Ok(())
    }

    /// Replaces each conditional's action range by one `conditional` node.
    /// Target declarations stay ability-wide (CR 601.2c); a branch governed
    /// by a cast choice such as kicker cannot own a required target. A
    /// target both branches act on is required whichever branch is taken
    /// (`Target creature gets -2/-2 ... If this spell was kicked, that
    /// creature gets -6/-6 ... instead.`).
    fn wrap(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        sequence: &EffectSequenceNode,
        action_starts: &[(usize, usize)],
        decisions: &mut [Value],
        spell_declarations: bool,
        effects: &mut Vec<Value>,
    ) -> Result<(), EffectSequenceLoweringFailure> {
        for (index, conditional) in sequence.conditionals.iter().enumerate().rev() {
            let (start_from, decisions_from) = action_starts[conditional.start()];
            let (then_from, _) = action_starts[conditional.then_first];
            let (then_to, _) = action_starts[conditional.then_end];
            let (else_to, decisions_to) = action_starts[conditional.end()];
            let condition = self.conditions[index]
                .take()
                .expect("every conditional is lowered at its first action");
            let (then, otherwise) = if conditional.replaced_first().is_some() {
                // `instead`: the replaced sentence is the other branch.
                let then = effects.drain(then_from..then_to).collect::<Vec<_>>();
                let otherwise = effects.drain(start_from..then_from).collect::<Vec<_>>();
                (then, otherwise)
            } else {
                let otherwise = effects.drain(then_to..else_to).collect::<Vec<_>>();
                let then = effects.drain(then_from..then_to).collect::<Vec<_>>();
                (then, otherwise)
            };
            cursor.enter("lower.sentence-conditional.cast-choice-targets");
            if sentence_condition_depends_on_cast_choice(&conditional.condition) {
                let governed = decisions
                    .get_mut(decisions_from..decisions_to)
                    .unwrap_or(&mut []);
                gate_cast_choice_targets(
                    cursor,
                    spell_declarations,
                    &condition,
                    governed,
                    &then,
                    &otherwise,
                )?;
            }
            effects.insert(
                start_from,
                json!({
                    "kind": "conditional",
                    "condition": condition,
                    "then": then,
                    "else": otherwise,
                }),
            );
        }
        Ok(())
    }
}

/// Whether a lowered condition is decided by the choices made as the spell
/// was cast (CR 601.2b-f), which the casting declarations already hold when
/// its targets are chosen (CR 601.2c): kicker, alone or composed.
fn condition_decided_at_casting(condition: &Value) -> bool {
    match condition["kind"].as_str() {
        Some("wasKicked") => condition["spell"]["kind"].as_str() == Some("self"),
        Some("kickerCostWasPaid") => condition["cost"].is_string(),
        // A promised gift, a paid teamwork or escape cost: each is a
        // decision of the casting declaration (CR 601.2b-f).
        Some("castChoiceMade") => {
            condition["spell"]["kind"].as_str() == Some("self")
                && matches!(
                    condition["choice"].as_str(),
                    Some("gift" | "teamwork" | "escape")
                )
        }
        Some("not") => condition_decided_at_casting(&condition["operand"]),
        Some("and" | "or") => condition["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty() && operands.iter().all(condition_decided_at_casting)
        }),
        _ => false,
    }
}

/// Target declarations governed by a cast-choice conditional (CR 601.2c:
/// `If this spell was kicked, destroy another target land`). A target both
/// branches act on is declared whichever branch is taken; a target only one
/// branch acts on is declared only when the cast choices select that branch,
/// the declaration's `condition` gating it. Only a spell's own casting
/// declarations hold the choices; any other declaration, a condition not
/// decided at casting, or a governed declaration that is not a target
/// fails explicitly.
fn gate_cast_choice_targets(
    cursor: &mut OracleTokenCursor<'_>,
    spell_declarations: bool,
    condition: &Value,
    governed: &mut [Value],
    then: &[Value],
    otherwise: &[Value],
) -> Result<(), EffectSequenceLoweringFailure> {
    fn unsupported(cursor: &mut OracleTokenCursor<'_>) -> EffectSequenceLoweringFailure {
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("a cast-choice conditional branch without target declarations of its own"),
        )
    }
    for decision in governed.iter_mut() {
        let Some(id) = decision["id"].as_str().map(str::to_owned) else {
            return Err(unsupported(cursor));
        };
        let in_then = mentions_identifier(then, &id);
        let in_otherwise = mentions_identifier(otherwise, &id);
        if in_then && in_otherwise {
            continue;
        }
        if !(spell_declarations
            && condition_decided_at_casting(condition)
            && decision["kind"].as_str() == Some("chooseTargets")
            && in_then != in_otherwise)
        {
            return Err(unsupported(cursor));
        }
        let gate = if in_then {
            condition.clone()
        } else {
            negation(condition.clone())
        };
        decision["condition"] = match decision.get("condition") {
            Some(existing) => and(vec![existing.clone(), gate]),
            None => gate,
        };
    }
    Ok(())
}

/// Whether a lowered program names the declaration `id` anywhere.
fn mentions_identifier(effects: &[Value], id: &str) -> bool {
    fn walk(value: &Value, id: &str) -> bool {
        match value {
            Value::String(text) => text == id,
            Value::Array(items) => items.iter().any(|item| walk(item, id)),
            Value::Object(map) => map.values().any(|child| walk(child, id)),
            _ => false,
        }
    }
    effects.iter().any(|effect| walk(effect, id))
}

fn collect_action_result_references(value: &Value, found: &mut Vec<usize>) {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("boundObjects")
                && let Some(index) = map
                    .get("binding")
                    .and_then(Value::as_str)
                    .and_then(|binding| binding.strip_prefix("actionResult:"))
                    .and_then(|index| index.parse::<usize>().ok())
                && !found.contains(&index)
            {
                found.push(index);
            }
            map.values()
                .for_each(|child| collect_action_result_references(child, found));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_action_result_references(child, found)),
        _ => {}
    }
}

/// Installs the result binding each `OBJECTS VERBed this way` count reads
/// (CR 608.2c). The producing action is the one the sequence parent
/// resolved; its lowering must be exactly one top-level runtime action whose
/// `bind` records the affected objects. Anything else fails explicitly.
fn install_action_result_bindings(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut referenced = Vec::new();
    effects
        .iter()
        .for_each(|effect| collect_action_result_references(effect, &mut referenced));
    for effect_index in referenced {
        cursor.enter("lower.action-result.binding");
        let verb = ActionResultVerb::ALL
            .into_iter()
            .find(|verb| {
                sequence
                    .effects
                    .get(effect_index)
                    .is_some_and(|effect| action_result_verb_produced_by(*verb, effect))
            })
            .ok_or_else(|| cursor.fail("an earlier action that produces the restated objects"))?;
        let (Some(&(from, _)), Some(&(to, _))) = (
            action_starts.get(effect_index),
            action_starts.get(effect_index + 1),
        ) else {
            return Err(cursor.fail("a lowered producing action").into());
        };
        let mut producers = effects[from..to]
            .iter_mut()
            .filter(|effect| verb.records_result(effect));
        let (Some(producer), None) = (producers.next(), producers.next()) else {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("one top-level runtime action recording the restated objects"),
            ));
        };
        // The producer's stable object set: an existing binding, a revealed
        // set the program already bound, or a binding installed now.
        let objects = action_result_objects(producer, effect_index)
            .map_err(|expected| cursor.fail(expected))?;
        let binding = action_result_binding(effect_index);
        if objects != json!({ "kind": "boundObjects", "binding": binding }) {
            effects
                .iter_mut()
                .for_each(|effect| rewrite_bound_objects(effect, &binding, &objects));
        }
    }
    Ok(())
}

/// The lowered program so far, for a condition about the instruction just
/// before it (`If you can't`, CR 609.3).
struct PreviousActionResult<'a> {
    action_starts: &'a [(usize, usize)],
    effects: &'a mut Vec<Value>,
}

impl PreviousActionResult<'_> {
    /// Runtime actions whose `bind` records whether they were performed: the
    /// object they acted on, or nothing when they could not act.
    const RECORDING_KINDS: [&'static str; 2] = ["removeCounters", "sacrificePermanent"];

    /// Whether `effect` records whether it was performed in its `bind`: a
    /// recording kind, or the sacrifice of exactly one chosen permanent
    /// (`sacrifice a creature. If you can't, ...`, CR 701.21a), whose bound
    /// set is empty exactly when the player had nothing to sacrifice.
    fn records_performance(effect: &Value) -> bool {
        effect["kind"]
            .as_str()
            .is_some_and(|kind| Self::RECORDING_KINDS.contains(&kind))
            || (effect["kind"].as_str() == Some("sacrificePermanents")
                && (effect["count"].as_i64() == Some(1)
                    || effect["count"] == json!({ "kind": "integer", "value": 1 })))
    }

    /// Binds the result of the instruction before `reader`, which must be
    /// exactly one top-level recording action. `None` without an earlier
    /// instruction; the condition then fails explicitly.
    fn bind(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        reader: usize,
    ) -> Result<Option<String>, EffectSequenceLoweringFailure> {
        let Some(previous) = reader.checked_sub(1) else {
            return Ok(None);
        };
        cursor.enter("lower.sentence-condition.previous-action.producer");
        let Some(&(from, _)) = self.action_starts.get(previous) else {
            return Ok(None);
        };
        let producers = (from..self.effects.len())
            .filter(|position| Self::records_performance(&self.effects[*position]))
            .collect::<Vec<_>>();
        let [position] = producers.as_slice() else {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("one previous top-level action recording whether it was performed"),
            ));
        };
        let binding = action_result_binding(previous);
        match self.effects[*position].get("bind").and_then(Value::as_str) {
            None => self.effects[*position]["bind"] = json!(binding),
            Some(existing) => cursor.require(
                existing == binding,
                "a previous action without another result binding",
            )?,
        }
        Ok(Some(binding))
    }
}

/// Every pending action-result quantity `(reader, quantity)` under `value`.
fn collect_action_result_quantities(value: &Value, found: &mut Vec<(usize, String)>) {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some(ACTION_RESULT_QUANTITY)
                && let (Some(reader), Some(quantity)) = (
                    map.get("reader").and_then(Value::as_u64),
                    map.get("quantity").and_then(Value::as_str),
                )
            {
                let entry = (reader as usize, quantity.to_string());
                if !found.contains(&entry) {
                    found.push(entry);
                }
            }
            map.values()
                .for_each(|child| collect_action_result_quantities(child, found));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_action_result_quantities(child, found)),
        _ => {}
    }
}

/// Rewrites the pending references of one reader into its bound value.
fn bind_action_result_quantity(value: &mut Value, reader: usize, quantity: &str, bound: &Value) {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some(ACTION_RESULT_QUANTITY)
                && map.get("reader").and_then(Value::as_u64) == Some(reader as u64)
                && map.get("quantity").and_then(Value::as_str) == Some(quantity)
            {
                *value = bound.clone();
                return;
            }
            map.values_mut()
                .for_each(|child| bind_action_result_quantity(child, reader, quantity, bound));
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| bind_action_result_quantity(child, reader, quantity, bound)),
        _ => {}
    }
}

/// Binds each action-result quantity (`the total life lost this way`,
/// `that many`, CR 608.2c, 702.101a) to the nearest earlier action of the
/// program whose lowering records that quantity in one top-level runtime
/// action. An existing binding of the same field is reused. A reader without
/// a producer, or whose nearest producer records several candidates, fails.
fn install_action_result_quantities(
    cursor: &mut OracleTokenCursor<'_>,
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut pending = Vec::new();
    effects
        .iter()
        .for_each(|effect| collect_action_result_quantities(effect, &mut pending));
    for (reader, quantity) in pending {
        cursor.enter("lower.action-result.quantity");
        let kind = ActionResultQuantity::from_ir_name(&quantity)
            .ok_or_else(|| cursor.fail("a known action-result quantity"))?;
        // An anaphor (`that many`) names the immediately preceding action;
        // it never skips that action in favour of an older producer.
        let earliest = if kind.is_anaphor() {
            reader.saturating_sub(1)
        } else {
            0
        };
        let producer = (earliest..reader).rev().find_map(|index| {
            let (from, _) = *action_starts.get(index)?;
            let (to, _) = *action_starts.get(index + 1)?;
            let producers = (from..to)
                .filter_map(|position| {
                    let field = kind.produced_by(effects[position]["kind"].as_str()?)?;
                    // Only a number chosen as its own instruction is a quantity;
                    // the count decision of another action is not.
                    super::resolution_object_choice::number_choice_produces(&effects[position])
                        .then_some((position, field))
                })
                .collect::<Vec<_>>();
            (!producers.is_empty()).then_some((index, producers))
        });
        let Some((producer_index, producers)) = producer else {
            return Err(cursor
                .fail("an earlier action of this program that produces the restated quantity")
                .into());
        };
        let [(position, field)] = producers.as_slice() else {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("one top-level runtime action recording the restated quantity"),
            ));
        };
        let binding = match effects[*position]
            .get(field.ir_field())
            .and_then(Value::as_str)
        {
            Some(existing) => existing.to_owned(),
            None => {
                let binding = action_result_binding(producer_index);
                effects[*position][field.ir_field()] = json!(binding);
                binding
            }
        };
        let bound = field.reader(&binding);
        effects
            .iter_mut()
            .for_each(|effect| bind_action_result_quantity(effect, reader, &quantity, &bound));
    }
    Ok(())
}

/// Binds each action-result quantity a sentence condition reads (`If excess
/// damage was dealt this way`, CR 120.4a, 608.2c) to the nearest earlier
/// action of the program lowered so far that records it in one top-level
/// runtime action, as `install_action_result_quantities` does for the
/// actions' own arguments. The condition is lowered where its sentence
/// begins, so its producers precede it; none, or several candidates in the
/// nearest producer, fail explicitly.
fn bind_condition_action_result_quantities(
    cursor: &mut OracleTokenCursor<'_>,
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
    condition: &mut Value,
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut pending = Vec::new();
    collect_action_result_quantities(condition, &mut pending);
    for (reader, quantity) in pending {
        cursor.enter("lower.action-result.condition-quantity");
        let kind = ActionResultQuantity::from_ir_name(&quantity)
            .ok_or_else(|| cursor.fail("a known action-result quantity"))?;
        let earliest = if kind.is_anaphor() {
            reader.saturating_sub(1)
        } else {
            0
        };
        let producer = (earliest..reader).rev().find_map(|index| {
            let (from, _) = *action_starts.get(index)?;
            // The last lowered action ends with the program lowered so far.
            let to = action_starts
                .get(index + 1)
                .map_or(effects.len(), |(to, _)| *to);
            let producers = (from..to)
                .filter_map(|position| {
                    let field = kind.produced_by(effects[position]["kind"].as_str()?)?;
                    super::resolution_object_choice::number_choice_produces(&effects[position])
                        .then_some((position, field))
                })
                .collect::<Vec<_>>();
            (!producers.is_empty()).then_some((index, producers))
        });
        let Some((producer_index, producers)) = producer else {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor
                    .fail("an earlier action of this program that produces the restated quantity"),
            ));
        };
        let [(position, field)] = producers.as_slice() else {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("one top-level runtime action recording the restated quantity"),
            ));
        };
        let binding = match effects[*position]
            .get(field.ir_field())
            .and_then(Value::as_str)
        {
            Some(existing) => existing.to_owned(),
            None => {
                let binding = action_result_binding(producer_index);
                effects[*position][field.ir_field()] = json!(binding);
                binding
            }
        };
        bind_action_result_quantity(condition, reader, &quantity, &field.reader(&binding));
    }
    Ok(())
}

/// Binds each postposed `where X is EXPR` definition (CR 107.3). The value is
/// computed once, by a `bind` placed before the sentence's first action;
/// every X reference of that sentence and of the later sentences of the
/// program (`... deals X damage to each opponent, where X is .... You gain
/// X life.`, `If X is three or more, ...`) reads the binding, up to the next
/// definition. An X a target declaration reads is the same amount read when
/// the targets are chosen (CR 601.2c, 603.3d) and rechecked (CR 608.2b): the
/// declaration carries the amount itself (`where_x_declaration`). An
/// expression the shared numeric lowering cannot represent, or one with no
/// value when targets are chosen, fails explicitly instead of defaulting.
///
/// Runs before the conditional instructions are wrapped: each inserted
/// `bind` belongs to the first action of its sentence, so a conditional
/// governing that sentence contains it, and a later conditional sees it.
fn apply_where_x_bindings(
    cursor: &mut OracleTokenCursor<'_>,
    action_starts: &mut [(usize, usize)],
    decisions: &mut [Value],
    sequence: &EffectSequenceNode,
    target_bindings: &[(TargetBinding, String)],
    conditions: &mut [Option<Value>],
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let bindings = &sequence.where_x;
    let target_slots = sequence_numeric_target_slots(sequence, target_bindings);
    let counter_slots = where_x_declaration::countered_spell_bindings(sequence, target_bindings);
    let mut slots_before_counter = sequence_numeric_target_slots(sequence, target_bindings);
    slots_before_counter.extend(where_x_declaration::countered_spell_slots(
        sequence,
        target_bindings,
    ));
    let mut binds = Vec::new();
    for (ordinal, binding) in bindings.iter().enumerate() {
        // The definition holds from its sentence to the next definition.
        let scope_end = bindings
            .get(ordinal + 1)
            .map_or(sequence.effects.len(), |next| next.first_effect)
            .max(binding.end_effect);
        let (effect_from, decision_from) = action_starts[binding.first_effect];
        let (effect_to, decision_to) = action_starts[scope_end];
        let declares_x = decisions[decision_from..decision_to]
            .iter()
            .any(references_x_value);
        cursor.enter("lower.where-x.definition");
        // A property of a declared target reads the slot that declared it
        // (its last known information once it left, CR 608.2h).
        let plain = lower_numeric_expression(&binding.expression);
        let reads_target = plain.is_none() && binding.expression.requires_target_binding();
        // A countered spell leaves the stack (CR 701.6a): a definition that
        // reads nothing but declared targets is read before the counter,
        // while the spell is still there.
        let read_before_counter = reads_target
            .then(|| {
                where_x_declaration::target_read_position(
                    &binding.expression,
                    &counter_slots,
                    action_starts,
                )
            })
            .flatten();
        let read_slots = if read_before_counter.is_some() {
            &slots_before_counter[..]
        } else {
            &target_slots[..]
        };
        let value = plain
            .or_else(|| {
                reads_target
                    .then(|| {
                        lower_numeric_expression_for_targets(&binding.expression, read_slots).ok()
                    })
                    .flatten()
            })
            .ok_or_else(|| {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("a definition of X that the shared numeric lowering represents"),
                )
            })?;
        if declares_x {
            // A declaration cannot read a target it declares.
            cursor.enter("lower.where-x.declarations");
            if reads_target {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                    "a definition of X read when targets are chosen, not a property of a declared target",
                )));
            }
            where_x_declaration::bind_declaration_x(
                &mut decisions[decision_from..decision_to],
                &value,
            )
            .map_err(|expected| {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
            })?;
        }
        let id = if ordinal == 0 {
            WHERE_X_BINDING_ID.to_owned()
        } else {
            format!("{WHERE_X_BINDING_ID}{}", ordinal + 1)
        };
        cursor.enter("lower.where-x.scope");
        let mut uses = 0;
        for effect in &mut effects[effect_from..effect_to] {
            uses += replace_x_value(effect, &id);
        }
        // `If X is three or more, ...` after the definition reads it. A
        // conditional governing the defining sentence itself is tested
        // before the binding inside its branch exists.
        for (conditional, condition) in sequence.conditionals.iter().zip(conditions.iter_mut()) {
            let Some(condition) = condition.as_mut() else {
                continue;
            };
            if !(binding.first_effect..scope_end).contains(&conditional.start())
                || !references_x_value(condition)
            {
                continue;
            }
            cursor.enter("lower.sentence-conditional.where-x");
            if conditional.start() == binding.first_effect {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("a condition on X after the sentence that defines X"),
                ));
            }
            uses += replace_x_value(condition, &id);
        }
        cursor.enter("lower.where-x.scope");
        cursor.require(
            uses > 0 || declares_x,
            "an action or a target declaration in this sentence whose quantity is X",
        )?;
        if uses > 0 {
            let position =
                read_before_counter.map_or(effect_from, |position| position.min(effect_from));
            binds.push((
                position,
                json!({ "kind": "bind", "id": id, "value": value }),
            ));
        }
    }
    binds.sort_by_key(|(position, _)| *position);
    // Each bind belongs to the action that starts at its position: later
    // action starts move past it.
    for start in action_starts.iter_mut() {
        start.0 += binds
            .iter()
            .filter(|(position, _)| *position < start.0)
            .count();
    }
    for (position, bind) in binds.into_iter().rev() {
        effects.insert(position, bind);
    }
    Ok(())
}

/// Wraps the actions each `unless` frame guards (CR 118.12). A payment is
/// one resolution cost attempt by the named payer, who chooses; a cost that
/// cannot be paid is not paid, so the guarded actions happen. A condition
/// guards them with its negation, evaluated at resolution. Frames run last to
/// first so earlier action positions stay valid; later positions are shifted.
fn apply_unless_frames(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    frames: &[UnlessFrameNode],
    action_starts: &mut [(usize, usize)],
    decisions: &[Value],
    targets: &[(TargetBinding, String)],
    players: &[(PlayerTargetBinding, String)],
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut frames = frames.iter().collect::<Vec<_>>();
    frames.sort_by_key(|frame| std::cmp::Reverse(frame.first_effect));
    for frame in frames {
        let from = action_starts[frame.first_effect].0;
        let to = action_starts[frame.end_effect].0;
        cursor.enter("lower.unless.guarded-actions");
        cursor.require(from < to, "guarded actions that lower to executable steps")?;
        let guarded = effects.drain(from..to).collect::<Vec<_>>();
        let replacement = match &frame.clause {
            UnlessClauseNode::Payment {
                payer: UnlessPayerNode::QuantifiedPlayer,
                cost,
            } => lower_distributed_unless_payment(cursor, sequence, frame, cost)?,
            UnlessClauseNode::Payment { payer, cost } => {
                cursor.enter("lower.unless.payer");
                let player = match payer {
                    UnlessPayerNode::Controller => controller(),
                    UnlessPayerNode::ControllerOf(binding) => {
                        // With several targets one may become illegal alone
                        // (CR 608.2b) and leave the payment without a payer.
                        cursor.enter("lower.unless.payer.single-target");
                        if decisions
                            .iter()
                            .filter(|decision| decision["kind"] == "chooseTargets")
                            .count()
                            != 1
                        {
                            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                cursor.fail("the only declared target of the sequence as the payer's object"),
                            ));
                        }
                        let id = targets
                            .iter()
                            .find_map(|(actual, id)| (actual == binding).then_some(id))
                            .ok_or_else(|| {
                                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                    cursor.fail("a declared single target whose controller pays"),
                                )
                            })?;
                        json!({ "kind": "controllerOf", "object": chosen_target(id) })
                    }
                    UnlessPayerNode::BoundPlayer(binding) => {
                        let id = players
                            .iter()
                            .find_map(|(actual, id)| (actual == binding).then_some(id))
                            .ok_or_else(|| {
                                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                    cursor.fail("a declared player target who pays"),
                                )
                            })?;
                        chosen_target(id)
                    }
                    UnlessPayerNode::QuantifiedPlayer => {
                        unreachable!("the distributed payer is lowered above")
                    }
                };
                cursor.enter("lower.unless.cost");
                let cost = lower_unless_cost(cursor, cost)?;
                let id = format!("unlessPayment{}", frame.first_effect);
                vec![
                    json!({ "kind": "tryPayCost", "id": id, "player": player, "cost": cost }),
                    json!({
                        "kind": "conditional",
                        "condition": {
                            "kind": "costWasPaid",
                            "decisionId": id,
                            "value": false,
                            "required": true,
                        },
                        "then": guarded,
                        "else": [],
                    }),
                ]
            }
            UnlessClauseNode::Condition(condition) => vec![json!({
                "kind": "conditional",
                "condition": not(lower_condition(condition)),
                "then": guarded,
                "else": [],
            })],
        };
        let inserted = replacement.len();
        effects.splice(from..from, replacement);
        for start in action_starts.iter_mut() {
            if start.0 >= to {
                start.0 = start.0 - (to - from) + inserted;
            } else if start.0 > from {
                start.0 = from;
            }
        }
    }
    Ok(())
}

/// `each player loses N life unless they PAY` (CR 118.12, 101.4): the frame
/// distributes over the quantified population. In APNAP order each player
/// chooses whether to pay; a player who does not pay suffers the guarded
/// life action. The iteration binds the payer; the action is ordinary.
fn lower_distributed_unless_payment(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    frame: &UnlessFrameNode,
    cost: &ResolutionCostNode,
) -> Result<Vec<Value>, EffectSequenceLoweringFailure> {
    cursor.enter("lower.unless.quantified-payer");
    let node = match sequence.effects.get(frame.first_effect) {
        Some(LocalEffectNode::QuantifiedPlayerAction(node))
            if frame.end_effect == frame.first_effect + 1 =>
        {
            node
        }
        _ => {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("one guarded quantified player action"),
            ));
        }
    };
    let [QuantifiedPlayerAction::Life(life)] = node.actions.as_slice() else {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("one life action suffered by each player who does not pay"),
        ));
    };
    let amount = lower_numeric_expression(&life.amount).ok_or_else(|| {
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("a life amount the shared numeric lowering represents"),
        )
    })?;
    cursor.enter("lower.unless.cost");
    let cost = lower_unless_cost(cursor, cost)?;
    let player_id = format!("unlessPayer{}", frame.first_effect);
    let payment_id = format!("unlessPayment{}", frame.first_effect);
    let player = json!({ "kind": "boundValue", "id": player_id });
    Ok(vec![json!({
        "kind": "forEachPlayer",
        "players": lower_quantified_population(node),
        "bind": player_id,
        "effects": [
            { "kind": "tryPayCost", "id": payment_id, "player": player, "cost": cost },
            {
                "kind": "conditional",
                "condition": {
                    "kind": "costWasPaid",
                    "decisionId": payment_id,
                    "value": false,
                    "required": true,
                },
                "then": [{
                    "kind": match life.action {
                        TargetPlayerLifeAction::Gain => "gainLife",
                        TargetPlayerLifeAction::Lose => "loseLife",
                    },
                    "player": player,
                    "amount": amount,
                }],
                "else": [],
            },
        ],
    })])
}

/// The resolution cost program of an `unless` payment. A variable mana
/// payment is a typed generic amount (bound by the spell's X or a postposed
/// `where X`); fixed mana must be bounded whole-mana symbols.
fn lower_unless_cost(
    cursor: &mut OracleTokenCursor<'_>,
    cost: &ResolutionCostNode,
) -> Result<Value, EffectSequenceLoweringFailure> {
    if let ResolutionCostNode::Payment(ResourcePaymentNode::Mana(mana)) = cost
        && mana.contains_variable_x()
    {
        return mana
            .generic_payment_expression()
            .and_then(|expression| lower_numeric_expression(&expression))
            .map(|amount| json!({ "kind": "payMana", "genericAmount": amount }))
            .ok_or_else(|| {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                    "a typed generic/X mana amount; colored variable payments need a wider cost program",
                ))
            });
    }
    let parts = match cost {
        ResolutionCostNode::Sequence(parts) => parts.as_slice(),
        single => std::slice::from_ref(single),
    };
    for part in parts {
        if let ResolutionCostNode::Payment(ResourcePaymentNode::Mana(mana)) = part {
            cursor.enter("lower.unless.cost.fixed-mana");
            cursor.require(
                !mana.contains_variable_x(),
                "a fixed mana part in an `unless` cost list",
            )?;
            if let Err(error) = mana.fixed_runtime_mana_value() {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail(error.expected()),
                ));
            }
        }
    }
    Ok(lower_resolution_cost(cost))
}

fn references_x_value(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            (object.get("kind").and_then(Value::as_str) == Some("decisionResult")
                && object.get("decisionId").and_then(Value::as_str) == Some("xValue"))
                || object.values().any(references_x_value)
        }
        Value::Array(items) => items.iter().any(references_x_value),
        _ => false,
    }
}

/// Replaces each `xValue` decision reference by the sentence's binding and
/// returns how many references were replaced.
fn replace_x_value(value: &mut Value, binding_id: &str) -> usize {
    if references_x_value_here(value) {
        *value = json!({ "kind": "boundValue", "id": binding_id });
        return 1;
    }
    match value {
        Value::Object(object) => object
            .values_mut()
            .map(|child| replace_x_value(child, binding_id))
            .sum(),
        Value::Array(items) => items
            .iter_mut()
            .map(|child| replace_x_value(child, binding_id))
            .sum(),
        _ => 0,
    }
}

fn references_x_value_here(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some("decisionResult")
        && value.get("decisionId").and_then(Value::as_str) == Some("xValue")
}

#[cfg(test)]
#[path = "effect_sequence_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "preposed_duration_tests.rs"]
mod preposed_duration_tests;

#[path = "delayed_frame_lowering.rs"]
mod delayed_frame_lowering;

#[path = "program_iteration_lowering.rs"]
mod program_iteration_lowering;
pub(in crate::oracle::canonical) use program_iteration_lowering::wrap_repeated_processes;

#[path = "effect_alternatives.rs"]
mod effect_alternatives;
use delayed_frame_lowering::apply_delayed_frames;

#[path = "optional_outcome.rs"]
mod optional_outcome;
pub(in crate::oracle::canonical) use optional_outcome::OPTIONAL_OUTCOME_CONDITION;
use optional_outcome::resolve_optional_outcomes;
pub(in crate::oracle::canonical) use optional_outcome::resolve_program_optional_outcomes;

#[cfg(test)]
#[path = "where_x_tests.rs"]
mod where_x_tests;

#[path = "where_x_declaration.rs"]
mod where_x_declaration;

#[cfg(test)]
#[path = "unless_frame_tests.rs"]
mod unless_frame_tests;

fn operand_target_base_ids(operand: &PermanentOperandNode) -> Vec<&'static str> {
    match operand {
        PermanentOperandNode::Targets(targets) => targets
            .slots
            .iter()
            .filter_map(|slot| lower_target_selector_candidates(&slot.selector).map(|(id, _)| id))
            .collect(),
        _ => Vec::new(),
    }
}

fn lower_fixed_damage_recipient(
    recipient: &DamageRecipientNode,
    context: EffectSequenceLoweringContext,
    object_bindings: &[(TargetBinding, String)],
    player_bindings: &[(PlayerTargetBinding, String)],
) -> Result<Value, TargetlessLoweringError> {
    // A reference the binding environment resolved reads the program's
    // declared slots; every other fixed group needs none.
    match recipient {
        DamageRecipientNode::BoundPlayer(reference) => {
            return bound_player_reference_value(*reference, object_bindings, player_bindings)
                .ok_or(TargetlessLoweringError::RequiresTarget);
        }
        DamageRecipientNode::BoundObject(reference) => {
            return bound_permanent_reference_value(*reference, object_bindings)
                .ok_or(TargetlessLoweringError::RequiresTarget);
        }
        DamageRecipientNode::AttackedRecipient(attacker) => {
            return lower_attacked_recipient(*attacker, object_bindings);
        }
        _ => {}
    }
    let declarations = match context {
        EffectSequenceLoweringContext::NoExternalX => TargetlessDeclarationSupport::None,
        EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
            TargetlessDeclarationSupport::XValue
        }
    };
    lower_targetless_damage_recipient(recipient, declarations)
}

/// Build numeric context from the very slot that produced the parent's ID.
/// No selector is reconstructed from text, and absent/plural bindings stay
/// rejected instead of borrowing another action's most recent target.
fn sequence_numeric_target_slots(
    sequence: &EffectSequenceNode,
    bindings: &[(TargetBinding, String)],
) -> Vec<NumericTargetSlot> {
    bindings
        .iter()
        .filter_map(|(binding, id)| {
            let (targets, after_zone_change) = match sequence.effects.get(binding.effect_index)? {
                LocalEffectNode::ModifyStats(_)
                | LocalEffectNode::BecomeTypes(_)
                | LocalEffectNode::TemporaryKeywordGrant(_)
                | LocalEffectNode::TapPermanents(_)
                | LocalEffectNode::PermanentKeywordAction(_)
                | LocalEffectNode::ControlChange(_)
                | LocalEffectNode::PutCounters(_)
                | LocalEffectNode::MoveCounters(_) => (
                    ordinary_action_targets(sequence.effects.get(binding.effect_index)?)?,
                    false,
                ),
                LocalEffectNode::ZoneChange(effect) => (&effect.target, true),
                LocalEffectNode::DealDamage(DamageEffectNode {
                    source: PermanentOperandNode::Targets(targets),
                    ..
                }) => (targets, false),
                // `This creature deals damage ... to target creature. That
                // creature deals damage equal to its power ...`: a recipient
                // slot declares the object when the source declares none.
                LocalEffectNode::DealDamage(DamageEffectNode {
                    source,
                    recipient: DamageRecipientNode::Targets(targets),
                    ..
                }) if !matches!(source, PermanentOperandNode::Targets(_)) => (targets, false),
                // The shield's one declared party and the second fighter of
                // a fight whose first fighter declares none: the slots the
                // lowering binds for a later anaphor.
                LocalEffectNode::PreventDamage(prevent) => {
                    (prevention_declared_party_index(prevent)?.1, false)
                }
                LocalEffectNode::Fight(FightEffectNode {
                    first,
                    second: PermanentOperandNode::Targets(targets),
                }) if !matches!(first, PermanentOperandNode::Targets(_)) => (targets, false),
                // `choose target ...`, or the declared subject of a
                // distributive predicate (`TARGETS each deal damage equal
                // to their power`): the declaration names its own slots.
                LocalEffectNode::Choose(ChooseInstructionNode {
                    domain: ChoiceDomainNode::PermanentTargets(targets),
                    ..
                }) => (targets, false),
                _ => return None,
            };
            let slot = targets.slots.get(binding.slot_index)?;
            // A plural slot registers only its set binding: its singular
            // binding exists only while an iteration frame binds the slot to
            // one member at a time (CR 608.2c), and that member exists.
            let plural = slot
                .count
                .as_ref()
                .map_or(slot.cardinality.maximum > 1, TargetCount::is_plural);
            if !plural && (slot.cardinality.minimum != 1 || slot.cardinality.maximum != 1) {
                return None;
            }
            Some(NumericTargetSlot {
                binding: *binding,
                target_id: id.clone(),
                selector: slot.selector.clone(),
                requires_post_zone_change_lki: after_zone_change,
            })
        })
        .collect()
}

/// The optional singular slot (`up to one other target creature`) a
/// distributive subject declared, when it is this damage's own source: the
/// numeric context of that one member.
fn optional_declared_source(
    sequence: &EffectSequenceNode,
    damage: &DamageEffectNode,
    bindings: &[(TargetBinding, String)],
) -> Option<NumericTargetSlot> {
    let PermanentOperandNode::BoundReference(PermanentEffectReference::Target(binding)) =
        &damage.source
    else {
        return None;
    };
    let LocalEffectNode::Choose(ChooseInstructionNode {
        domain: ChoiceDomainNode::PermanentTargets(targets),
        ..
    }) = sequence.effects.get(binding.effect_index)?
    else {
        return None;
    };
    let slot = targets.slots.get(binding.slot_index)?;
    if slot.count.is_some() || slot.cardinality.minimum != 0 || slot.cardinality.maximum != 1 {
        return None;
    }
    let id = bindings
        .iter()
        .find(|(candidate, _)| candidate == binding)
        .map(|(_, id)| id.clone())?;
    Some(NumericTargetSlot {
        binding: *binding,
        target_id: id,
        selector: slot.selector.clone(),
        requires_post_zone_change_lki: false,
    })
}

fn replace_json_value(value: &mut Value, from: &Value, to: &Value) {
    if value == from {
        *value = to.clone();
        return;
    }
    match value {
        Value::Object(map) => map
            .values_mut()
            .for_each(|child| replace_json_value(child, from, to)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| replace_json_value(child, from, to)),
        _ => {}
    }
}

fn lower_sequence_bound_amount(
    cursor: &mut OracleTokenCursor<'_>,
    amount: &NumericExpression,
    sequence: &EffectSequenceNode,
    targets: &[(TargetBinding, String)],
    context: EffectSequenceLoweringContext,
) -> Result<Value, EffectSequenceLoweringFailure> {
    let declarations = match context {
        EffectSequenceLoweringContext::NoExternalX => TargetlessDeclarationSupport::None,
        EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
            TargetlessDeclarationSupport::XValue
        }
    };
    match lower_targetless_numeric_expression(amount, declarations) {
        Ok(value) => return Ok(value),
        Err(error) if !amount.requires_target_binding() => {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail(error.expectation()),
            ));
        }
        Err(_) => {}
    }
    if amount.uses_x() && declarations == TargetlessDeclarationSupport::None {
        cursor.enter("sequence.target-numeric.variable-binding");
        return Err(EffectSequenceLoweringFailure::FailedSupport(
            cursor.fail("a declared numeric variable in the current face"),
        ));
    }
    let slots = sequence_numeric_target_slots(sequence, targets);
    lower_numeric_expression_for_targets(amount, &slots).map_err(|error| {
        let (state, expected) = match error {
            TargetNumericLoweringError::MissingBinding => (
                "sequence.target-numeric.binding",
                "the original singular target slot for this numeric reference",
            ),
            TargetNumericLoweringError::NominalMismatch => (
                "sequence.target-numeric.nominal",
                "a required nominal implied by every branch of the original target slot",
            ),
            TargetNumericLoweringError::UnprovedCharacteristicDomain => (
                "sequence.target-numeric.characteristic-domain",
                "an original target slot guaranteed to be Creature for a proved power or toughness read",
            ),
            TargetNumericLoweringError::PostZoneChangeLki => (
                "sequence.target-numeric.lki-support",
                "proved post-operation live-or-LKI properties of the original target incarnation",
            ),
            TargetNumericLoweringError::UnsupportedExpression => (
                "sequence.target-numeric.expression",
                "a supported contextual numeric expression",
            ),
        };
        cursor.enter(state);
        let failure = cursor.fail(expected);
        match error {
            TargetNumericLoweringError::MissingBinding => {
                EffectSequenceLoweringFailure::FailedInvariant(failure)
            }
            TargetNumericLoweringError::NominalMismatch => {
                EffectSequenceLoweringFailure::FailedSupport(failure)
            }
            TargetNumericLoweringError::UnprovedCharacteristicDomain => {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure)
            }
            TargetNumericLoweringError::PostZoneChangeLki
            | TargetNumericLoweringError::UnsupportedExpression => {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure)
            }
        }
    })
}

fn lower_sequence_bound_life(
    cursor: &mut OracleTokenCursor<'_>,
    kind: &str,
    player: &LocalPlayerReference,
    amount: &NumericExpression,
    sequence: &EffectSequenceNode,
    targets: &[(TargetBinding, String)],
    context: EffectSequenceLoweringContext,
) -> Result<Value, EffectSequenceLoweringFailure> {
    let player = match player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOfQualified(_) | LocalPlayerReference::Bound(_) => {
            cursor.enter("sequence.player-reference.qualified-controller-support");
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("a proved nominal/controller snapshot binding for this program parent"),
            ));
        }
        LocalPlayerReference::ControllerOf(binding) => {
            if !amount.requires_target_binding() {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail(TargetlessLoweringError::RequiresTarget.expectation()),
                ));
            }
            let slots = sequence_numeric_target_slots(sequence, targets);
            if slots
                .iter()
                .find(|slot| slot.binding == *binding)
                .is_some_and(|slot| slot.requires_post_zone_change_lki)
            {
                cursor.enter("sequence.target-numeric.lki-support");
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail("proved post-operation live-or-LKI properties of the original target incarnation")));
            }
            let id = targets
                .iter()
                .find_map(|(actual, id)| (actual == binding).then_some(id))
                .ok_or_else(|| {
                    cursor.fail("a declared target for the life recipient's controller")
                })?;
            json!({"kind":"controllerOf", "object":chosen_target(id)})
        }
    };
    let amount = lower_sequence_bound_amount(cursor, amount, sequence, targets, context)?;
    Ok(json!({"kind":kind, "player":player, "amount":amount}))
}

#[cfg(test)]
#[path = "target_numeric_sequence_tests.rs"]
mod target_numeric_sequence_tests;

/// One tapped-state change. A plural target reference uses the runtime's
/// collection form for untapping; tapping accepts both reference arities.
/// CR 701.15a: `regenerate OBJECT` creates one regeneration shield per object
/// for this turn; a collection reference uses the plural primitive.
/// The binding of the tokens a creation action created (`they`).
pub(in crate::oracle::canonical) const CREATED_TOKENS_BINDING: &str = "createdTokens";

fn created_tokens_are_next_antecedent(sequence: &EffectSequenceNode, effect_index: usize) -> bool {
    let reference = PermanentOperandNode::BoundReference(PermanentEffectReference::Observed(
        TypedObservedEventObject::CreatedTokens,
    ));
    let Some(next) = sequence.effects.get(effect_index + 1) else {
        return false;
    };
    // `Sacrifice it at the beginning of the next end step`: a zone action
    // on the one created token.
    if let LocalEffectNode::ReferencedPermanentAction(node) = next {
        return node.object
            == PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens);
    }
    // `Exile them at the beginning of the next end step`: a card action
    // whose pronoun names the created tokens (CR 608.2c).
    if let LocalEffectNode::CardAction(
        CardActionNode::Move { cards, .. }
        | CardActionNode::Reveal(cards)
        | CardActionNode::Copy { cards, .. }
        | CardActionNode::Permit(CardPermissionNode { cards, .. }),
    ) = next
    {
        return matches!(
            cards,
            CardOperandNode::ObjectAntecedent {
                object: PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens),
                ..
            }
        );
    }
    let operands: Vec<&PermanentOperandNode> = match next {
        LocalEffectNode::TemporaryKeywordGrant(node) => vec![&node.objects],
        LocalEffectNode::ModifyStats(node) => vec![&node.targets],
        LocalEffectNode::PutCounters(node) => vec![&node.permanent],
        LocalEffectNode::TapPermanents(node) => vec![&node.targets],
        LocalEffectNode::UntapRestriction(node) => vec![&node.objects],
        LocalEffectNode::RuleChange(node) => node.objects().into_iter().collect(),
        LocalEffectNode::PermanentKeywordAction(node) => vec![&node.targets],
        // `create a token, then attach this to it` (CR 702.92a, 702.163a):
        // the attach reads the created token as its recipient or attachment.
        LocalEffectNode::Attach(node) => vec![&node.attachment, &node.recipient],
        _ => vec![],
    };
    operands.iter().any(|operand| **operand == reference)
}

/// True when the action after a draw names the drawn cards by anaphora
/// (`... and reveal them`, `you may cast one of them ...`).
/// The next action reads an observed card collection (`from among them`,
/// `that card`), directly or as the first action of an optional sequence.
fn milled_cards_are_next_antecedent(sequence: &EffectSequenceNode, effect_index: usize) -> bool {
    fn reads(effect: &LocalEffectNode) -> bool {
        match effect {
            LocalEffectNode::CardAction(
                CardActionNode::Move { cards, .. }
                | CardActionNode::Reveal(cards)
                | CardActionNode::Permit(CardPermissionNode { cards, .. }),
            ) => matches!(
                cards,
                CardOperandNode::Reference(_)
                    | CardOperandNode::ObjectAntecedent { .. }
                    | CardOperandNode::Selection(_)
            ),
            LocalEffectNode::CardAction(CardActionNode::Choose(_)) => true,
            LocalEffectNode::OptionalSequence(inner) => inner.effects.first().is_some_and(reads),
            _ => false,
        }
    }
    sequence.effects.get(effect_index + 1).is_some_and(reads)
}

/// Puts the performer of one action on every primitive the action produced
/// (`effects[start..]`). A population is a player loop whose body is the
/// action performed by each player in turn (CR 101.4, 608.2c).
fn apply_performer(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut Vec<Value>,
    pending: Option<(usize, usize, PerformerNode)>,
    target_bindings: &[(TargetBinding, String)],
    declared_player_targets: &[(PlayerTargetBinding, String)],
) -> Result<(), EffectSequenceLoweringFailure> {
    let Some((start, effect_index, performer)) = pending else {
        return Ok(());
    };
    cursor.enter("lower.performer");
    cursor.require(
        effects.len() > start,
        "an action that produces at least one primitive for its performer",
    )?;
    // A population derived from a previous action's performance (`each
    // player who does`) is owned by the performer lowering module.
    if let Some(result) =
        super::lower_derived_performer(cursor, effects, start, effect_index, &performer)
    {
        return result.map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit);
    }
    let player = match &performer {
        PerformerNode::Player(PlayerEffectSubject::EventPlayer) => {
            json!({"kind": "triggeringPlayer"})
        }
        PerformerNode::Player(PlayerEffectSubject::ObjectRelation { relation, object }) => json!({
            "kind": match relation {
                ObjectPlayerRelation::Controller => "controllerOf",
                ObjectPlayerRelation::Owner => "ownerOf",
            },
            "object": lower_bound_permanent_reference(cursor, *object, target_bindings)?,
        }),
        PerformerNode::Player(PlayerEffectSubject::Bound(binding)) => {
            let id = declared_player_targets
                .iter()
                .find_map(|(key, id)| (key.effect_index == binding.0).then_some(id.clone()))
                .ok_or_else(|| cursor.fail("an earlier declared player for the performer"))?;
            chosen_target(&id)
        }
        // The declaration was lowered where the action began.
        PerformerNode::Player(PlayerEffectSubject::DeclaredTarget { binding, .. }) => {
            let id = declared_player_targets
                .iter()
                .find_map(|(key, id)| (key.effect_index == binding.0).then_some(id.clone()))
                .ok_or_else(|| cursor.fail("the declared target player of the performer"))?;
            chosen_target(&id)
        }
        PerformerNode::Player(PlayerEffectSubject::Controller) => {
            return Err(EffectSequenceLoweringFailure::FailedInvariant(
                cursor.fail("a performer other than the controller"),
            ));
        }
        // `the player to your left|right` (CR 101.4).
        PerformerNode::SeatNeighbor(direction) => lower_seat_neighbor(*direction),
        // CR 608.2d: the controller chooses one player of the population on
        // resolution; every primitive of the action (and of the verbs that
        // continue it) names the same choice. The choice is the shared
        // resolution-time `choosePlayers` step (the binding environment's
        // `choose an opponent`), made once per binding where the action
        // begins; the performer is the player it binds.
        PerformerNode::Chosen { players, binding } => {
            let id = format!("chosenPerformer{}", binding.0);
            let already_chosen = effects[..start].iter().any(|effect| {
                effect["kind"] == "choosePlayers" && effect["bindAs"].as_str() == Some(id.as_str())
            });
            let player = json!({"kind": "boundValue", "id": id});
            for effect in &mut effects[start..] {
                effect["performer"] = player.clone();
            }
            if !already_chosen {
                effects.insert(
                    start,
                    json!({
                        "kind": "choosePlayers",
                        "id": id,
                        "player": controller(),
                        "candidates": lower_player_set_reference(*players),
                        "minimum": integer(1),
                        "maximum": integer(1),
                        "bindAs": id,
                    }),
                );
            }
            return Ok(());
        }
        // `two target players each VERB`: every declared player performs the
        // action in turn order (CR 115.1, 608.2c).
        PerformerNode::DeclaredPlayers { binding, .. } | PerformerNode::BoundPlayers(binding) => {
            let id = declared_player_targets
                .iter()
                .find_map(|(key, id)| (key.effect_index == binding.0).then_some(id.clone()))
                .ok_or_else(|| cursor.fail("the declared target players of the performer"))?;
            let bind = format!("performer{effect_index}");
            let body = effects
                .drain(start..)
                .map(|mut effect| {
                    effect["performer"] = json!({"kind": "boundValue", "id": bind});
                    effect
                })
                .collect::<Vec<_>>();
            let collect = super::resolution_object_choice::player_loop_collection(&body);
            let mut lowered = json!({
                "kind": "forEachPlayer",
                "bind": bind,
                "players": {"kind": "chosenTargets", "id": id},
                "effects": body,
            });
            if let Some(collect) = collect {
                lowered["collect"] = collect;
            }
            effects.push(lowered);
            return Ok(());
        }
        // `For each OBJECT VERBed this way, its controller ACTION` (CR
        // 608.2c, 608.2h): one pass per object the earlier action affected,
        // each performed by that object's player (last known information).
        PerformerNode::EachAffectedObject {
            relation,
            effect_index: producer,
            filter,
        } => {
            let bind = format!("performedObject{effect_index}");
            let bound = json!({
                "kind": "boundObjects",
                "binding": action_result_binding(*producer),
            });
            let objects = match filter {
                ActionResultFilter::All => bound,
                ActionResultFilter::Cards(predicate) => json!({
                    "kind": "filterObjects",
                    "objects": bound,
                    "where": lower_card_object_predicate(predicate),
                }),
            };
            let player = json!({
                "kind": match relation {
                    ObjectPlayerRelation::Controller => "controllerOf",
                    ObjectPlayerRelation::Owner => "ownerOf",
                },
                "object": {"kind": "boundValue", "id": bind},
            });
            let body = effects
                .drain(start..)
                .map(|mut effect| {
                    effect["performer"] = player.clone();
                    effect
                })
                .collect::<Vec<_>>();
            effects.push(json!({
                "kind": "forEachObject",
                "objects": objects,
                "bind": bind,
                "effects": body,
            }));
            return Ok(());
        }
        PerformerNode::Population { .. } | PerformerNode::FilteredPopulation(_) => {
            let bind = format!("performer{effect_index}");
            let players = match &performer {
                // `each player who ...`, `each player other than its
                // controller` (CR 102.1-102.3): the shared player-set filter.
                PerformerNode::FilteredPopulation(query) => {
                    let except = match &query.except {
                        None => None,
                        Some(subject) => Some(lower_bound_player_subject(
                            cursor,
                            subject,
                            target_bindings,
                            declared_player_targets,
                        )?),
                    };
                    lower_player_population_query(query, except).map_err(|expected| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            cursor.fail(expected),
                        )
                    })?
                }
                // CR 102.3: `each other player` is every player but the
                // controller, teammates included, not the opponents set.
                PerformerNode::Population {
                    excluding_controller: true,
                    ..
                } => json!({"kind": "playersExcept", "player": controller()}),
                PerformerNode::Population { players, .. } => lower_player_set_reference(*players),
                _ => {
                    return Err(EffectSequenceLoweringFailure::FailedInvariant(
                        cursor.fail("a player population performer"),
                    ));
                }
            };
            // A restriction is read once, when its instruction begins: a
            // later verb the same restricted subject continues (`each
            // opponent who has seven or more cards in hand discards a card,
            // then draws a card`) would read it again after the first
            // action changed the game, so it is refused explicitly.
            if players["kind"] == "filterPlayers"
                && effects[..start]
                    .iter()
                    .any(|effect| effect["kind"] == "forEachPlayer" && effect["players"] == players)
            {
                cursor.enter("lower.performer.restricted-population.continued");
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("one action per restricted player population"),
                ));
            }
            let body = effects
                .drain(start..)
                .map(|mut effect| {
                    effect["performer"] = json!({"kind": "boundValue", "id": bind});
                    effect
                })
                .collect::<Vec<_>>();
            // Each player's chosen permanents are republished after the
            // loop: a later `those creatures` / `the rest` reads all of them.
            let collect = super::resolution_object_choice::player_loop_collection(&body);
            let mut lowered = json!({
                "kind": "forEachPlayer",
                "bind": bind,
                "players": players,
                "effects": body,
            });
            if let Some(collect) = collect {
                lowered["collect"] = collect;
            }
            effects.push(lowered);
            return Ok(());
        }
    };
    for effect in &mut effects[start..] {
        effect["performer"] = player.clone();
    }
    Ok(())
}

/// A player the program already binds (`that player`, `its controller`,
/// the triggering event's player), as one singular runtime reference: the
/// player excluded from a population (`each player other than its
/// controller`) or the holder of searched zones (`its controller's library`).
fn lower_bound_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
    subject: &PlayerEffectSubject,
    target_bindings: &[(TargetBinding, String)],
    declared_player_targets: &[(PlayerTargetBinding, String)],
) -> Result<Value, EffectSequenceLoweringFailure> {
    cursor.enter("lower.bound-player");
    match subject {
        PlayerEffectSubject::EventPlayer => Ok(json!({"kind": "triggeringPlayer"})),
        PlayerEffectSubject::ObjectRelation { relation, object } => Ok(json!({
            "kind": match relation {
                ObjectPlayerRelation::Controller => "controllerOf",
                ObjectPlayerRelation::Owner => "ownerOf",
            },
            "object": lower_bound_permanent_reference(cursor, *object, target_bindings)?,
        })),
        PlayerEffectSubject::Bound(binding) => declared_player_targets
            .iter()
            .find_map(|(key, id)| (key.effect_index == binding.0).then(|| chosen_target(id)))
            .ok_or_else(|| {
                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("an earlier declared player for the bound player"),
                )
            }),
        PlayerEffectSubject::Controller | PlayerEffectSubject::DeclaredTarget { .. } => {
            Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("a player this program already binds"),
            ))
        }
    }
}

/// The player targets that query possessors (`target opponent controls`)
/// of the actions lowered so far declare, resolved by the shared pass.
fn declare_query_possessor_targets(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut [Value],
    decisions: &mut Vec<Value>,
    declared_player_targets: &mut Vec<(PlayerTargetBinding, String)>,
) -> Result<(), EffectSequenceLoweringFailure> {
    match super::declare_embedded_player_targets(effects, decisions, declared_player_targets) {
        Ok(()) => Ok(()),
        Err(expected) => {
            cursor.enter("lower.query-possessor.declared-player");
            Err(EffectSequenceLoweringFailure::FailedSupport(
                cursor.fail(expected),
            ))
        }
    }
}

/// A declared performer (`target opponent VERBs`, CR 115.1, 601.2c) is one
/// player target declared where its action begins, under the shared
/// player-target ordinal; later `that player` reuses its id.
fn declare_performer_target(
    cursor: &mut OracleTokenCursor<'_>,
    performer: &PerformerNode,
    player_target_bindings: &[PlayerTargetBinding],
    decisions: &mut Vec<Value>,
    declared_player_targets: &mut Vec<(PlayerTargetBinding, String)>,
) -> Result<(), EffectSequenceLoweringFailure> {
    if let PerformerNode::DeclaredPlayers { target, binding } = performer {
        cursor.enter("lower.performer.declared-players");
        return declare_player_targets(
            cursor,
            target,
            PlayerTargetBinding {
                effect_index: binding.0,
            },
            player_target_bindings,
            decisions,
            declared_player_targets,
        );
    }
    let PerformerNode::Player(PlayerEffectSubject::DeclaredTarget { target, binding }) = performer
    else {
        return Ok(());
    };
    cursor.enter("lower.performer.declared-target");
    declare_player_target(
        cursor,
        target,
        PlayerTargetBinding {
            effect_index: binding.0,
        },
        player_target_bindings,
        decisions,
        declared_player_targets,
    )
    .map(|_| ())
}

/// One player slot declared by an action (a performer, a searched zone's
/// holder): its target decision under the shared player-target ordinal,
/// once per binding. Returns the declaration id.
fn declare_player_target(
    cursor: &mut OracleTokenCursor<'_>,
    target: &TargetListNode,
    key: PlayerTargetBinding,
    player_target_bindings: &[PlayerTargetBinding],
    decisions: &mut Vec<Value>,
    declared_player_targets: &mut Vec<(PlayerTargetBinding, String)>,
) -> Result<String, EffectSequenceLoweringFailure> {
    if let Some((_, id)) = declared_player_targets
        .iter()
        .find(|(declared, _)| *declared == key)
    {
        return Ok(id.clone());
    }
    let [slot] = target.slots.as_slice() else {
        return Err(EffectSequenceLoweringFailure::FailedInvariant(
            cursor.fail("one declared player target slot"),
        ));
    };
    let (_, candidates) = lower_target_selector_candidates(&slot.selector)
        .ok_or_else(|| cursor.fail("a supported player target selector"))?;
    let ordinal = player_target_bindings
        .iter()
        .position(|value| *value == key)
        .ok_or_else(|| cursor.fail("a declared player-target binding"))?;
    let target_id = player_target_id_for_ordinal(ordinal, player_target_bindings.len());
    decisions.push(target_decision(&target_id, candidates, 1, 1));
    declared_player_targets.push((key, target_id.clone()));
    Ok(target_id)
}

/// Several players declared by one target decision of a performer (`two
/// target players each`, `any number of target opponents each`): the
/// decision's bounds are the slot's literal cardinality or its typed count,
/// under the shared player-target ordinal (CR 115.1, 601.2c).
fn declare_player_targets(
    cursor: &mut OracleTokenCursor<'_>,
    target: &TargetListNode,
    key: PlayerTargetBinding,
    player_target_bindings: &[PlayerTargetBinding],
    decisions: &mut Vec<Value>,
    declared_player_targets: &mut Vec<(PlayerTargetBinding, String)>,
) -> Result<(), EffectSequenceLoweringFailure> {
    if declared_player_targets
        .iter()
        .any(|(declared, _)| *declared == key)
    {
        return Ok(());
    }
    let [slot] = target.slots.as_slice() else {
        return Err(EffectSequenceLoweringFailure::FailedInvariant(
            cursor.fail("one declared player target slot"),
        ));
    };
    let (_, candidates) = lower_target_selector_candidates(&slot.selector)
        .ok_or_else(|| cursor.fail("a supported player target selector"))?;
    let ordinal = player_target_bindings
        .iter()
        .position(|value| *value == key)
        .ok_or_else(|| cursor.fail("a declared player-target binding"))?;
    let target_id = player_target_id_for_ordinal(ordinal, player_target_bindings.len());
    let mut decision = target_decision(
        &target_id,
        candidates,
        i64::from(slot.cardinality.minimum),
        i64::from(slot.cardinality.maximum),
    );
    if let Some(count) = &slot.count {
        cursor.require(
            apply_target_count_to_decision(&mut decision, count).is_some(),
            "a declared player count the target decision can bound",
        )?;
    }
    decisions.push(decision);
    declared_player_targets.push((key, target_id));
    Ok(())
}

/// `return this card from your graveyard to the battlefield`, then an action
/// reading the permanent it became (`It gains haste`, CR 400.7): the source's
/// move binds the moved-onto-battlefield result only when a later primitive
/// reads it, through the program's single allocation of that binding.
fn bind_returned_source_when_read(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut [Value],
    collections: &mut CardCollectionMemory,
) -> Result<(), EffectSequenceLoweringFailure> {
    fn reads(value: &Value) -> bool {
        match value {
            Value::Array(values) => values.iter().any(reads),
            Value::Object(object) => {
                object.get("binding").and_then(Value::as_str)
                    == Some(MOVED_ONTO_BATTLEFIELD_BINDING)
                    || object.values().any(reads)
            }
            _ => false,
        }
    }
    fn is_source_return(effect: &Value) -> bool {
        effect.get("kind").and_then(Value::as_str) == Some("moveCards")
            && effect["cards"] == self_ref()
            && effect["from"]["kind"].as_str() == Some("graveyard")
            && effect["to"]["kind"].as_str() == Some("battlefield")
    }
    if !effects.iter().any(reads)
        || effects.iter().any(|effect| {
            effect.get("bind").and_then(Value::as_str) == Some(MOVED_ONTO_BATTLEFIELD_BINDING)
        })
    {
        return Ok(());
    }
    let returns = effects
        .iter()
        .filter(|effect| is_source_return(effect) && effect.get("bind").is_none())
        .count();
    if returns == 0 {
        return Ok(());
    }
    cursor.enter("lower.move-source.battlefield-entry.binding");
    if returns != 1 || !collections.allocate_moved_onto_battlefield() {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("one battlefield entry whose permanent a later action reads"),
        ));
    }
    if let Some(effect) = effects
        .iter_mut()
        .find(|effect| is_source_return(effect) && effect.get("bind").is_none())
    {
        effect["bind"] = json!(MOVED_ONTO_BATTLEFIELD_BINDING);
    }
    Ok(())
}

/// `search your library for a basic land card, put it onto the
/// battlefield`, then an action reading the permanent it became (`That land
/// becomes ...`, CR 400.7, 701.23a): the search's own battlefield move binds
/// the moved-onto-battlefield result only when a later primitive reads it,
/// through the program's single allocation of that binding. The parser
/// publishes that antecedent only for a search finding at most one card
/// (`moved_onto_battlefield_selector`); any other unbound battlefield entry
/// in the same sequence makes the producer ambiguous and fails explicitly.
fn bind_battlefield_search_when_read(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    effects: &mut [Value],
    collections: &mut CardCollectionMemory,
) -> Result<(), EffectSequenceLoweringFailure> {
    fn reads(value: &Value) -> bool {
        match value {
            Value::Array(values) => values.iter().any(reads),
            Value::Object(object) => {
                object.get("binding").and_then(Value::as_str)
                    == Some(MOVED_ONTO_BATTLEFIELD_BINDING)
                    || object.values().any(reads)
            }
            _ => false,
        }
    }
    fn binds(value: &Value) -> bool {
        match value {
            Value::Array(values) => values.iter().any(binds),
            Value::Object(object) => {
                object.get("bind").and_then(Value::as_str) == Some(MOVED_ONTO_BATTLEFIELD_BINDING)
                    || object.values().any(binds)
            }
            _ => false,
        }
    }
    fn unbound_entry(value: &Value) -> bool {
        value.get("kind").and_then(Value::as_str) == Some("moveCards")
            && value["to"]["kind"].as_str() == Some("battlefield")
            && value.get("bind").is_none()
    }
    fn unbound_entries(value: &Value) -> usize {
        match value {
            Value::Array(values) => values.iter().map(unbound_entries).sum(),
            Value::Object(object) => {
                usize::from(unbound_entry(value))
                    + object.values().map(unbound_entries).sum::<usize>()
            }
            _ => 0,
        }
    }
    fn bind(value: &mut Value) -> bool {
        if unbound_entry(value) {
            value["bind"] = json!(MOVED_ONTO_BATTLEFIELD_BINDING);
            return true;
        }
        match value {
            Value::Array(values) => values.iter_mut().any(bind),
            Value::Object(object) => object.values_mut().any(bind),
            _ => false,
        }
    }
    let searches_onto_battlefield = sequence.effects.iter().any(|effect| {
        matches!(
            effect,
            LocalEffectNode::ControllerLibrarySearch(search)
                | LocalEffectNode::OptionalLibrarySearch(search)
                if matches!(search.destination, SearchDestination::Battlefield { .. })
        )
    });
    if !searches_onto_battlefield || !effects.iter().any(reads) || effects.iter().any(binds) {
        return Ok(());
    }
    cursor.enter("lower.library-search.battlefield-entry.binding");
    let entries = effects.iter().map(unbound_entries).sum::<usize>();
    if entries != 1 || !collections.allocate_moved_onto_battlefield() {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("one battlefield entry whose permanent a later action reads"),
        ));
    }
    let bound = effects.iter_mut().any(bind);
    cursor.require(bound, "the search's battlefield entry to bind")?;
    Ok(())
}

/// The deployed card's move binds the moved-onto-battlefield result only
/// when a later primitive reads it, through the program's single allocation
/// of that binding: a second producer fails explicitly.
fn bind_deployed_card_when_read(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut [Value],
    deployed_choices: &[String],
    collections: &mut CardCollectionMemory,
) -> Result<(), EffectSequenceLoweringFailure> {
    fn reads(value: &Value) -> bool {
        match value {
            Value::Array(values) => values.iter().any(reads),
            Value::Object(object) => {
                object.get("binding").and_then(Value::as_str)
                    == Some(MOVED_ONTO_BATTLEFIELD_BINDING)
                    || object.values().any(reads)
            }
            _ => false,
        }
    }
    fn bind(value: &mut Value, choice: &str) -> bool {
        match value {
            Value::Array(values) => values.iter_mut().any(|value| bind(value, choice)),
            Value::Object(object) => {
                let deployment = object.get("kind").and_then(Value::as_str) == Some("moveCards")
                    && object
                        .get("cards")
                        .and_then(|cards| cards.get("decisionId"))
                        .and_then(Value::as_str)
                        == Some(choice);
                if deployment {
                    object.insert("bind".to_string(), json!(MOVED_ONTO_BATTLEFIELD_BINDING));
                    return true;
                }
                object.values_mut().any(|value| bind(value, choice))
            }
            _ => false,
        }
    }
    if deployed_choices.is_empty() || !effects.iter().any(reads) {
        return Ok(());
    }
    cursor.enter("lower.optional-card-deployment.binding");
    let [choice] = deployed_choices else {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("one battlefield entry whose permanent a later action reads"),
        ));
    };
    if effects.iter().any(|effect| {
        effect.get("bind").and_then(Value::as_str) == Some(MOVED_ONTO_BATTLEFIELD_BINDING)
    }) || !collections.allocate_moved_onto_battlefield()
    {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("one battlefield entry whose permanent a later action reads"),
        ));
    }
    let bound = effects.iter_mut().any(|effect| bind(effect, choice));
    cursor.require(bound, "the deployment's move to bind")?;
    Ok(())
}

/// True when a permanent operand denotes objects on the battlefield as the
/// action resolves (CR 400.7, 603.10a), the condition of an indefinite
/// grant: declared permanent targets (one or several), collections, the
/// source, attached permanents, the card a move put onto the battlefield,
/// created tokens, an entering permanent and the combat counterpart. A
/// target declared by a card action (a card in a graveyard or exile) and an
/// event's permanent (which may be last known information in a dies or
/// leaves frame) are not.
fn operand_on_battlefield(operand: &PermanentOperandNode, sequence: &EffectSequenceNode) -> bool {
    match operand {
        PermanentOperandNode::Targets(_)
        | PermanentOperandNode::Collection { .. }
        | PermanentOperandNode::Reference(_)
        // Chosen among battlefield permanents as the action resolves.
        | PermanentOperandNode::Chosen(_) => true,
        PermanentOperandNode::BoundReference(reference) => match reference {
            PermanentEffectReference::Source(_) | PermanentEffectReference::Attached(_) => true,
            PermanentEffectReference::Target(binding)
            | PermanentEffectReference::TargetSet(binding) => sequence
                .effects
                .get(binding.effect_index)
                .and_then(ordinary_action_targets)
                .is_some_and(|targets| targets.slots.get(binding.slot_index).is_some()),
            PermanentEffectReference::Observed(
                TypedObservedEventObject::MovedOntoBattlefield
                | TypedObservedEventObject::CreatedTokens
                | TypedObservedEventObject::EnteringPermanent
                | TypedObservedEventObject::CombatCounterpart,
            ) => true,
            PermanentEffectReference::Observed(_) => false,
            PermanentEffectReference::ActionResult(result) => result.permanent,
        },
    }
}

fn drawn_cards_are_next_antecedent(sequence: &EffectSequenceNode, effect_index: usize) -> bool {
    matches!(
        sequence.effects.get(effect_index + 1),
        Some(LocalEffectNode::CardAction(
            CardActionNode::Reveal(
                CardOperandNode::Reference(_) | CardOperandNode::ObjectAntecedent { .. }
            ) | CardActionNode::Permit(CardPermissionNode {
                cards: CardOperandNode::Reference(_)
                    | CardOperandNode::ObjectAntecedent { .. }
                    | CardOperandNode::Selection(_),
                ..
            })
        ))
    )
}

/// One primitive per keyword action over its lowered permanent operand.
pub(in crate::oracle::canonical) fn lower_regeneration_shield(object: Value) -> Value {
    let duration = json!({"kind": "untilEndOfCurrentTurn"});
    match object["kind"].as_str() {
        Some("chosenTargets" | "eachPermanent") => {
            json!({"kind": "installRegenerationShields", "objects": object, "duration": duration})
        }
        _ => json!({"kind": "installRegenerationShield", "object": object, "duration": duration}),
    }
}

pub(in crate::oracle::canonical) fn lower_tap_state_change(untap: bool, object: Value) -> Value {
    match (untap, object["kind"].as_str()) {
        (false, _) => json!({"kind": "tapPermanent", "permanent": object}),
        // A collection operand untaps each of its objects (`untap each
        // other Merfolk you control`); the singular primitive takes one.
        (
            true,
            Some("chosenTargets" | "chosenObjects" | "eachPermanent" | "boundObjects" | "union"),
        ) => json!({"kind": "untapPermanents", "objects": object}),
        (true, _) => json!({"kind": "untapPermanent", "permanent": object}),
    }
}
