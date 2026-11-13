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

fn modifier_target_supported(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate.supports_domain(ObjectKind::Permanent)
                && predicate.has_domain(ObjectKind::Permanent)
        }
        TargetSelector::Union(branches) => {
            !branches.is_empty() && branches.iter().all(modifier_target_supported)
        }
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
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
            player: LocalPlayerReference::ControllerOf(_),
        } => return None,
    })
}

fn target_id_for_ordinal(base_id: &str, ordinal: usize, target_count: usize) -> String {
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

fn lower_local_effect_sequence_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    sequence: &EffectSequenceNode,
    context: EffectSequenceLoweringContext,
) -> Result<LoweredEffectSequence, EffectSequenceLoweringFailure> {
    let permanent_target_base_ids = sequence
        .effects
        .iter()
        .flat_map(|effect| match effect {
            LocalEffectNode::ModifyStats(modifier) => modifier
                .targets
                .slots
                .iter()
                .filter_map(|slot| {
                    lower_target_selector_candidates(&slot.selector).map(|(id, _)| id)
                })
                .collect::<Vec<_>>(),
            LocalEffectNode::DealDamage(damage) => {
                default_damage_target_id(damage).into_iter().collect()
            }
            LocalEffectNode::ZoneChange(zone_change) => {
                vec![lower_single_target(&zone_change.target).0]
            }
            LocalEffectNode::TemporaryKeywordGrant(grant) => match &grant.objects {
                TemporaryKeywordGrantObjectsNode::Targets(targets) => targets
                    .slots
                    .iter()
                    .filter_map(|slot| {
                        lower_target_selector_candidates(&slot.selector).map(|(id, _)| id)
                    })
                    .collect::<Vec<_>>(),
                TemporaryKeywordGrantObjectsNode::ControlledSet(_) => Vec::new(),
            },
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
        .filter_map(|effect| match effect {
            LocalEffectNode::TargetPlayerLife(life) if life.declares_target => Some(life.player),
            _ => None,
        })
        .fold(Vec::new(), |mut bindings, binding| {
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
            bindings
        });
    let mut declared_player_targets = Vec::<(PlayerTargetBinding, String)>::new();

    for (effect_index, effect) in sequence.effects.iter().enumerate() {
        match effect {
            LocalEffectNode::TemporaryKeywordGrant(grant) => {
                cursor.enter("lower.keyword-grant.scalar-target");
                let TemporaryKeywordGrantObjectsNode::Targets(targets) = &grant.objects else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("a targeted temporary keyword grant in this sequence"),
                    ));
                };
                let [slot] = targets.slots.as_slice() else {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        cursor.fail("exactly one target slot for a temporary keyword grant"),
                    ));
                };
                if let Err(failure) = cursor.require(
                    slot.cardinality.maximum == 1,
                    "a scalar target slot; target collections require collection lowering",
                ) {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                        failure,
                    ));
                }
                let (base_id, _) = lower_target_selector_candidates(&slot.selector)
                    .ok_or_else(|| cursor.fail("a lowerable permanent target selector"))?;
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
                let lowered = lower_temporary_keyword_grant(grant.clone(), &target_id)
                    .ok_or_else(|| cursor.fail("a lowerable targeted temporary keyword grant"))?;
                let decision = lowered.decision.ok_or_else(|| {
                    cursor.fail("a target decision for the temporary combat restriction")
                })?;
                decisions.push(decision);
                effects.extend(lowered.effects);
                target_bindings.push((
                    TargetBinding {
                        effect_index,
                        slot_index: 0,
                    },
                    target_id,
                ));
            }
            LocalEffectNode::ControllerLibrarySearch(search) => {
                effects.extend(lower_controller_library_search_effects(cursor, search)?);
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
                let duration = lower_duration(&modifier.duration)
                    .ok_or_else(|| cursor.fail("a duration independent of a target binding"))?;
                for (slot_index, slot) in modifier.targets.slots.iter().enumerate() {
                    cursor.enter("lower.modify-stats.scalar-target");
                    if let Err(failure) = cursor.require(
                        slot.cardinality.maximum == 1,
                        "a scalar target slot; target collections require collection lowering",
                    ) {
                        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            failure,
                        ));
                    }
                    if let Err(failure) = cursor.require(
                        modifier_target_supported(&slot.selector),
                        "a permanent target selector for a power/toughness modifier",
                    ) {
                        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            failure,
                        ));
                    }
                    let (_, mut candidates) = lower_target_selector_candidates(&slot.selector)
                        .ok_or_else(|| cursor.fail("a lowerable permanent target selector"))?;
                    if slot_index == 0 && modifier.exclude_source_from_first_target {
                        candidates["excludeSource"] = Value::Bool(true);
                    }
                    let (base_id, _) = lower_target_selector_candidates(&slot.selector)
                        .ok_or_else(|| cursor.fail("a lowerable permanent target selector"))?;
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
                    target_bindings.push((
                        TargetBinding {
                            effect_index,
                            slot_index,
                        },
                        target_id.clone(),
                    ));
                    let object = chosen_target(&target_id);
                    effects.push(json!({
                        "kind": "modifyPowerToughness",
                        "object": object.clone(),
                        "power": integer(modifier.delta.power),
                        "toughness": integer(modifier.delta.toughness),
                        "duration": duration.clone(),
                    }));
                    effects.extend(modifier.keywords.iter().map(|keyword| {
                        json!({
                            "kind": "grantKeyword",
                            "object": object.clone(),
                            "keyword": keyword.canonical_name(),
                            "duration": duration.clone(),
                        })
                    }));
                }
            }
            LocalEffectNode::UntapTarget(untap) => {
                cursor.enter("lower.untap-target.permanent-binding");
                let target_id = target_bindings
                    .iter()
                    .find_map(|(binding, target_id)| {
                        (*binding == untap.permanent).then_some(target_id)
                    })
                    .ok_or_else(|| cursor.fail("an earlier target slot for the untap anaphor"))?;
                effects.push(json!({
                    "kind": "untapPermanent",
                    "permanent": chosen_target(target_id),
                }));
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
                let (base_id, candidates, minimum, maximum) =
                    lower_single_target(&zone_change.target);
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
                effects.push(lower_targeted_zone_change_node(
                    cursor,
                    zone_change,
                    chosen_target(&target_id),
                )?);
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
            }
            LocalEffectNode::CreateNamedTokens(creation) => {
                cursor.enter("lower.create-named-token.controller");
                let token_controller = match creation.player {
                    LocalPlayerReference::Controller => controller(),
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
                cursor.require(
                    !matches!(creation.quantity, NumericAtom::VariableX),
                    "a literal named-token quantity; X requires a declaration",
                )?;
                effects.push(json!({
                    "kind": "createTokens",
                    "controller": token_controller,
                    "quantity": lower_numeric_atom(creation.quantity.clone()),
                    "token": {
                        "kind": "namedToken",
                        "name": creation.token_name,
                    },
                }));
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
                let amount = lower_numeric_expression(&life.amount)
                    .ok_or_else(|| cursor.fail("a target-independent life amount"))?;
                effects.push(json!({
                    "kind": match life.action {
                        TargetPlayerLifeAction::Gain => "gainLife",
                        TargetPlayerLifeAction::Lose => "loseLife",
                    },
                    "player": chosen_target(target_id),
                    "amount": amount,
                }));
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
                let lowered =
                    lower_targetless_local_effect(effect, declarations).map_err(|error| {
                        EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                            cursor.fail(error.expectation()),
                        )
                    })?;
                effects.push(lowered);
            }
        }
    }

    Ok(LoweredEffectSequence { decisions, effects })
}

#[cfg(test)]
#[path = "effect_sequence_tests.rs"]
mod tests;
