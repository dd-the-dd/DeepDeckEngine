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
                LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_),
        } => return None,
        // No continuous-effect primitive reads the end-of-next-turn window.
        EffectDurationNode::UntilEndOfNextTurn { .. } => return None,
        EffectDurationNode::WhileSourceControlled => json!({ "kind": "whileSourceControlled" }),
    })
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

/// Whether the next action that reads or replaces the card-collection scope
/// reads it through a lexical reference (`that card`, `it`, `them`).
fn reads_card_collection_reference(following: &[LocalEffectNode]) -> bool {
    following.iter().find_map(|effect| match effect {
        LocalEffectNode::CardAction(
            CardActionNode::Move { cards, .. }
            | CardActionNode::Reveal(cards)
            | CardActionNode::Permit(CardPermissionNode { cards, .. }),
        ) => Some(matches!(cards, CardOperandNode::Reference(_))),
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
    let permanent_target_base_ids = sequence
        .effects
        .iter()
        .flat_map(|effect| match effect {
            LocalEffectNode::ModifyStats(_)
            | LocalEffectNode::BecomeTypes(_)
            | LocalEffectNode::PutCounters(_)
            | LocalEffectNode::TapPermanents(_)
            | LocalEffectNode::PermanentKeywordAction(_)
            | LocalEffectNode::ControlChange(_)
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
                default_damage_target_id(damage).into_iter().collect()
            }
            LocalEffectNode::PreventDamage(prevent) => [&prevent.recipient, &prevent.source]
                .into_iter()
                .filter_map(|party| damage_party_targets(party))
                .filter_map(|targets| lower_target_slot_candidates(&targets.slots[0]))
                .map(|(id, _)| id)
                .collect(),
            LocalEffectNode::ZoneChange(zone_change) => {
                lower_zone_change_target_candidates(zone_change)
                    .map(|(id, _, _, _)| vec![id])
                    .unwrap_or_default()
            }
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
    let player_target_bindings = sequence
        .effects
        .iter()
        .enumerate()
        .filter_map(|(effect_index, effect)| match effect {
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
            _ => None,
        })
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

    for (effect_index, effect) in sequence.effects.iter().enumerate() {
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
        unbound_creature_type_from = effects.len();
        conditional_scopes.at_boundary(
            cursor,
            sequence,
            effect_index,
            collections,
            &target_bindings,
            &card_target_bindings,
        )?;
        action_starts.push((effects.len(), decisions.len()));
        // A postposed definition supplies X to the actions of its sentence, so
        // those actions lower X provisionally and the binding replaces it below.
        let context = if sequence
            .where_x
            .iter()
            .any(|binding| binding.covers(effect_index))
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
                action: SingularPlayerAction::Draw(_) | SingularPlayerAction::Mill(_),
                ..
            })
        ) {
            collections.clear_antecedent();
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
                    PlayerEffectSubject::Controller | PlayerEffectSubject::EventPlayer => {
                        String::new()
                    }
                };
                let antecedent = match &action.player {
                    PlayerEffectSubject::Bound(binding) => Some(BoundPlayerLowering {
                        binding: *binding,
                        target_id: &target_id,
                    }),
                    _ => None,
                };
                let lowered = lower_player_effect_subject(&action.player, &target_id, antecedent)
                    .map_err(|_| {
                    cursor.fail("a valid declared or previously bound singular player subject")
                })?;
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
                    SingularPlayerAction::Life(life) => (
                        match life.action {
                            TargetPlayerLifeAction::Gain => "gainLife",
                            TargetPlayerLifeAction::Lose => "loseLife",
                        },
                        "amount",
                        &life.amount,
                    ),
                    SingularPlayerAction::Discard(discard) => {
                        validate_targetless_numeric_expression(&discard.count, declarations)
                            .map_err(|error| {
                                EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                    cursor.fail(error.expectation()),
                                )
                            })?;
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
                                &format!("effect{effect_index}:playerSacrifice"),
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
                let amount = lower_sequence_bound_amount(
                    cursor,
                    amount,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                let mut effect = json!({"kind": kind, "player": lowered.player});
                effect[field] = amount;
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
                effects.extend(
                    collections
                        .lower_action(cursor, action, declarations, &mut decisions)
                        .map_err(EffectSequenceLoweringFailure::FailedSupport)?,
                );
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
                effects.push(json!({"kind":"optionalEffects", "player":controller(), "effects":lowered.effects}));
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
                for object in objects {
                    effects.extend(
                        lower_keyword_grant_to_object(
                            &grant.keywords,
                            &grant.abilities,
                            &grant.duration,
                            &object,
                        )
                        .ok_or_else(|| {
                            cursor.fail("a supported keyword-grant duration and granted rule")
                        })?,
                    );
                }
            }
            LocalEffectNode::PutCounters(node) => {
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
                for object in objects {
                    effects.push(json!({"kind":"putCounters", "permanent":object, "counter":node.counter.as_str(), "count":amount}));
                }
            }
            LocalEffectNode::ControllerLibrarySearch(search) => {
                effects.extend(lower_controller_library_search_effects(cursor, search)?);
            }
            LocalEffectNode::OptionalLibrarySearch(search) => {
                cursor.enter("lower.optional-library-search.player");
                if let Err(failure) = cursor.require(
                    search.player == LocalPlayerReference::Controller,
                    "a controller-owned optional search; referenced controllers need an explicit snapshot",
                ) {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure));
                }
                effects.push(json!({
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": lower_controller_library_search_effects(cursor, search)?,
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
                for object in objects {
                    effects.extend(
                        lower_become_types_to_object(change, &object)
                            .map_err(|expected| cursor.fail(expected))?,
                    );
                }
            }
            LocalEffectNode::ModifyStats(modifier) => {
                cursor.enter("lower.modify-stats.another-target-relation");
                if modifier.exclude_source_from_first_target
                    && decisions
                        .iter()
                        .any(|decision: &Value| decision["kind"].as_str() == Some("chooseTargets"))
                {
                    // This local node can represent source exclusion, but the
                    // sequence result cannot yet carry a relation to earlier
                    // target declarations. Refuse that distinct-target meaning
                    // instead of silently substituting excludeSource, even if
                    // the previous target's domain appears incompatible.
                    return Err(EffectSequenceLoweringFailure::FailedSupport(
                        cursor.fail("distinct-target relation support for `another` after a preceding target declaration"),
                    ));
                }
                cursor.require(
                    [&modifier.delta.power, &modifier.delta.toughness].into_iter().all(|amount| {
                        !matches!(amount, NumericExpression::Atom(NumericAtom::Integer(value)) if i32::try_from(*value).is_err())
                    }),
                    "a power/toughness delta representable by runtime i32 quantities",
                )?;
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
                        )
                        .ok_or_else(|| {
                            cursor.fail("a supported keyword-grant duration and granted rule")
                        })?,
                    );
                }
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
                                | EffectDurationNode::WhileSourceControlled),
                            ) => lower_duration(duration),
                            Some(_) => {
                                return Err(
                                    EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                                        cursor.fail(
                                            "an indefinite, until-end-of-turn or while-source-controlled control duration",
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
            LocalEffectNode::CopyStackObject(copy) => {
                cursor.enter("lower.copy-stack-object.target");
                let [slot] = copy.target.slots.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("one stack-object target slot for the copy action"),
                    ));
                };
                // The stack-object target slot (id and candidates) is the one
                // the counter action declares; only the action differs.
                let (target_id, candidates, _) = lower_counter_action(slot, false);
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
                effects.push(json!({
                    "kind": "copyStackItem",
                    "object": chosen_target(target_id),
                    "controller": controller(),
                    "mayChooseNewTargets": copy.may_choose_new_targets,
                }));
            }
            LocalEffectNode::PermanentKeywordAction(keyword_action) => {
                cursor.enter(match keyword_action.action {
                    PermanentKeywordAction::Regenerate => "lower.regenerate.object",
                    PermanentKeywordAction::Goad => "lower.goad.object",
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
                for object in objects {
                    effects.push(lower_permanent_keyword_action(
                        keyword_action.action,
                        object,
                    ));
                }
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
                // The engine resolves an attached object and a destination
                // as source, target, bound or decision references only. An
                // event-object reference is not a supported attach operand.
                cursor.require(
                    attachments
                        .iter()
                        .chain(recipients.iter())
                        .all(|object| value_kind_is_not_observed(object)),
                    "an attach operand that is not an observed event object",
                )?;
                let ([attachment], [recipient]) = (attachments.as_slice(), recipients.as_slice())
                else {
                    return Err(EffectSequenceLoweringFailure::FailedSupport(
                        cursor.fail("one attached object and one attachment destination"),
                    ));
                };
                effects.push(json!({
                    "kind": "attachPermanent",
                    "attachment": attachment.clone(),
                    "to": recipient.clone(),
                }));
            }
            LocalEffectNode::PreventDamage(prevent) => {
                cursor.enter("lower.prevent.parties");
                let mut parties = Vec::with_capacity(2);
                for party in [&prevent.recipient, &prevent.source] {
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
                let source = parties.pop().flatten();
                let recipient = parties.pop().flatten();
                effects.push(lower_prevent_damage_shield(
                    prevent, amount, recipient, source,
                ));
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
                }
            }
            LocalEffectNode::UntapTarget(untap) => {
                cursor.enter("lower.untap-target.permanent-binding");
                let reference =
                    lower_bound_permanent_reference(cursor, untap.permanent, &target_bindings)?;
                effects.push(json!({"kind":"untapPermanent", "permanent":reference}));
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
                let lowered = lower_targeted_damage_effect(damage, &target_id)
                    .ok_or_else(|| cursor.fail("a targeted damage effect"))?;
                decisions.push(lowered.decision);
                effects.push(lowered.effect);
            }
            LocalEffectNode::ZoneChange(zone_change) => {
                cursor.enter("lower.zone-change.target-candidates");
                let (base_id, candidates, minimum, maximum) = cursor.require_value(
                    lower_zone_change_target_candidates(zone_change),
                    "a supported zone-change target candidate policy",
                )?;
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
                // `other` after an earlier target can express a relation to
                // that target. A new collection must not approximate it as
                // source exclusion while distinct-target constraints are absent.
                if maximum > 1
                    && zone_change.target.slots[0].exclude_source
                    && decisions
                        .iter()
                        .any(|decision| decision["kind"] == "chooseTargets")
                {
                    cursor.enter("lower.zone-change.another-target-relation");
                    return Err(EffectSequenceLoweringFailure::FailedSupport(cursor.fail(
                        "a represented distinct-target relation for the plural other-target operand",
                    )));
                }
                decisions.push(target_decision(&target_id, candidates, minimum, maximum));
                let target = lower_zone_change_target_operand(
                    cursor,
                    zone_change,
                    &target_id,
                    ZoneChangeTargetSupport::DestroyCollection,
                )?;
                let mut operation = lower_targeted_zone_change_node(cursor, zone_change, target)?;
                // A later card action reading `that card`/`it` names the object
                // this exile produced; record that result only when it is read.
                if zone_change.action == ZoneChangeAction::Exile
                    && maximum == 1
                    && zone_change.target.slots.len() == 1
                    && reads_card_collection_reference(&sequence.effects[effect_index + 1..])
                {
                    operation["bind"] = json!(collections.observe_exiled_object());
                }
                effects.push(operation);
                if minimum == 1 && maximum == 1 && zone_change.target.slots.len() == 1 {
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
                effects.push(
                    lower_target_card_move_effect(zone_change, &target_id)
                        .ok_or_else(|| cursor.fail("a supported targeted card move"))?,
                );
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
                    LocalPlayerReference::ControllerOfQualified(_) => {
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
            effect => {
                let declarations = match context {
                    EffectSequenceLoweringContext::NoExternalX => {
                        TargetlessDeclarationSupport::None
                    }
                    EffectSequenceLoweringContext::ValidateXAfterCardComposition => {
                        TargetlessDeclarationSupport::XValue
                    }
                };
                let mut lowered = lower_targetless_local_effects(
                    effect,
                    declarations,
                    Some(&format!("effect{effect_index}")),
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
                effects.extend(lowered);
            }
        }
    }

    conditional_scopes.at_boundary(
        cursor,
        sequence,
        sequence.effects.len(),
        collections,
        &target_bindings,
        &card_target_bindings,
    )?;
    action_starts.push((effects.len(), decisions.len()));
    apply_delayed_frames(cursor, &sequence.delayed, &mut action_starts, &mut effects)?;
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
    conditional_scopes.wrap(cursor, sequence, &action_starts, &mut effects)?;
    apply_where_x_bindings(
        cursor,
        &sequence.where_x,
        &action_starts,
        &decisions,
        &mut effects,
    )?;

    Ok(LoweredEffectSequence { decisions, effects })
}

/// Per-sentence state of the conditional instructions of one sequence
/// (CR 608.2c). Each condition is lowered where its sentence begins, with the
/// bindings visible there; each branch starts from the same card-collection
/// scope, and a scope that differs after the instruction does not escape it.
struct ConditionalScopes {
    conditions: Vec<Option<Value>>,
    before: Vec<Option<CardCollectionMemory>>,
    after_then: Vec<Option<CardCollectionMemory>>,
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
                previous_end <= conditional.then_first
                    && conditional.then_first < conditional.then_end
                    && conditional
                        .else_end
                        .is_none_or(|end| conditional.then_end < end)
                    && conditional.end() <= sequence.effects.len(),
                "ordered, nonempty and disjoint conditional sentence ranges",
            )?;
            previous_end = conditional.end();
        }
        if !sequence.conditionals.is_empty() && !sequence.where_x.is_empty() {
            cursor.enter("lower.sentence-conditional.where-x");
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                "a sequence without both a sentence conditional and a postposed definition of X",
            )));
        }
        let count = sequence.conditionals.len();
        Ok(Self {
            conditions: vec![None; count],
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
    ) -> Result<(), EffectSequenceLoweringFailure> {
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
                }
            }
        }
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            if conditional.else_end.is_some() && conditional.then_end == effect_index {
                self.after_then[index] = Some(collections.clone());
                let before = self.before[index]
                    .as_ref()
                    .expect("a conditional scope opens before its other branch");
                collections.restore_scope(before);
            }
        }
        for (index, conditional) in sequence.conditionals.iter().enumerate() {
            if conditional.then_first == effect_index {
                let bindings = ConditionObjectBindings {
                    permanent_targets,
                    card_targets,
                    card_focus: collections.singular_focus_object(),
                };
                self.conditions[index] = Some(
                    lower_sentence_condition(cursor, &conditional.condition, &bindings)
                        .map_err(EffectSequenceLoweringFailure::FailedSupport)?,
                );
                self.before[index] = Some(collections.clone());
            }
        }
        Ok(())
    }

    /// Replaces each conditional's action range by one `conditional` node.
    /// Target declarations stay ability-wide (CR 601.2c); a branch governed
    /// by a cast choice such as kicker cannot own a required target.
    fn wrap(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        sequence: &EffectSequenceNode,
        action_starts: &[(usize, usize)],
        effects: &mut Vec<Value>,
    ) -> Result<(), EffectSequenceLoweringFailure> {
        for (index, conditional) in sequence.conditionals.iter().enumerate().rev() {
            let (then_from, decisions_from) = action_starts[conditional.then_first];
            let (then_to, _) = action_starts[conditional.then_end];
            let (else_to, decisions_to) = action_starts[conditional.end()];
            cursor.enter("lower.sentence-conditional.cast-choice-targets");
            if sentence_condition_depends_on_cast_choice(&conditional.condition)
                && decisions_from != decisions_to
            {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail(
                        "a cast-choice conditional branch without target declarations of its own",
                    ),
                ));
            }
            let condition = self.conditions[index]
                .take()
                .expect("every conditional is lowered at its first action");
            let otherwise = effects.drain(then_to..else_to).collect::<Vec<_>>();
            let then = effects.drain(then_from..then_to).collect::<Vec<_>>();
            effects.insert(
                then_from,
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

/// Binds each postposed `where X is EXPR` definition (CR 107.3). The value is
/// computed once, by a `bind` placed before the sentence's first action, and
/// every X reference lowered by that sentence's actions reads the binding.
/// An X that a target declaration needs, or an expression the shared numeric
/// lowering cannot represent, fails explicitly instead of defaulting.
fn apply_where_x_bindings(
    cursor: &mut OracleTokenCursor<'_>,
    bindings: &[WhereXBindingNode],
    action_starts: &[(usize, usize)],
    decisions: &[Value],
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut binds = Vec::new();
    for (ordinal, binding) in bindings.iter().enumerate() {
        let (effect_from, decision_from) = action_starts[binding.first_effect];
        let (effect_to, decision_to) = action_starts[binding.end_effect];
        cursor.enter("lower.where-x.declarations");
        if decisions[decision_from..decision_to]
            .iter()
            .any(references_x_value)
        {
            return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("X in action quantities, not in a target declaration"),
            ));
        }
        cursor.enter("lower.where-x.definition");
        let value = lower_numeric_expression(&binding.expression).ok_or_else(|| {
            EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                cursor.fail("a definition of X that the shared numeric lowering represents"),
            )
        })?;
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
        cursor.require(uses > 0, "an action in this sentence whose quantity is X")?;
        binds.push((
            effect_from,
            json!({ "kind": "bind", "id": id, "value": value }),
        ));
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
                "condition": not(lower_trigger_guard(condition)),
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

fn value_kind_is_not_observed(object: &Value) -> bool {
    object["kind"].as_str() != Some("observedEventObject")
}

#[cfg(test)]
#[path = "effect_sequence_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "preposed_duration_tests.rs"]
mod preposed_duration_tests;

#[path = "delayed_frame_lowering.rs"]
mod delayed_frame_lowering;
use delayed_frame_lowering::apply_delayed_frames;

#[cfg(test)]
#[path = "where_x_tests.rs"]
mod where_x_tests;

#[cfg(test)]
#[path = "unless_frame_tests.rs"]
mod unless_frame_tests;

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
                | LocalEffectNode::PutCounters(_) => (
                    ordinary_action_targets(sequence.effects.get(binding.effect_index)?)?,
                    false,
                ),
                LocalEffectNode::ZoneChange(effect) => (&effect.target, true),
                _ => return None,
            };
            let slot = targets.slots.get(binding.slot_index)?;
            if slot.cardinality.minimum != 1 || slot.cardinality.maximum != 1 {
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
        LocalPlayerReference::ControllerOfQualified(_) => {
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
    sequence
        .effects
        .get(effect_index + 1)
        .and_then(|next| match next {
            LocalEffectNode::TemporaryKeywordGrant(node) => Some(&node.objects),
            LocalEffectNode::ModifyStats(node) => Some(&node.targets),
            LocalEffectNode::PutCounters(node) => Some(&node.permanent),
            LocalEffectNode::TapPermanents(node) => Some(&node.targets),
            LocalEffectNode::PermanentKeywordAction(node) => Some(&node.targets),
            _ => None,
        })
        .is_some_and(|operand| *operand == reference)
}

/// True when the action after a draw names the drawn cards by anaphora
/// (`... and reveal them`, `you may cast one of them ...`).
fn drawn_cards_are_next_antecedent(sequence: &EffectSequenceNode, effect_index: usize) -> bool {
    matches!(
        sequence.effects.get(effect_index + 1),
        Some(LocalEffectNode::CardAction(
            CardActionNode::Reveal(CardOperandNode::Reference(_))
                | CardActionNode::Permit(CardPermissionNode {
                    cards: CardOperandNode::Reference(_) | CardOperandNode::Selection(_),
                    ..
                })
        ))
    )
}

/// One primitive per keyword action over its lowered permanent operand.
pub(in crate::oracle::canonical) fn lower_permanent_keyword_action(
    action: PermanentKeywordAction,
    object: Value,
) -> Value {
    match action {
        PermanentKeywordAction::Regenerate => lower_regeneration_shield(object),
        // CR 701.38a: the ability controller goads; the designation lasts
        // until that player's next turn (engine `goaded` modifier).
        PermanentKeywordAction::Goad => json!({"kind": "goadPermanents", "objects": object}),
    }
}

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
        (true, Some("chosenTargets")) => json!({"kind": "untapPermanents", "objects": object}),
        (true, _) => json!({"kind": "untapPermanent", "permanent": object}),
    }
}
