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
            | CardActionNode::Copy { cards, .. }
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
            LocalEffectNode::PreventDamage(prevent) => [&prevent.recipient, &prevent.source]
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

    for (effect_index, effect) in sequence.effects.iter().enumerate() {
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
            &target_bindings,
            &declared_player_targets,
        )?;
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
                action: SingularPlayerAction::Draw(_)
                    | SingularPlayerAction::Mill(_)
                    | SingularPlayerAction::Scry(_)
                    | SingularPlayerAction::CreateTokens(_),
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
                    let object =
                        lower_bound_permanent_reference(cursor, *object, &target_bindings)?;
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
                    SingularPlayerAction::Proliferate => {
                        effects.push(json!({"kind": "proliferate", "player": lowered.player}));
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
                        effects.push(
                            lower_token_creation_body(body, lowered.player, quantity)
                                .map_err(|expected| cursor.fail(expected))?,
                        );
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
                let amount = lower_sequence_bound_amount(
                    cursor,
                    amount,
                    sequence,
                    &target_bindings,
                    context,
                )?;
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
                effects.extend(
                    collections
                        .lower_action(cursor, action, declarations, &mut decisions)
                        .map_err(EffectSequenceLoweringFailure::FailedSupport)?,
                );
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
                let bind = reads_card_collection_reference(&sequence.effects[effect_index + 1..])
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
                effects.push(json!({"kind":"optionalEffects", "player":controller(), "effects":lowered.effects}));
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
                for object in objects {
                    let count = match recipient_count {
                        Some(counter) => json!({
                            "kind": "counterCountOf", "object": object.clone(), "counter": counter,
                        }),
                        None => amount.clone(),
                    };
                    effects.push(json!({"kind":"putCounters", "permanent":object, "counter":node.counter.as_str(), "count":count}));
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
                }
            },
            LocalEffectNode::OptionalLibrarySearch(search) => {
                cursor.enter("lower.optional-library-search.player");
                if let Err(failure) = cursor.require(
                    search.player == LocalPlayerReference::Controller,
                    "a controller-owned optional search; referenced controllers need an explicit snapshot",
                ) {
                    return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure));
                }
                let lowered = lower_controller_library_search_effects(cursor, search)?;
                collections.register_collection_binding_ids(
                    &lowered,
                    &lower_collection_binding(search.binding),
                );
                effects.push(json!({
                    "kind": "optionalEffects",
                    "player": controller(),
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
                        lower_become_types_to_object(
                            change,
                            &object,
                            copy.as_ref(),
                            &format!("characteristicObjects{effect_index}"),
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
                let duration = match restriction.duration {
                    UntapRestrictionDuration::NextUntapStep => "nextUntapStep",
                    UntapRestrictionDuration::WhileSourceControlled => "whileSourceControlled",
                };
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
                                "duration": {"kind": duration},
                            }],
                        }));
                        continue;
                    }
                    effects.push(json!({
                        "kind": "installUntapRestriction",
                        "permanent": object,
                        "duration": {"kind": duration},
                    }));
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
            LocalEffectNode::CopyStackObject(copy) if copy.event_spell => {
                // The spell the triggering cast record binds (CR 707.10).
                effects.push(json!({
                    "kind": "copyStackItem",
                    "object": {"kind": "triggeringStackObject"},
                    "controller": controller(),
                    "mayChooseNewTargets": copy.may_choose_new_targets,
                }));
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
                cursor.enter("lower.keyword-action.controller");
                effects.extend(
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
                );
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
                for (party_index, party) in [&prevent.recipient, &prevent.source]
                    .into_iter()
                    .enumerate()
                {
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
                            // A singular protected target is the antecedent
                            // a later `that creature` names.
                            if party_index == 0
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
                let reference =
                    lower_bound_permanent_reference(cursor, destroy.object, &target_bindings)?;
                let kind = match destroy.action {
                    ReferencedPermanentAction::Destroy => "destroyPermanent",
                    ReferencedPermanentAction::Exile => "exilePermanent",
                    ReferencedPermanentAction::Sacrifice => "sacrificePermanent",
                    // CR 400.3: the permanent goes to its owner's library.
                    ReferencedPermanentAction::PutOnOwnerLibrary(placement) => {
                        effects.push(json!({
                            "kind": "putPermanentOnOwnerLibrary",
                            "permanent": reference,
                            "position": match placement {
                                OwnerLibraryPlacement::Top => "top",
                                OwnerLibraryPlacement::Bottom => "bottom",
                                OwnerLibraryPlacement::OwnerChoiceTopOrBottom => {
                                    "ownerChoiceTopOrBottom"
                                }
                            },
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
                cursor.require(
                    source["kind"].as_str() == Some("chosenTarget"),
                    "a damage source declared as a target or bound to a declared target",
                )?;
                let amount = lower_sequence_bound_amount(
                    cursor,
                    &damage.amount,
                    sequence,
                    &target_bindings,
                    context,
                )?;
                let recipients = match &damage.recipient {
                    DamageRecipientNode::Targets(targets) => {
                        cursor.enter("lower.damage.object-source.recipient");
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
                        recipient["exclude"] = json!({"kind":"chosenTargets","id":source["id"]});
                    }
                    effects.push(json!({
                        "kind": "dealDamage",
                        "source": source.clone(),
                        "amount": amount.clone(),
                        "recipient": recipient,
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
                target_bindings.truncate(bound);
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
                let lowered = lower_targeted_damage_effect(damage, &target_id)
                    .ok_or_else(|| cursor.fail("a targeted damage effect"))?;
                decisions.push(lowered.decision);
                effects.push(lowered.effect);
                // A singular damage target is the antecedent a later reference
                // (`that creature`, `its controller`) resolves to.
                if let DamageRecipientNode::Targets(targets) = &damage.recipient
                    && let [slot] = targets.slots.as_slice()
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
                    && reads_card_collection_reference(&sequence.effects[effect_index + 1..])
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
                    && reads_card_collection_reference(&sequence.effects[effect_index + 1..])
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
                let objects = lower_ordinary_permanent_subject(
                    cursor,
                    operand,
                    effect_index,
                    false,
                    &permanent_target_base_ids,
                    &mut used_target_base_ids,
                    &mut decisions,
                    &mut target_bindings,
                )?;
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
        &target_bindings,
        &declared_player_targets,
    )?;
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
    install_action_result_bindings(cursor, sequence, &action_starts, &mut effects)?;
    install_action_result_quantities(cursor, &action_starts, &mut effects)?;
    install_action_result_object_counts(&action_starts, &mut effects).map_err(|expected| {
        cursor.enter("lower.action-result.count");
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
    })?;
    bind_deployed_card_when_read(cursor, &mut effects, &deployed_choices, collections)?;
    apply_delayed_frames(
        cursor,
        &sequence.delayed,
        &target_bindings,
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
    conditional_scopes.wrap(cursor, sequence, &action_starts, &mut effects)?;
    apply_where_x_bindings(
        cursor,
        &sequence.where_x,
        &action_starts,
        &decisions,
        &sequence_numeric_target_slots(sequence, &target_bindings),
        &mut effects,
    )?;
    for effect in &mut effects {
        super::resolution_choice::rebind_resolution_players(effect, &resolution_players);
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
        mut previous: PreviousActionResult<'_>,
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
                let previous_action_result =
                    if sentence_condition_reads_previous_action(&conditional.condition) {
                        previous.bind(cursor, effect_index)?
                    } else {
                        None
                    };
                let bindings = ConditionObjectBindings {
                    permanent_targets,
                    card_targets,
                    card_focus: collections.singular_focus_object(),
                    previous_action_result,
                };
                let mut condition =
                    lower_sentence_condition(cursor, &conditional.condition, &bindings)
                        .map_err(EffectSequenceLoweringFailure::FailedSupport)?;
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
                self.conditions[index] = Some(condition);
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
        let mut producers = effects[from..to].iter_mut().filter(|effect| {
            effect["kind"]
                .as_str()
                .is_some_and(|kind| verb.runtime_kinds().contains(&kind))
        });
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
    const RECORDING_KINDS: [&'static str; 1] = ["removeCounters"];

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
            .filter(|position| {
                self.effects[*position]["kind"]
                    .as_str()
                    .is_some_and(|kind| Self::RECORDING_KINDS.contains(&kind))
            })
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
                    Some((position, field))
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
    target_slots: &[NumericTargetSlot],
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
        // A property of a declared target reads the slot that declared it
        // (its last known information once it left, CR 608.2h).
        let value = lower_numeric_expression(&binding.expression)
            .or_else(|| {
                binding
                    .expression
                    .requires_target_binding()
                    .then(|| {
                        lower_numeric_expression_for_targets(&binding.expression, target_slots).ok()
                    })
                    .flatten()
            })
            .ok_or_else(|| {
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
    let operands: Vec<&PermanentOperandNode> = match next {
        LocalEffectNode::TemporaryKeywordGrant(node) => vec![&node.objects],
        LocalEffectNode::ModifyStats(node) => vec![&node.targets],
        LocalEffectNode::PutCounters(node) => vec![&node.permanent],
        LocalEffectNode::TapPermanents(node) => vec![&node.targets],
        LocalEffectNode::UntapRestriction(node) => vec![&node.objects],
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
                CardOperandNode::Reference(_) | CardOperandNode::Selection(_)
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
            effects.push(json!({
                "kind": "forEachPlayer",
                "bind": bind,
                "players": {"kind": "chosenTargets", "id": id},
                "effects": body,
            }));
            return Ok(());
        }
        PerformerNode::Population {
            players,
            excluding_controller,
        } => {
            let bind = format!("performer{effect_index}");
            // CR 102.3: `each other player` is every player but the
            // controller, teammates included, not the opponents set.
            let players = if *excluding_controller {
                json!({"kind": "playersExcept", "player": controller()})
            } else {
                lower_player_set_reference(*players)
            };
            let body = effects
                .drain(start..)
                .map(|mut effect| {
                    effect["performer"] = json!({"kind": "boundValue", "id": bind});
                    effect
                })
                .collect::<Vec<_>>();
            effects.push(json!({
                "kind": "forEachPlayer",
                "bind": bind,
                "players": players,
                "effects": body,
            }));
            return Ok(());
        }
    };
    for effect in &mut effects[start..] {
        effect["performer"] = player.clone();
    }
    Ok(())
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
        | PermanentOperandNode::Reference(_) => true,
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
        },
    }
}

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
