//! Shared event lowering for the generic triggered-program parent.
//! Event recognition never implies that its runtime semantics are supported.

use super::super::*;

/// One source transition, or the `oneOf` form used by permanent transitions
/// for a union such as `enters or dies`. Branch order is the written order.
pub(in crate::oracle::canonical) fn lower_source_transition_event(
    envelope: &SourceTriggerEnvelopeNode,
) -> Value {
    let events: Vec<Value> = envelope
        .events()
        .into_iter()
        .map(|event| {
            let kind = match event {
                SourceTriggerEventKind::Entered => "enterBattlefield",
                SourceTriggerEventKind::Died => "permanentDied",
                SourceTriggerEventKind::Tapped => "permanentTapped",
            };
            json!({"kind": kind, "object": self_ref()})
        })
        .collect();
    if envelope.is_union() {
        json!({"kind": "oneOf", "events": events})
    } else {
        events.into_iter().next().expect("one source transition")
    }
}

pub(in crate::oracle::canonical) fn lower_generic_trigger_event(
    cursor: &mut OracleTokenCursor<'_>,
    event: &GenericTriggerEventNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.program.event-lowering");
    match event {
        GenericTriggerEventNode::SourceTransition(envelope) => {
            cursor.require(
                envelope.source == LocalObjectReference::SourceSelf,
                "the current source as transition subject",
            )?;
            Ok(lower_source_transition_event(envelope))
        }
        GenericTriggerEventNode::SpellCast(event) => {
            // The existing matcher observes a printed definition, not the
            // contextual stack spell. Preserve the old numeric support cap.
            if event.query.predicate.contains_numeric_comparison() {
                cursor.enter("trigger.spell-cast.numeric-stack-context");
                return Err(cursor.fail(
                    "a spell event predicate without unproved numeric stack characteristics",
                ));
            }
            let predicate =
                lower_spell_object_predicate(&event.query.predicate).unwrap_or(Value::Null);
            let mut lowered = json!({"kind": "spellCast", "where": predicate});
            match event.caster {
                SpellCastEventCaster::Controller => lowered["player"] = controller(),
                SpellCastEventCaster::AnyPlayer => lowered["anyPlayer"] = json!(true),
                SpellCastEventCaster::Opponent => {
                    lowered["opponentOfSourceController"] = json!(true);
                }
            }
            if let Some(ordinal) = &event.ordinal {
                cursor.require(
                    ordinal.basis == SpellCastOrdinalBasis::AllSpellsByCasterThisTurn,
                    "an ordinal counted among all spells cast by that caster this turn",
                )?;
                lowered["spellCastOrdinal"] = json!(ordinal.value);
            }
            Ok(lowered)
        }
        GenericTriggerEventNode::Sacrificed(event) => lower_sacrifice_event(cursor, event),
        GenericTriggerEventNode::ActorObjectAlternatives(event) => {
            cursor.require(
                !event.actions.is_empty(),
                "at least one typed event alternative",
            )?;
            let events = event.actions.iter().map(|action| match action {
                ActorObjectEventAction::SacrificePermanent => lower_sacrifice_event(cursor, &SacrificeEventNode {
                    actor: event.actor, object: event.object.clone(), exclude_source: event.exclude_source,
                }),
                ActorObjectEventAction::CreateToken => {
                    cursor.enter("trigger.creation.actor-scope-support");
                    cursor.require(event.actor == EventActorScope::Controller,
                        "the source controller as token creator until other creation actor matchers are proved")?;
                    cursor.enter("trigger.creation.source-exclusion-support");
                    cursor.require(!event.exclude_source,
                        "a creation event without an unproved source exclusion")?;
                    require_stable_permanent_event_predicate(cursor, &event.object.predicate)?;
                    Ok(json!({"kind":"tokenCreated", "player":controller(),
                        "where":lower_permanent_object_predicate(&event.object.predicate)}))
                }
            }).collect::<MachineResult<Vec<_>>>()?;
            if events.len() == 1 {
                Ok(events.into_iter().next().expect("validated event"))
            } else {
                Ok(json!({"kind":"oneOf","events":events}))
            }
        }
        GenericTriggerEventNode::Step(event) => {
            let step = match event.step {
                TurnStep::Upkeep => "upkeep",
                TurnStep::Draw => "draw",
                TurnStep::PrecombatMain => "precombatMain",
                TurnStep::BeginCombat => "beginCombat",
                TurnStep::PostcombatMain => "postcombatMain",
                TurnStep::EndStep => "endStep",
            };
            let player = match event.player {
                StepPlayerScope::AbilityController => controller(),
                StepPlayerScope::EachPlayer => json!({"kind": "eachPlayer"}),
                StepPlayerScope::OpponentsOfAbilityController => {
                    json!({"kind": "opponentsOf", "player": controller()})
                }
            };
            Ok(json!({"kind": "stepBegan", "step": step, "player": player}))
        }
        GenericTriggerEventNode::PermanentTransition {
            kind,
            subjects,
            entry_state,
        } => {
            cursor.require(
                !subjects.alternatives.is_empty(),
                "one or more permanent event subjects",
            )?;
            let events = subjects
                .alternatives
                .iter()
                .map(|subject| {
                    let mut lowered = lower_transition_branch(cursor, *kind, subject)?;
                    if let Some(state) = entry_state {
                        cursor.require(
                            *kind == PermanentTransitionKind::Entered,
                            "an entry observation only on a battlefield-entry event",
                        )?;
                        lowered["entryState"] =
                            json!({"tapped":*state==EntryStatePredicate::Tapped});
                    }
                    Ok(lowered)
                })
                .collect::<MachineResult<Vec<_>>>()?;
            if events.len() == 1 {
                Ok(events.into_iter().next().expect("one validated subject"))
            } else {
                Ok(json!({"kind": "oneOf", "events": events}))
            }
        }
        GenericTriggerEventNode::Local(event) => lower_local_event(cursor, event),
        GenericTriggerEventNode::Object(event) => lower_object_event(cursor, event),
    }
}

/// Maps (verb, multiplicity) to the runtime matcher that already implements
/// that event grouping (CR 603.2c). The subject query is lowered by the shared
/// predicate lowerer. Combinations without an inspected batch matcher fail
/// explicitly rather than approximating a per-event trigger per object.
fn lower_object_event(
    cursor: &mut OracleTokenCursor<'_>,
    event: &ObjectEventNode,
) -> MachineResult<Value> {
    let subject = &event.subject;
    cursor.enter("trigger.object-event.threshold-support");
    if let EventMultiplicity::PerEvent { minimum } = subject.multiplicity {
        cursor.require(
            minimum == 1,
            "a `one or more` threshold (no runtime matcher counts larger event batches)",
        )?;
    }
    let per_event = matches!(subject.multiplicity, EventMultiplicity::PerEvent { .. });
    if let ObjectEventVerb::Enter | ObjectEventVerb::Die = event.verb {
        // The shared permanent-transition lowering owns subject, controller
        // and `other`; the runtime groups one event per instruction or
        // state-based-action pass.
        let transition = if event.verb == ObjectEventVerb::Enter {
            PermanentTransitionKind::Entered
        } else {
            PermanentTransitionKind::Died
        };
        cursor.enter("trigger.object-event.transition-subject");
        cursor.require(
            subject
                .query
                .predicate
                .supports_domain(ObjectKind::Permanent)
                && subject.query.predicate.has_nominal_head(),
            "a nominal battlefield-permanent subject",
        )?;
        let mut lowered = lower_transition_branch(
            cursor,
            transition,
            &PermanentDiedEventBranchNode::Filtered {
                predicate: subject.query.predicate.clone(),
                controller: subject.query.controller,
                exclude_source: subject.exclude_source,
            },
        )?;
        if per_event {
            lowered["aggregation"] = json!("oneOrMore");
        }
        return Ok(lowered);
    }
    cursor.enter("trigger.object-event.source-exclusion-support");
    cursor.require(
        !subject.exclude_source,
        "an object event without an unproved source exclusion",
    )?;
    cursor.enter("trigger.object-event.verb-support");
    match event.verb {
        ObjectEventVerb::LeaveControllerGraveyard => {
            cursor.require(
                subject.query.controller.is_none()
                    && subject.query.predicate.supports_domain(ObjectKind::Card),
                "an uncontrolled card query for cards leaving a graveyard",
            )?;
            Ok(json!({
                "kind": "cardsLeftGraveyard",
                "player": controller(),
                "where": lower_card_object_predicate(&subject.query.predicate),
                "aggregation": if per_event { "oneOrMore" } else { "perCard" },
            }))
        }
        ObjectEventVerb::Attack | ObjectEventVerb::DealCombatDamageToPlayer => {
            cursor.require(
                per_event,
                "a `one or more` subject (per-object attack and damage matchers are separate)",
            )?;
            cursor.require(
                subject.query.controller == Some(ObjectControllerScope::AbilityController)
                    && subject
                        .query
                        .predicate
                        .supports_domain(ObjectKind::Permanent),
                "permanents the ability controller controls",
            )?;
            let filter = lower_permanent_object_predicate(&subject.query.predicate);
            Ok(match event.verb {
                // One attack declaration is one event for all attackers.
                ObjectEventVerb::Attack => json!({
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "where": filter,
                }),
                // Simultaneous combat damage is grouped per damaged player.
                _ => json!({
                    "kind": "controlledCreaturesCombatDamageToPlayer",
                    "player": controller(),
                    "where": filter,
                }),
            })
        }
        ObjectEventVerb::BeDealtDamage(damage) => {
            // One event per damaged object per damage event (CR 120.3, 603.2c).
            cursor.require(
                !per_event,
                "a singular damaged-object subject (the runtime has no one-or-more matcher for permanents dealt damage)",
            )?;
            cursor.require(
                subject.query.controller == Some(ObjectControllerScope::AbilityController)
                    && subject
                        .query
                        .predicate
                        .supports_domain(ObjectKind::Permanent),
                "permanents the ability controller controls",
            )?;
            let mut lowered = json!({
                "kind": "controlledPermanentDealtDamage",
                "player": controller(),
                "where": lower_permanent_object_predicate(&subject.query.predicate),
            });
            apply_damage_event_kind(cursor, &mut lowered, damage)?;
            Ok(lowered)
        }
        ObjectEventVerb::Enter | ObjectEventVerb::Die => {
            unreachable!("entry and death are lowered by the shared transition branch")
        }
    }
}

/// The runtime distinguishes only `noncombatOnly`; a combat-only observer has
/// no matcher, so it stays an explicit failure rather than widening to any damage.
fn apply_damage_event_kind(
    cursor: &mut OracleTokenCursor<'_>,
    lowered: &mut Value,
    damage: DamageEventKind,
) -> MachineResult<()> {
    cursor.enter("trigger.dealt-damage.kind-support");
    match damage {
        DamageEventKind::Any => {}
        DamageEventKind::Noncombat => lowered["noncombatOnly"] = json!(true),
        DamageEventKind::Combat => {
            return Err(cursor.fail(
                "any or noncombat damage (the runtime has no combat-only received-damage matcher)",
            ));
        }
    }
    Ok(())
}

/// Only permanent-domain identity and token status survive this existing
/// departure observer unchanged. Characteristics require an effective LKI
/// snapshot and remain explicitly unsupported; the full predicate is retained.
fn stable_permanent_event_predicate(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Permanent) | ObjectPredicate::IsToken => true,
        ObjectPredicate::Not(child) => stable_permanent_event_predicate(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            !children.is_empty() && children.iter().all(stable_permanent_event_predicate)
        }
        _ => false,
    }
}

fn require_stable_permanent_event_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    predicate: &ObjectPredicate,
) -> MachineResult<()> {
    if predicate.contains_numeric_comparison() {
        cursor.enter("trigger.sacrifice.numeric-snapshot-support");
        return Err(cursor.fail("a predicate without numeric sacrificed-object characteristics until the LKI filter is proved"));
    }
    cursor.enter("trigger.sacrifice.characteristic-snapshot-support");
    cursor.require(stable_permanent_event_predicate(predicate)
        && domain_filter_is_representable(predicate),
        "a stable permanent/token identity predicate until effective object characteristics are captured")
}

/// Shared by the generic event parent and the established sacrifice adapter.
pub(in crate::oracle::canonical) fn lower_sacrifice_event(
    cursor: &mut OracleTokenCursor<'_>,
    event: &SacrificeEventNode,
) -> MachineResult<Value> {
    if event.actor == EventActorScope::Opponent {
        cursor.enter("trigger.sacrifice.actor-scope-support");
        return Err(
            cursor.fail("an accepted shared sacrifice matcher for the opponent actor scope")
        );
    }
    require_stable_permanent_event_predicate(cursor, &event.object.predicate)?;
    let mut lowered = json!({"kind":"permanentSacrificed", "where":lower_permanent_object_predicate(&event.object.predicate)});
    if event.actor == EventActorScope::Controller {
        lowered["player"] = controller();
    }
    if event.exclude_source {
        lowered["excludeSource"] = json!(true);
    }
    Ok(lowered)
}

fn lower_transition_branch(
    cursor: &mut OracleTokenCursor<'_>,
    transition: PermanentTransitionKind,
    branch: &PermanentDiedEventBranchNode,
) -> MachineResult<Value> {
    let kind = match transition {
        PermanentTransitionKind::Entered => "permanentEntered",
        PermanentTransitionKind::Died => "permanentDied",
    };
    match branch {
        PermanentDiedEventBranchNode::Source(LocalObjectReference::SourceSelf) => {
            let kind = if transition == PermanentTransitionKind::Entered {
                "enterBattlefield"
            } else {
                kind
            };
            Ok(json!({"kind": kind, "object": self_ref()}))
        }
        PermanentDiedEventBranchNode::Filtered {
            predicate,
            controller: scope,
            exclude_source,
        } => {
            let mut lowered =
                json!({"kind": kind, "where": lower_permanent_object_predicate(predicate)});
            match scope {
                Some(ObjectControllerScope::AbilityController) => lowered["player"] = controller(),
                Some(ObjectControllerScope::OpponentOfAbilityController) => {
                    lowered["player"] = json!({"kind": "opponentsOf", "player": controller()});
                }
                Some(ObjectControllerScope::NotAbilityController) => {
                    cursor.enter("trigger.program.event-controller-support");
                    return Err(
                        cursor.fail("a supported positive permanent-event controller scope")
                    );
                }
                None if transition == PermanentTransitionKind::Entered => {
                    lowered["anyController"] = json!(true);
                }
                None => {}
            }
            if *exclude_source {
                lowered["excludeSource"] = json!(true);
            }
            Ok(lowered)
        }
    }
}

fn lower_local_event(
    cursor: &mut OracleTokenCursor<'_>,
    event: &LocalTriggerEventNode,
) -> MachineResult<Value> {
    match event {
        LocalTriggerEventNode::SourceEntered { source } => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf,
                "the current entering source",
            )?;
            Ok(json!({"kind": "enterBattlefield", "object": self_ref()}))
        }
        LocalTriggerEventNode::SourceAttacked { source } => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf,
                "the current attacking source",
            )?;
            Ok(json!({"kind": "declaredAttacker", "object": self_ref()}))
        }
        LocalTriggerEventNode::CombatDamageToPlayer { source, amount } => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf
                    && *amount == EventNumericBinding::DamageAmount,
                "the current damage source and the runtime damageAmount event binding",
            )?;
            Ok(json!({"kind": "combatDamageToPlayer", "source": self_ref()}))
        }
        LocalTriggerEventNode::SourceDealtDamage {
            source,
            damage,
            amount,
        } => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf
                    && *amount == EventNumericBinding::DamageAmount,
                "the current damaged source and the runtime damageAmount event binding",
            )?;
            let mut lowered = json!({"kind": "permanentDealtDamage", "object": self_ref()});
            apply_damage_event_kind(cursor, &mut lowered, *damage)?;
            Ok(lowered)
        }
        LocalTriggerEventNode::LandPlayed {
            player,
            exclude_source,
        } => {
            cursor.enter("trigger.program.land-play.player-support");
            cursor.require(
                *player == LocalPlayerReference::Controller,
                "the source controller as land-playing player",
            )?;
            let mut lowered = json!({"kind": "landPlayed", "player": controller()});
            if *exclude_source {
                lowered["excludeSource"] = json!(true);
            }
            Ok(lowered)
        }
        LocalTriggerEventNode::CardDrawn { player, exception }
            if *player == PlayerPredicate::Opponent
                && *exception == Some(DrawEventException::FirstInEachDrawStep) =>
        {
            Ok(
                json!({"kind": "cardDrawn", "opponentOfSourceController": true, "exceptFirstInDrawStep": true}),
            )
        }
        LocalTriggerEventNode::OrdinalCardDrawn {
            player: LocalPlayerReference::Controller,
            ordinal,
            period: DrawPeriod::Turn,
        } if *ordinal > 0 => Ok(json!({
            "kind": "cardDrawn",
            "player": controller(),
            "drawOrdinal": ordinal,
        })),
        LocalTriggerEventNode::CardDrawn {
            player,
            exception: None,
        } => Ok(match player {
            PlayerPredicate::Any => json!({"kind": "cardDrawn", "anyPlayer": true}),
            PlayerPredicate::Opponent => {
                json!({"kind": "cardDrawn", "opponentOfSourceController": true})
            }
        }),
        LocalTriggerEventNode::PlayerEvent {
            actor,
            action,
            one_or_more,
        } => lower_player_event(cursor, *actor, *action, *one_or_more),
        LocalTriggerEventNode::CardDrawn { .. }
        | LocalTriggerEventNode::OrdinalCardDrawn { .. } => {
            cursor.enter("trigger.program.local-event-support");
            Err(cursor.fail(
                "a local draw event with an inspected and accepted runtime player/ordinal contract",
            ))
        }
    }
}

/// Maps one player-event verb and actor to the runtime event that already
/// implements it. Combinations without an inspected runtime matcher fail
/// explicitly instead of widening the actor.
fn lower_player_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    action: PlayerEventAction,
    one_or_more: bool,
) -> MachineResult<Value> {
    let player = match actor {
        EventActorScope::Controller => controller(),
        EventActorScope::Opponent => json!({"kind": "opponentsOf", "player": controller()}),
        EventActorScope::AnyPlayer => json!({"kind": "eachPlayer"}),
    };
    cursor.enter("trigger.program.player-event-support");
    if let PlayerEventAction::BeDealtDamage(damage) = action {
        // The runtime observes damage dealt to the controller's opponents,
        // per recipient or as one event for the whole batch.
        cursor.require(
            actor == EventActorScope::Opponent,
            "an opponent actor for a received-damage event (the runtime matches only opponents)",
        )?;
        let mut lowered = json!({
            "kind": "opponentDealtDamage",
            "player": player,
            "aggregation": if one_or_more { "oneOrMore" } else { "perRecipient" },
        });
        apply_damage_event_kind(cursor, &mut lowered, damage)?;
        return Ok(lowered);
    }
    cursor.require(
        !one_or_more,
        "a per-player event (a one-or-more actor is proved only for received damage)",
    )?;
    match (action, actor) {
        (PlayerEventAction::GainLife, _) => Ok(json!({"kind": "lifeGained", "player": player})),
        (PlayerEventAction::LoseLife, _) => Ok(json!({"kind": "lifeLost", "player": player})),
        (PlayerEventAction::DrawCard, EventActorScope::Controller) => {
            Ok(json!({"kind": "cardDrawn", "player": player}))
        }
        (PlayerEventAction::DrawCard, EventActorScope::Opponent) => {
            Ok(json!({"kind": "cardDrawn", "opponentOfSourceController": true}))
        }
        (PlayerEventAction::DrawCard, EventActorScope::AnyPlayer) => {
            Ok(json!({"kind": "cardDrawn", "anyPlayer": true}))
        }
        (PlayerEventAction::Attack, EventActorScope::Controller) => {
            Ok(json!({"kind": "controlledCreaturesAttacked", "player": player}))
        }
        (PlayerEventAction::FaceDilemma, EventActorScope::Controller) => {
            Ok(json!({"kind": "dilemmaFaced", "player": player}))
        }
        (PlayerEventAction::Attack | PlayerEventAction::FaceDilemma, _) => Err(cursor.fail(
            "a controller actor for this player event (the runtime matches only the source controller)",
        )),
        (PlayerEventAction::BeDealtDamage(_), _) => {
            unreachable!("received damage is lowered before the per-player verbs")
        }
    }
}

#[cfg(test)]
#[path = "dealt_damage_event_tests.rs"]
mod dealt_damage_event_tests;
