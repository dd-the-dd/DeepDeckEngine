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
            // Numeric characteristics are read on the cast record: mana
            // value with the announced X (CR 202.3e), the card's power and
            // toughness. The right side must be a literal (no other producer).
            if event.query.predicate.contains_numeric_comparison() {
                cursor.enter("trigger.spell-cast.numeric-stack-context");
                cursor.require(
                    literal_numeric_comparisons(&event.query.predicate),
                    "a spell characteristic compared with a literal number",
                )?;
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
                // A matching-spell ordinal counts the caster's logged spells
                // this turn that satisfy the same predicate; the log keeps
                // type line and name only.
                cursor.enter("trigger.spell-cast.ordinal-basis-support");
                cursor.require(
                    ordinal.basis == SpellCastOrdinalBasis::AllSpellsByCasterThisTurn
                        || spell_history_predicate(&event.query.predicate),
                    "an ordinal over spells whose type line or name the spell history records",
                )?;
                lowered["spellCastOrdinal"] = json!(ordinal.value);
            }
            if event.source_spell {
                // CR 601.2i: the cast spell's own ability, from the stack.
                lowered["sourceIsSelf"] = json!(true);
            }
            if let Some(targets) = &event.targets {
                lowered["targets"] = lower_spell_targets(cursor, targets)?;
            }
            // The cast record carries the zone the spell was cast from.
            if let Some(origin) = event.origin {
                let field = if origin.excluded {
                    "fromZoneNot"
                } else {
                    "fromZone"
                };
                lowered[field] = json!(origin.zone);
                if origin.own_zone && !origin.excluded {
                    lowered["fromOwnZone"] = json!(true);
                }
            }
            match event.turn {
                Some(SpellTurnNode::ControllerTurn) => {
                    lowered["duringControllerTurn"] = json!(true)
                }
                Some(SpellTurnNode::OpponentTurn) => lowered["duringOpponentTurn"] = json!(true),
                None => {}
            }
            if event.also_copy {
                cursor.enter("trigger.spell-cast.copy-support");
                cursor.require(
                    event.caster == SpellCastEventCaster::Controller && event.ordinal.is_none(),
                    "a spell copied by the ability controller (the copy observer's only player)",
                )?;
                let copied = json!({"kind": "spellCopied", "player": controller(), "where": lowered["where"].clone()});
                return Ok(json!({"kind": "oneOf", "events": [lowered, copied]}));
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
        GenericTriggerEventNode::Step(event) => Ok(lower_step_event(*event)),
        GenericTriggerEventNode::PermanentTransition {
            kind,
            subjects,
            entry_state,
            origin,
            union_kind,
        } => {
            cursor.require(
                !subjects.alternatives.is_empty(),
                "one or more permanent event subjects",
            )?;
            let mut events = Vec::new();
            for transition in std::iter::once(*kind).chain(*union_kind) {
                for subject in &subjects.alternatives {
                    let mut lowered = lower_transition_branch(cursor, transition, subject)?;
                    if let Some(state) = entry_state
                        && transition == PermanentTransitionKind::Entered
                    {
                        lowered["entryState"] =
                            json!({"tapped":*state==EntryStatePredicate::Tapped});
                    }
                    if let Some(origin) = origin
                        && transition == PermanentTransitionKind::Entered
                    {
                        lowered["fromZone"] = json!(origin.zone);
                    }
                    events.push(lowered);
                }
            }
            if entry_state.is_some() {
                cursor.require(
                    *kind == PermanentTransitionKind::Entered,
                    "an entry observation only on a battlefield-entry event",
                )?;
            }
            if events.len() == 1 {
                Ok(events.into_iter().next().expect("one validated subject"))
            } else {
                Ok(json!({"kind": "oneOf", "events": events}))
            }
        }
        GenericTriggerEventNode::Local(event) => lower_local_event(cursor, event),
        GenericTriggerEventNode::Object(event) => lower_object_event(cursor, event),
        GenericTriggerEventNode::SourceEvent(event) => lower_source_event_union(cursor, event),
        GenericTriggerEventNode::CounterPlacement(event) => {
            lower_counter_placement_event(cursor, event)
        }
        GenericTriggerEventNode::BecameTarget(event) => Ok(lower_became_target_event(event)),
        GenericTriggerEventNode::Compound(members) => lower_compound_trigger_event(cursor, members),
        GenericTriggerEventNode::Conditioned { .. } => {
            // A single occurrence's condition belongs to the ability
            // (`eventCondition`); the envelope hoists it before lowering.
            cursor.enter("trigger.event-condition.owner");
            Err(cursor.fail("an event-time condition owned by the triggered ability"))
        }
        GenericTriggerEventNode::SourceCounterState(event) => {
            lower_source_counter_state_event(cursor, event)
        }
    }
}

/// Event kinds whose matcher evaluates a member's own `eventCondition` as
/// the occurrence is observed. A compound member with an event-time
/// condition of another kind has no evaluator: the publication of a list
/// does not know which member matched.
const MEMBER_EVENT_CONDITION_KINDS: [&str; 2] = ["controlledCreaturesAttacked", "turnedFaceUp"];

/// A trigger condition list (CR 603.2): every member through the same
/// event lowering, flattened into one `oneOf` in written order. The rule
/// triggers once for each occurrence that matches any member.
fn lower_compound_trigger_event(
    cursor: &mut OracleTokenCursor<'_>,
    members: &[GenericTriggerEventNode],
) -> MachineResult<Value> {
    cursor.enter("trigger.compound.members");
    cursor.require(
        members.len() >= 2,
        "at least two trigger conditions in a list",
    )?;
    let mut events = Vec::new();
    for member in members {
        let (event, condition) = match member {
            GenericTriggerEventNode::Conditioned { event, condition } => {
                (event.as_ref(), Some(condition))
            }
            event => (event, None),
        };
        cursor.require(
            !matches!(event, GenericTriggerEventNode::Compound(_)),
            "a flat trigger condition list",
        )?;
        let mut lowered = lower_generic_trigger_event(cursor, event)?;
        if let Some(condition) = condition {
            let kind = lowered["kind"].as_str().unwrap_or_default();
            if !MEMBER_EVENT_CONDITION_KINDS.contains(&kind) {
                return Err(engine_gap(
                    cursor,
                    "an engine matcher that evaluates this member's event-time condition",
                ));
            }
            lowered["eventCondition"] = lower_event_time_condition(cursor, condition)?;
        }
        if lowered["kind"] == "oneOf" {
            let members = lowered["events"]
                .as_array()
                .cloned()
                .ok_or_else(|| cursor.fail("a lowered event union"))?;
            events.extend(members);
        } else {
            events.push(lowered);
        }
    }
    Ok(json!({"kind": "oneOf", "events": events}))
}

/// The event-time condition (CR 603.2): the turn owner and the `while`
/// condition, each lowered by the shared condition lowering, evaluated once
/// as the event occurs. Never the intervening `if` (CR 603.4).
pub(in crate::oracle::canonical) fn lower_event_time_condition(
    cursor: &mut OracleTokenCursor<'_>,
    node: &EventTimeConditionNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.event-condition.lowering");
    let mut operands = Vec::new();
    match node.turn_owner {
        Some(EventTurnOwner::ControllerTurn) => {
            operands.push(json!({"kind": "duringControllerTurn", "player": controller()}));
        }
        Some(EventTurnOwner::OpponentTurn) => operands.push(json!({"kind": "notYourTurn"})),
        None => {}
    }
    if let Some(condition) = &node.condition {
        operands.push(lower_sentence_condition(
            cursor,
            condition,
            &ConditionObjectBindings {
                permanent_targets: &[],
                card_targets: &[],
                card_focus: None,
                previous_action_result: None,
            },
        )?);
    }
    match operands.len() {
        0 => Err(cursor.fail("a turn owner or a `while` condition")),
        1 => Ok(operands.pop().expect("one operand")),
        _ => Ok(json!({"kind": "and", "operands": operands})),
    }
}

/// `SOURCE has no|N or more COUNTERS on it` (CR 603.8): the engine's state
/// trigger, whose condition compares the source's counter count.
fn lower_source_counter_state_event(
    cursor: &mut OracleTokenCursor<'_>,
    event: &SourceCounterStateNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.counter-state.source-support");
    cursor.require(
        event.source == LocalObjectReference::SourceSelf,
        "the current source as the counter holder",
    )?;
    let (operator, value) = match event.comparison {
        CounterStateComparison::None => ("==", 0),
        CounterStateComparison::AtLeast(minimum) => (">=", minimum),
        CounterStateComparison::AtMost(maximum) => ("<=", maximum),
        CounterStateComparison::Exactly(value) => ("==", value),
    };
    Ok(json!({
        "kind": "stateConditionMet",
        "condition": {
            "kind": "compare",
            "left": {"kind": "countCounters", "object": self_ref(), "counter": event.counter.as_str()},
            "operator": operator,
            "right": {"kind": "integer", "value": value},
        },
    }))
}

/// One placement of one counter kind on one permanent is one runtime event
/// (`countersPlaced`); a `one or more` trigger observes it once per permanent
/// (CR 122.6, 603.2c). Unproved scopes fail instead of widening the matcher.
fn lower_counter_placement_event(
    cursor: &mut OracleTokenCursor<'_>,
    event: &CounterPlacementEventNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.counters.kind-support");
    // The runtime publishes each counter kind separately, so an any-kind
    // batch could trigger once per kind of one simultaneous placement.
    let counter = event.counter.as_ref().ok_or_else(|| {
        cursor.fail("a named counter kind (no any-kind placement batch is proved)")
    })?;
    let mut lowered = json!({
        "kind": "countersPlaced",
        "player": controller(),
        "counter": counter.as_str(),
    });
    cursor.enter("trigger.counters.placer-support");
    match event.placed_by {
        None => {}
        Some(EventActorScope::Controller) => lowered["placedBy"] = controller(),
        Some(_) => {
            return Err(cursor.fail(
                "the ability controller as placing player (the runtime matches only that placer)",
            ));
        }
    }
    cursor.enter("trigger.counters.recipient-support");
    match &event.recipient {
        CounterRecipient::Source(source) => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf,
                "the current source as counter recipient",
            )?;
            lowered["object"] = self_ref();
        }
        CounterRecipient::Query {
            query,
            exclude_source,
        } => {
            cursor.require(
                !query.predicate.contains_numeric_comparison()
                    && domain_filter_is_representable(&query.predicate),
                "a representable recipient predicate without numeric characteristics",
            )?;
            let filter = lower_permanent_object_predicate(&query.predicate);
            cursor.require(!filter.is_null(), "a lowered recipient predicate")?;
            lowered["where"] = filter;
            match query.controller {
                Some(ObjectControllerScope::AbilityController) => {}
                None => lowered["anyController"] = json!(true),
                Some(_) => {
                    return Err(cursor.fail(
                        "a recipient you control or any recipient (no other controller matcher)",
                    ));
                }
            }
            if *exclude_source {
                lowered["excludeSource"] = json!(true);
            }
        }
    }
    Ok(lowered)
}

/// Each source verb maps to the runtime matcher that observes the source
/// object itself; a union becomes `oneOf` in written order. Verbs without an
/// inspected self matcher fail explicitly instead of widening the subject.
fn lower_source_event_union(
    cursor: &mut OracleTokenCursor<'_>,
    event: &SourceEventUnionNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.source-event.subject-support");
    cursor.require(
        event.source == LocalObjectReference::SourceSelf && !event.verbs.is_empty(),
        "the current source as event subject",
    )?;
    // A modified verb is a generic event pattern (docs/oracle-event-model.md);
    // a modifier the runtime matcher cannot evaluate is an engine gap,
    // reported apart from grammar and never dropped.
    if !event.modifiers.is_empty() {
        return lower_source_event_patterns(cursor, event);
    }
    cursor.enter("trigger.source-event.verb-support");
    let events = event
        .verbs
        .iter()
        .map(|verb| {
            let object = self_ref();
            Ok(match verb {
                SourceEventVerb::Entered => json!({"kind": "enterBattlefield", "object": object}),
                SourceEventVerb::Died => json!({"kind": "permanentDied", "object": object}),
                SourceEventVerb::Attacked => json!({"kind": "declaredAttacker", "object": object}),
                SourceEventVerb::Blocked => json!({"kind": "declaredBlocker", "object": object}),
                SourceEventVerb::BecameTapped => {
                    json!({"kind": "permanentTapped", "object": object})
                }
                // CR 603.10a: leaves-the-battlefield abilities look back.
                SourceEventVerb::LeftBattlefield => {
                    json!({"kind": "permanentLeftBattlefield", "object": object})
                }
                // A card's own `from anywhere` ability, matched as it arrives.
                SourceEventVerb::PutIntoGraveyardFromAnywhere => json!({
                    "kind": "eventPattern",
                    "lemma": "move",
                    "subject": {"object": object},
                    "to": "graveyard",
                }),
                SourceEventVerb::PutIntoGraveyardFromZone(zone) => json!({
                    "kind": "eventPattern",
                    "lemma": "move",
                    "subject": {"object": object},
                    "from": zone,
                    "to": "graveyard",
                }),
                // CR 603.10a: look-back on the exiled source.
                SourceEventVerb::PutIntoExileFromBattlefield => json!({
                    "kind": "permanentLeftBattlefield",
                    "object": object,
                    "destination": "exile",
                }),
                SourceEventVerb::PutIntoGraveyardFromBattlefield => json!({
                    "kind": "permanentLeftBattlefield",
                    "object": object,
                    "destination": "graveyard",
                }),
                // CR 509.1h: once per combat, when the first blocker is declared.
                SourceEventVerb::BecameBlocked => {
                    json!({"kind": "permanentBecameBlocked", "object": object})
                }
                // CR 708.8: the face-down source is turned face up.
                SourceEventVerb::TurnedFaceUp => {
                    json!({"kind": "turnedFaceUp", "object": object})
                }
                // CR 701.26b: the source becomes untapped, whatever untaps it.
                SourceEventVerb::BecameUntapped => {
                    json!({"kind": "permanentUntapped", "object": object})
                }
                // Read only with its counterpart, as an event pattern.
                SourceEventVerb::Exploited => {
                    return Err(cursor.fail("`exploits` with the exploited object"));
                }
                // Read only with its targeting stack object (a modifier).
                SourceEventVerb::BecameTarget => {
                    return Err(cursor.fail("`becomes the target of` with its stack object"));
                }
            })
        })
        .collect::<MachineResult<Vec<_>>>()?;
    Ok(if events.len() == 1 {
        events.into_iter().next().expect("one source event")
    } else {
        json!({"kind": "oneOf", "events": events})
    })
}

fn predicate_reads_ownership(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::OwnedByAbilityController { .. } => true,
        ObjectPredicate::Not(operand) => predicate_reads_ownership(operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
            operands.iter().any(predicate_reads_ownership)
        }
        _ => false,
    }
}

fn engine_gap(cursor: &mut OracleTokenCursor<'_>, expected: &str) -> MachineFailure {
    cursor.enter("trigger.event.engine-gap");
    cursor.engine_gap(
        crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
        expected,
    )
}

/// `during your turn` / `during an opponent's turn` on a combat pattern: the
/// record's turn owner relative to the ability controller (CR 500.1). The
/// field names are the cast pattern's (`cast_events.rs`).
fn apply_turn_owner(pattern: &mut Value, owner: Option<EventTurnOwner>) {
    match owner {
        Some(EventTurnOwner::ControllerTurn) => pattern["duringControllerTurn"] = json!(true),
        Some(EventTurnOwner::OpponentTurn) => pattern["duringOpponentTurn"] = json!(true),
        None => {}
    }
}

/// The modifiers of a quantified object event that its lowering does not
/// evaluate: a recognized modifier is an engine gap, never dropped.
fn reject_object_event_modifiers(
    cursor: &mut OracleTokenCursor<'_>,
    event: &ObjectEventNode,
) -> MachineResult<()> {
    if event.alone {
        return Err(engine_gap(
            cursor,
            "an engine event matcher for `alone` on this object event",
        ));
    }
    match event.modifiers.first_modifier_name() {
        Some(name) => Err(engine_gap(
            cursor,
            &format!("an engine event matcher for {name} on this object event"),
        )),
        None => Ok(()),
    }
}

/// The source as the subject of a combat event pattern, with the typed
/// modifiers the runtime matcher evaluates: `alone` (CR 506.5), `and isn't
/// blocked` (CR 509.1h), a block counterpart (CR 509.3b, 509.3d) and the
/// per-turn ordinal of attacks and blocks.
fn lower_source_event_patterns(
    cursor: &mut OracleTokenCursor<'_>,
    event: &SourceEventUnionNode,
) -> MachineResult<Value> {
    let first_time = event
        .modifiers
        .contains(&SourceEventModifier::FirstTimeEachTurn);
    let not_blocked = event.modifiers.contains(&SourceEventModifier::NotBlocked);
    let turn_owner = event.modifiers.iter().find_map(|modifier| match modifier {
        SourceEventModifier::DuringTurn(owner) => Some(*owner),
        _ => None,
    });
    if not_blocked && event.verbs != [SourceEventVerb::Attacked] {
        return Err(engine_gap(
            cursor,
            "an engine event matcher for `and isn't blocked` after an event other than one attack",
        ));
    }
    let mut events = Vec::new();
    for verb in &event.verbs {
        // The source entering from a written zone: the entry matcher reads
        // the origin the engine recorded on the entering object.
        if *verb == SourceEventVerb::Entered
            && let Some(origin) = event.modifiers.iter().find_map(|modifier| match modifier {
                SourceEventModifier::EnteredFrom(origin) => Some(origin),
                _ => None,
            })
        {
            if first_time {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for `for the first time each turn` on an entry",
                ));
            }
            events.push(json!({
                "kind": "enterBattlefield",
                "object": self_ref(),
                "fromZone": origin.zone,
            }));
            continue;
        }
        // CR 115.1: the source as the targeted object, observed by the
        // generic targeting matcher with the stack-object selector.
        if *verb == SourceEventVerb::BecameTarget {
            let stack = event
                .modifiers
                .iter()
                .find_map(|modifier| match modifier {
                    SourceEventModifier::TargetingStack(stack) => Some(stack),
                    _ => None,
                })
                .ok_or_else(|| cursor.fail("`becomes the target of` with its stack object"))?;
            if turn_owner.is_some() {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for a targeting event with a turn owner",
                ));
            }
            let mut lowered = lower_source_became_target_event(stack);
            if first_time {
                // The runtime counts the logged targeting events of the
                // source whose stack object satisfied the same selector.
                lowered["turnOrdinal"] = json!(1);
            }
            events.push(lowered);
            continue;
        }
        let alone = *verb == SourceEventVerb::Attacked
            && event
                .modifiers
                .contains(&SourceEventModifier::AttackingAlone);
        let counterpart = event.modifiers.iter().find_map(|modifier| match modifier {
            SourceEventModifier::Counterpart(node) if node.verb == *verb => Some(node),
            _ => None,
        });
        // CR 702.110b: the keyword action published when the source's exploit
        // ability sacrifices the counterpart (src/engine/keyword_actions.rs).
        if *verb == SourceEventVerb::Exploited {
            let counterpart =
                counterpart.ok_or_else(|| cursor.fail("`exploits` with the exploited object"))?;
            if first_time
                || turn_owner.is_some()
                || counterpart.one_or_more
                || counterpart.query.controller.is_some()
                || counterpart.query.predicate.contains_numeric_comparison()
                || !domain_filter_is_representable(&counterpart.query.predicate)
            {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for an exploited object without controller, numeric characteristics or a turn ordinal",
                ));
            }
            let filter = lower_permanent_object_predicate(&counterpart.query.predicate);
            if filter.is_null() {
                return Err(engine_gap(cursor, "a lowered exploited-object predicate"));
            }
            events.push(json!({
                "kind": "eventPattern",
                "lemma": "exploit",
                "subject": {"object": self_ref()},
                "counterpart": {"where": filter},
            }));
            continue;
        }
        let (lemma, role) = match verb {
            SourceEventVerb::Attacked if not_blocked => ("becomeUnblocked", "attacker"),
            SourceEventVerb::Attacked => ("attack", "attacker"),
            SourceEventVerb::Blocked => ("block", "blocker"),
            SourceEventVerb::BecameBlocked => ("block", "attacker"),
            _ if first_time => {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for `for the first time each turn` on an attack or a block",
                ));
            }
            _ if turn_owner.is_some() => {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for a turn owner on an event other than an attack or a block",
                ));
            }
            // An unmodified verb of a modified union keeps its own matcher.
            _ => {
                events.push(lower_source_event_union(
                    cursor,
                    &SourceEventUnionNode {
                        source: event.source,
                        verbs: vec![*verb],
                        modifiers: Vec::new(),
                    },
                )?);
                continue;
            }
        };
        if !alone && counterpart.is_none() && !first_time && !not_blocked && turn_owner.is_none() {
            events.push(lower_source_event_union(
                cursor,
                &SourceEventUnionNode {
                    source: event.source,
                    verbs: vec![*verb],
                    modifiers: Vec::new(),
                },
            )?);
            continue;
        }
        let mut pattern = json!({
            "kind": "eventPattern",
            "lemma": lemma,
            "subject": {"role": role, "object": self_ref()},
        });
        if alone {
            pattern["attackingAlone"] = json!(true);
        }
        if not_blocked {
            pattern["declaredAttacker"] = json!(true);
        }
        if first_time {
            // The runtime records the ordinal of the attacker of an attack and
            // of both combatants of a block (the attacker counts the combats
            // it became blocked in); an unblocked record carries none. Per
            // pair, `becomes blocked by X` has no first-pair ordinal.
            if not_blocked || (*verb == SourceEventVerb::BecameBlocked && counterpart.is_some()) {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for `for the first time each turn` on an attack or a block",
                ));
            }
            pattern["turnOrdinal"] = json!(1);
        }
        apply_turn_owner(&mut pattern, turn_owner);
        if let Some(counterpart) = counterpart {
            if counterpart.query.controller.is_some()
                || counterpart.query.predicate.contains_numeric_comparison()
                || !domain_filter_is_representable(&counterpart.query.predicate)
            {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for a block counterpart without controller or numeric characteristics",
                ));
            }
            let filter = lower_permanent_object_predicate(&counterpart.query.predicate);
            if filter.is_null() {
                return Err(engine_gap(cursor, "a lowered block counterpart predicate"));
            }
            let mut lowered = json!({
                "role": if role == "attacker" { "blocker" } else { "attacker" },
                "where": filter,
            });
            if counterpart.one_or_more {
                lowered["oneOrMore"] = json!(true);
            }
            pattern["counterpart"] = lowered;
        }
        events.push(pattern);
    }
    Ok(if events.len() == 1 {
        events.into_iter().next().expect("one source event")
    } else {
        json!({"kind": "oneOf", "events": events})
    })
}

/// A departure of a permanent with any controller, or an opponent's: the
/// generic zone-change pattern, read on the last-known object (CR 603.10a).
fn lower_departure_move_pattern(
    cursor: &mut OracleTokenCursor<'_>,
    event: &ObjectEventNode,
) -> MachineResult<Value> {
    let subject = &event.subject;
    cursor.require(
        !subject.query.predicate.contains_numeric_comparison()
            && domain_filter_is_representable(&subject.query.predicate),
        "a representable departing-permanent predicate without numeric characteristics",
    )?;
    let mut object = json!({"kind": "permanents"});
    match subject.query.controller {
        None => {}
        Some(ObjectControllerScope::AbilityController) => object["controller"] = controller(),
        Some(ObjectControllerScope::OpponentOfAbilityController) => {
            object["controller"] = json!({"kind": "opponentsOf", "player": controller()});
        }
        Some(_) => {
            return Err(cursor.fail("a departing permanent with any controller or an opponent"));
        }
    }
    if subject.query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Permanent) {
        let filter = lower_permanent_object_predicate(&subject.query.predicate);
        cursor.require(!filter.is_null(), "a lowered departing-permanent predicate")?;
        object["where"] = filter;
    }
    if subject.exclude_source {
        object["excludeSource"] = json!(true);
    }
    let mut lowered = json!({
        "kind": "eventPattern",
        "lemma": "move",
        "subject": {"object": object},
        "from": "battlefield",
    });
    if event.verb == ObjectEventVerb::PutIntoGraveyardFromBattlefield {
        lowered["to"] = json!("graveyard");
    }
    // `one or more`: one trigger per simultaneous event (CR 603.2c).
    if matches!(
        event.subject.multiplicity,
        EventMultiplicity::PerEvent { .. }
    ) {
        lowered["aggregation"] = json!("oneOrMore");
    }
    Ok(lowered)
}

/// A card arriving in a graveyard from any zone: the generic move pattern
/// read on the card in its new zone; `one or more` is one trigger per
/// simultaneous event (CR 603.2c).
fn lower_graveyard_arrival_pattern(
    cursor: &mut OracleTokenCursor<'_>,
    event: &ObjectEventNode,
    owner: GraveyardOwner,
    origin: Option<&'static str>,
) -> MachineResult<Value> {
    let subject = &event.subject;
    cursor.enter("trigger.object-event.graveyard-arrival");
    cursor.require(
        subject.query.controller.is_none()
            && !subject.exclude_source
            && subject.query.predicate.supports_domain(ObjectKind::Card)
            && !subject.query.predicate.contains_numeric_comparison()
            && domain_filter_is_representable(&subject.query.predicate),
        "a representable card predicate without controller or source exclusion",
    )?;
    let mut object = json!({"kind": "cards"});
    match owner {
        GraveyardOwner::Any => {}
        GraveyardOwner::Controller => object["owner"] = controller(),
        GraveyardOwner::Opponent => {
            object["owner"] = json!({"kind": "opponentsOf", "player": controller()});
        }
    }
    if subject.query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Card) {
        let filter = lower_card_object_predicate(&subject.query.predicate);
        cursor.require(!filter.is_null(), "a lowered arriving-card predicate")?;
        object["where"] = filter;
    }
    let mut lowered = json!({
        "kind": "eventPattern",
        "lemma": "move",
        "subject": {"object": object},
        "to": "graveyard",
    });
    if let Some(origin) = origin {
        lowered["from"] = json!(origin);
    }
    if matches!(subject.multiplicity, EventMultiplicity::PerEvent { .. }) {
        lowered["aggregation"] = json!("oneOrMore");
    }
    Ok(lowered)
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
        // The attack-declaration matcher counts the declared attackers.
        cursor.require(
            minimum == 1 || event.verb == ObjectEventVerb::Attack,
            "a `one or more` threshold (no runtime matcher counts larger event batches)",
        )?;
    }
    if !event.attacked.is_empty() && event.verb != ObjectEventVerb::Attack {
        return Err(cursor.fail("an attacked recipient only after an attack"));
    }
    let per_event = matches!(subject.multiplicity, EventMultiplicity::PerEvent { .. });
    // Only the per-attacker pattern evaluates the clause modifiers (`alone`,
    // `and isn't blocked`, the turn ordinal and the turn owner).
    if event.verb != ObjectEventVerb::Attack || per_event {
        reject_object_event_modifiers(cursor, event)?;
    }
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
    if let ObjectEventVerb::LeaveBattlefield | ObjectEventVerb::PutIntoGraveyardFromBattlefield =
        event.verb
    {
        // The departure matcher observes one permanent as it last existed on
        // the battlefield, with an explicit source exclusion (CR 603.10a).
        cursor.enter("trigger.object-event.departure-support");
        cursor.require(
            subject
                .query
                .predicate
                .supports_domain(ObjectKind::Permanent),
            "a permanent leaving the battlefield",
        )?;
        if per_event || subject.query.controller != Some(ObjectControllerScope::AbilityController) {
            return lower_departure_move_pattern(cursor, event);
        }
        let mut lowered = json!({
            "kind": "controlledPermanentLeftBattlefield",
            "player": controller(),
            "where": lower_permanent_object_predicate(&subject.query.predicate),
            "excludeSource": subject.exclude_source,
        });
        if event.verb == ObjectEventVerb::PutIntoGraveyardFromBattlefield {
            lowered["destination"] = json!("graveyard");
        }
        return Ok(lowered);
    }
    if let ObjectEventVerb::PutIntoGraveyard(owner, origin) = event.verb {
        return lower_graveyard_arrival_pattern(cursor, event, owner, origin);
    }
    if event.verb == ObjectEventVerb::DealDamage
        || (event.verb == ObjectEventVerb::DealCombatDamageToPlayer && !per_event)
    {
        let established = DamageDealtComplement {
            kind: DamageKind::Combat,
            recipient: Some(DamageRecipient::Player(EventActorScope::AnyPlayer)),
        };
        let complement = event.damage.as_ref().unwrap_or(&established);
        // `one or more SOURCES deal combat damage to you`: the received-combat
        // observer groups one combat damage step per damaged player.
        if per_event
            && complement.kind == DamageKind::Combat
            && complement.recipient == Some(DamageRecipient::Player(EventActorScope::Controller))
            && subject.query.controller.is_none()
            && !subject.exclude_source
        {
            cursor.enter("trigger.dealt-damage.source-support");
            cursor.require(
                subject
                    .query
                    .predicate
                    .supports_domain(ObjectKind::Permanent)
                    && !subject.query.predicate.contains_numeric_comparison()
                    && domain_filter_is_representable(&subject.query.predicate),
                "a representable damage-source predicate",
            )?;
            let mut lowered = json!({"kind": "combatDamageReceived", "player": controller()});
            if subject.query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Permanent) {
                let filter = lower_permanent_object_predicate(&subject.query.predicate);
                cursor.require(!filter.is_null(), "a lowered damage-source predicate")?;
                lowered["where"] = filter;
            }
            return Ok(lowered);
        }
        let source = lower_damage_subject_query(cursor, subject)?;
        return lower_damage_dealt_event(cursor, source, complement);
    }
    cursor.enter("trigger.object-event.source-exclusion-support");
    // The generic attack and turned-face-up patterns carry their own source
    // exclusion.
    cursor.require(
        !subject.exclude_source
            || (matches!(
                event.verb,
                ObjectEventVerb::Attack | ObjectEventVerb::TurnedFaceUp
            ) && !per_event),
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
        // One trigger per attacking creature (CR 508.3a): the generic attack
        // pattern with the subject's controller relation and predicate.
        ObjectEventVerb::Attack if !per_event => {
            cursor.enter("trigger.object-event.attack-subject");
            cursor.require(
                subject
                    .query
                    .predicate
                    .supports_domain(ObjectKind::Permanent)
                    && domain_filter_is_representable(&subject.query.predicate),
                "a representable attacking-permanent predicate",
            )?;
            let mut object = json!({"kind": "permanents"});
            match subject.query.controller {
                None => {}
                Some(ObjectControllerScope::AbilityController) => {
                    object["controller"] = controller();
                }
                Some(ObjectControllerScope::OpponentOfAbilityController) => {
                    object["controller"] = json!({"kind": "opponentsOf", "player": controller()});
                }
                Some(
                    ObjectControllerScope::NotAbilityController
                    | ObjectControllerScope::EventPlayer,
                ) => {
                    return Err(cursor.fail("a positive controller relation for an attacker"));
                }
            }
            if subject.query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Permanent) {
                let filter = lower_permanent_object_predicate(&subject.query.predicate);
                cursor.require(!filter.is_null(), "a lowered attacking-permanent predicate")?;
                object["where"] = filter;
            }
            if subject.exclude_source {
                object["excludeSource"] = json!(true);
            }
            let modifiers = event.modifiers;
            // The unblocked record carries no per-turn ordinal.
            if modifiers.not_blocked && modifiers.first_time_each_turn {
                return Err(engine_gap(
                    cursor,
                    "an engine event matcher for `for the first time each turn` on an unblocked attacker",
                ));
            }
            // `attacks and isn't blocked`: the attacker became unblocked
            // (CR 509.1h); only a declared attacker attacked (CR 508.4).
            let lemma = if modifiers.not_blocked {
                "becomeUnblocked"
            } else {
                "attack"
            };
            let mut pattern = json!({
                "kind": "eventPattern",
                "lemma": lemma,
                "subject": {"role": "attacker", "object": object},
            });
            if event.alone {
                pattern["attackingAlone"] = json!(true);
            }
            if modifiers.not_blocked {
                pattern["declaredAttacker"] = json!(true);
            }
            if modifiers.first_time_each_turn {
                pattern["turnOrdinal"] = json!(1);
            }
            apply_turn_owner(&mut pattern, modifiers.turn_owner);
            if !event.attacked.is_empty() {
                pattern["attacked"] = lower_attacked_recipients(cursor, &event.attacked)?;
            }
            Ok(pattern)
        }
        ObjectEventVerb::TurnedFaceUp => {
            // One turned-face-up permanent per event (CR 708.8).
            cursor.enter("trigger.object-event.turned-face-up-subject");
            cursor.require(
                !per_event,
                "a singular turned-face-up subject (no one-or-more batch is proved)",
            )?;
            let mut object = json!({"kind": "permanents"});
            if let Some(filter) = lower_damage_permanent_query(cursor, &subject.query, &mut object)?
            {
                object["where"] = filter;
            }
            if subject.exclude_source {
                object["excludeSource"] = json!(true);
            }
            Ok(json!({"kind": "turnedFaceUp", "object": object}))
        }
        ObjectEventVerb::Attack | ObjectEventVerb::DealCombatDamageToPlayer => {
            cursor.require(
                per_event,
                "a `one or more` subject (per-object attack and damage matchers are separate)",
            )?;
            if !event.attacked.is_empty() {
                return Err(engine_gap(
                    cursor,
                    "an attack-declaration matcher with a per-attacker attacked recipient",
                ));
            }
            cursor.require(
                subject.query.controller == Some(ObjectControllerScope::AbilityController)
                    && subject
                        .query
                        .predicate
                        .supports_domain(ObjectKind::Permanent),
                "permanents the ability controller controls",
            )?;
            let filter = lower_permanent_object_predicate(&subject.query.predicate);
            let minimum = match subject.multiplicity {
                EventMultiplicity::PerEvent { minimum } => minimum,
                EventMultiplicity::PerObject => 1,
            };
            Ok(match event.verb {
                // One attack declaration is one event for all attackers; the
                // matcher counts the matching attackers (CR 508.1, 603.2c).
                ObjectEventVerb::Attack if minimum > 1 => json!({
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "where": filter,
                    "minimum": minimum,
                }),
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
        // `one or more PERMANENTS block / become blocked`: the generic block
        // pattern, once per declare-blockers event (CR 509.1, 603.2c).
        ObjectEventVerb::Block | ObjectEventVerb::BecomeBlocked if per_event => {
            cursor.enter("trigger.object-event.block-batch");
            cursor.require(
                subject
                    .query
                    .predicate
                    .supports_domain(ObjectKind::Permanent)
                    && !subject.exclude_source
                    && domain_filter_is_representable(&subject.query.predicate),
                "a representable combatant predicate without source exclusion",
            )?;
            let mut object = json!({"kind": "permanents"});
            match subject.query.controller {
                None => {}
                Some(ObjectControllerScope::AbilityController) => {
                    object["controller"] = controller()
                }
                Some(ObjectControllerScope::OpponentOfAbilityController) => {
                    object["controller"] = json!({"kind": "opponentsOf", "player": controller()});
                }
                Some(
                    ObjectControllerScope::NotAbilityController
                    | ObjectControllerScope::EventPlayer,
                ) => {
                    return Err(cursor.fail("a positive controller relation for a combatant"));
                }
            }
            if subject.query.predicate != ObjectPredicate::ObjectKind(ObjectKind::Permanent) {
                let filter = lower_permanent_object_predicate(&subject.query.predicate);
                cursor.require(!filter.is_null(), "a lowered combatant predicate")?;
                object["where"] = filter;
            }
            let role = if event.verb == ObjectEventVerb::Block {
                "blocker"
            } else {
                "attacker"
            };
            Ok(json!({
                "kind": "eventPattern",
                "lemma": "block",
                "subject": {"role": role, "object": object},
                "aggregation": "oneOrMore",
            }))
        }
        ObjectEventVerb::Block | ObjectEventVerb::BecomeBlocked => {
            // One trigger per blocking creature, or once per combat for each
            // attacking creature that becomes blocked (CR 509.1h).
            cursor.require(
                subject.query.controller == Some(ObjectControllerScope::AbilityController)
                    && subject
                        .query
                        .predicate
                        .supports_domain(ObjectKind::Permanent),
                "creatures the ability controller controls",
            )?;
            let kind = if event.verb == ObjectEventVerb::Block {
                "controlledCreatureDeclaredBlocker"
            } else {
                "controlledCreatureBecameBlocked"
            };
            Ok(json!({
                "kind": kind,
                "player": controller(),
                "where": lower_permanent_object_predicate(&subject.query.predicate),
            }))
        }
        ObjectEventVerb::LeaveBattlefield
        | ObjectEventVerb::PutIntoGraveyardFromBattlefield
        | ObjectEventVerb::PutIntoGraveyard(..) => {
            unreachable!("zone changes are lowered before the per-verb support checks")
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
        ObjectEventVerb::DealDamage => {
            unreachable!("dealt damage is lowered by the shared damage-event branch")
        }
    }
}

/// One permanent per event (`a creature you control deals ...`): the damage
/// source is a battlefield permanent matching the query when it deals the
/// damage. A one-or-more batch of damage sources has no matcher yet.
fn lower_damage_subject_query(
    cursor: &mut OracleTokenCursor<'_>,
    subject: &QuantifiedEventSubjectNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.dealt-damage.source-support");
    cursor.require(
        subject.multiplicity == EventMultiplicity::PerObject,
        "a singular damage source (no one-or-more damage-source batch is proved)",
    )?;
    let mut source = json!({"kind": "permanents"});
    if let Some(filter) = lower_damage_permanent_query(cursor, &subject.query, &mut source)? {
        source["where"] = filter;
    }
    if subject.exclude_source {
        source["excludeSource"] = json!(true);
    }
    Ok(source)
}

/// A permanent query of a damage event: its effective-characteristics filter
/// and its controller relation as a player reference.
fn lower_damage_permanent_query(
    cursor: &mut OracleTokenCursor<'_>,
    query: &ObjectQueryNode,
    target: &mut Value,
) -> MachineResult<Option<Value>> {
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent)
            && !query.predicate.contains_numeric_comparison()
            && domain_filter_is_representable(&query.predicate),
        "a representable permanent predicate without numeric characteristics",
    )?;
    match query.controller {
        None => {}
        Some(ObjectControllerScope::AbilityController) => target["controller"] = controller(),
        Some(ObjectControllerScope::OpponentOfAbilityController) => {
            target["controller"] = json!({"kind": "opponentsOf", "player": controller()});
        }
        Some(ObjectControllerScope::NotAbilityController | ObjectControllerScope::EventPlayer) => {
            return Err(cursor.fail("a positive controller relation for a damage event object"));
        }
    }
    if query.predicate == ObjectPredicate::ObjectKind(ObjectKind::Permanent) {
        return Ok(None);
    }
    let filter = lower_permanent_object_predicate(&query.predicate);
    cursor.require(!filter.is_null(), "a lowered permanent predicate")?;
    Ok(Some(filter))
}

/// `damageDealtBySource`: one trigger per damage event and recipient (CR
/// 120.1, 603.2c). The damage kind and the recipient are independent fields.
fn lower_damage_dealt_event(
    cursor: &mut OracleTokenCursor<'_>,
    source: Value,
    complement: &DamageDealtComplement,
) -> MachineResult<Value> {
    let mut lowered = json!({"kind": "damageDealtBySource", "source": source});
    match complement.kind {
        DamageKind::Any => {}
        DamageKind::Combat => lowered["combatOnly"] = json!(true),
        DamageKind::Noncombat => lowered["noncombatOnly"] = json!(true),
    }
    cursor.enter("trigger.dealt-damage.recipient-support");
    // Without a recipient one event can damage several objects and players
    // at once: the generic damage pattern triggers once per source and
    // simultaneous event, with the total amount (docs/oracle-event-model.md).
    let Some(recipient) = complement.recipient.as_ref() else {
        let source = lowered["source"].take();
        let mut pattern = json!({"kind": "eventPattern", "lemma": "dealDamage", "source": source});
        for flag in ["combatOnly", "noncombatOnly"] {
            if let Some(value) = lowered.get(flag) {
                pattern[flag] = value.clone();
            }
        }
        return Ok(pattern);
    };
    lowered["recipient"] = match recipient {
        DamageRecipient::Player(actor) => {
            json!({"kind": "player", "player": event_actor_player(*actor)})
        }
        DamageRecipient::Permanent(query) => {
            let mut recipient = json!({"kind": "permanent"});
            if let Some(filter) = lower_damage_permanent_query(cursor, query, &mut recipient)? {
                recipient["where"] = filter;
            }
            recipient
        }
    };
    Ok(lowered)
}

/// The attacked object of an attack pattern (CR 506.2, 508.1b): each
/// alternative through the shared player relation and permanent query.
fn lower_attacked_recipients(
    cursor: &mut OracleTokenCursor<'_>,
    recipients: &[AttackedRecipientNode],
) -> MachineResult<Value> {
    cursor.enter("trigger.attack.attacked-recipient-support");
    let alternatives = recipients
        .iter()
        .map(|recipient| match recipient {
            AttackedRecipientNode::Player(actor) => {
                Ok(json!({"kind": "player", "player": event_actor_player(*actor)}))
            }
            AttackedRecipientNode::Permanent(query) => {
                let mut lowered = json!({"kind": "permanents"});
                if let Some(filter) = lower_damage_permanent_query(cursor, query, &mut lowered)? {
                    lowered["where"] = filter;
                }
                Ok(lowered)
            }
        })
        .collect::<MachineResult<Vec<_>>>()?;
    Ok(json!({"anyOf": alternatives}))
}

/// One attack declaration (CR 508.1): the attack-batch matcher counts the
/// attackers of the ability controller that match the shared query. With
/// the source coordinated (`SOURCE and at least N other creatures attack`),
/// the source must attack and is counted among the attackers.
fn lower_attack_declaration(
    cursor: &mut OracleTokenCursor<'_>,
    node: &AttackDeclarationNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.attack-declaration.attacker-support");
    cursor.require(
        node.attacker == EventActorScope::Controller,
        "the ability controller as attacking player (the batch matcher counts only that player's attackers)",
    )?;
    let EventMultiplicity::PerEvent { minimum } = node.attackers.multiplicity else {
        return Err(cursor.fail("`N or more` attackers of one declaration"));
    };
    let predicate = &node.attackers.query.predicate;
    cursor.enter("trigger.attack-declaration.attackers-support");
    cursor.require(
        node.attackers.query.controller.is_none()
            && predicate.supports_domain(ObjectKind::Permanent)
            && !predicate.contains_numeric_comparison()
            && domain_filter_is_representable(predicate),
        "a representable attacker predicate without numeric characteristics",
    )?;
    let filter = lower_permanent_object_predicate(predicate);
    cursor.require(!filter.is_null(), "a lowered attacker predicate")?;
    let mut lowered = json!({
        "kind": "controlledCreaturesAttacked",
        "player": controller(),
        "where": filter,
    });
    match node.source {
        None => {
            if minimum > 1 {
                lowered["minimum"] = json!(minimum);
            }
        }
        Some(source) => {
            cursor.enter("trigger.attack-declaration.source-support");
            cursor.require(
                source == LocalObjectReference::SourceSelf,
                "the current source among the attackers",
            )?;
            // The matcher filters every counted attacker, the source
            // included: only the unfiltered creature set counts the source
            // and its companions alike.
            if *predicate != ObjectPredicate::CardType(ObjectCardType::Creature) {
                return Err(engine_gap(
                    cursor,
                    "an attack-declaration matcher that filters the source's companions apart from the source",
                ));
            }
            lowered["sourceMustAttack"] = json!(true);
            lowered["minimum"] = json!(minimum + 1);
        }
    }
    Ok(lowered)
}

/// Received-damage observers filter the damage kind with `combatOnly` or
/// `noncombatOnly` (CR 120.2); any damage sets neither.
fn apply_damage_event_kind(
    cursor: &mut OracleTokenCursor<'_>,
    lowered: &mut Value,
    damage: DamageKind,
) -> MachineResult<()> {
    cursor.enter("trigger.dealt-damage.kind-support");
    match damage {
        DamageKind::Any => {}
        DamageKind::Noncombat => lowered["noncombatOnly"] = json!(true),
        DamageKind::Combat => lowered["combatOnly"] = json!(true),
    }
    Ok(())
}

/// `that targets [only] T`: each alternative through the shared player and
/// permanent-query lowering (CR 115.1).
fn lower_spell_targets(
    cursor: &mut OracleTokenCursor<'_>,
    targets: &SpellTargetsNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.spell-cast.target-support");
    let alternatives = targets
        .alternatives
        .iter()
        .map(|target| match target {
            SpellTargetNode::Source => Ok(json!({"kind": "self"})),
            SpellTargetNode::Player(actor) => {
                Ok(json!({"kind": "player", "player": event_actor_player(*actor)}))
            }
            SpellTargetNode::Permanent(query) => {
                let mut lowered = json!({"kind": "permanents"});
                if let Some(filter) = lower_damage_permanent_query(cursor, query, &mut lowered)? {
                    lowered["where"] = filter;
                }
                Ok(lowered)
            }
        })
        .collect::<MachineResult<Vec<_>>>()?;
    Ok(json!({"only": targets.only, "anyOf": alternatives}))
}

/// Every numeric comparison of the predicate has a literal right side.
fn literal_numeric_comparisons(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => {
            matches!(**right, NumericExpression::Atom(NumericAtom::Integer(_)))
        }
        ObjectPredicate::Not(child) => literal_numeric_comparisons(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            children.iter().all(literal_numeric_comparisons)
        }
        _ => true,
    }
}

/// The characteristics the spell-cast history records for each spell: its
/// type line (types, subtypes, supertypes) and name.
fn spell_history_predicate(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Spell)
        | ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::Supertype(_) => true,
        ObjectPredicate::Not(child) => spell_history_predicate(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            !children.is_empty() && children.iter().all(spell_history_predicate)
        }
        _ => false,
    }
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

/// The characteristics the engine captures when an object leaves the
/// battlefield (types, subtypes, colors, supertypes, token status, keywords;
/// docs/oracle-event-model.md). Numeric characteristics are not captured.
fn snapshot_permanent_event_predicate(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Permanent)
        | ObjectPredicate::IsToken
        | ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::Color(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::HasKeyword(_) => true,
        ObjectPredicate::Not(child) => snapshot_permanent_event_predicate(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            !children.is_empty() && children.iter().all(snapshot_permanent_event_predicate)
        }
        _ => false,
    }
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
    if event.object.predicate.contains_numeric_comparison() {
        cursor.enter("trigger.sacrifice.numeric-snapshot-support");
        return Err(cursor.fail(
            "a predicate without numeric sacrificed-object characteristics (not captured at departure)",
        ));
    }
    // The engine reads the predicate on the effective characteristics
    // captured as the object left the battlefield (CR 603.10a).
    cursor.enter("trigger.sacrifice.characteristic-snapshot-support");
    cursor.require(
        snapshot_permanent_event_predicate(&event.object.predicate)
            && domain_filter_is_representable(&event.object.predicate),
        "a predicate over the characteristics captured at departure",
    )?;
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
            // The zone-change matcher filters the moved object's
            // characteristics; it has no owner relation (`you don't own`).
            if predicate_reads_ownership(predicate) {
                return Err(engine_gap(
                    cursor,
                    "a zone-change object filter without an ownership relation",
                ));
            }
            let mut lowered =
                json!({"kind": kind, "where": lower_permanent_object_predicate(predicate)});
            match scope {
                Some(ObjectControllerScope::AbilityController) => lowered["player"] = controller(),
                Some(ObjectControllerScope::OpponentOfAbilityController) => {
                    lowered["player"] = json!({"kind": "opponentsOf", "player": controller()});
                }
                Some(
                    ObjectControllerScope::NotAbilityController
                    | ObjectControllerScope::EventPlayer,
                ) => {
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
        LocalTriggerEventNode::LastCounterRemoved { source, counter } => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf,
                "the current source as the object losing its last counter",
            )?;
            Ok(json!({
                "kind": "counterRemoved",
                "object": self_ref(),
                "counter": counter.as_str(),
                "last": true,
            }))
        }
        LocalTriggerEventNode::SourceCycled { source } => {
            cursor.require(
                *source == LocalObjectReference::SourceSelf,
                "the current cycled source",
            )?;
            Ok(json!({"kind": "sourceCycled", "object": self_ref()}))
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
            actions,
            one_or_more,
            card,
            permanent,
        } => lower_player_event(
            cursor,
            *actor,
            actions,
            *one_or_more,
            card.as_ref(),
            permanent.as_ref(),
        ),
        LocalTriggerEventNode::AttackDeclaration(node) => lower_attack_declaration(cursor, node),
        // Aura and Equipment attachments share the attached-object selector
        // of the generic targeting matcher (CR 115.1).
        LocalTriggerEventNode::AttachedObjectTargeted { role: _, stack } => Ok(json!({
            "kind": "becameTarget",
            "object": {"kind": "attachedPermanent", "attachment": self_ref()},
            "targetingStackObject": lower_targeting_stack_selector(stack),
        })),
        // One received-damage record per damage event and recipient (CR
        // 120.3), from any source, observed by the attachment.
        LocalTriggerEventNode::AttachedObjectDealtDamage { role: _, damage } => {
            let mut lowered = json!({
                "kind": "eventPattern",
                "lemma": "dealDamage",
                "recipient": {"kind": "attachedPermanent", "attachment": self_ref()},
            });
            apply_damage_event_kind(cursor, &mut lowered, *damage)?;
            Ok(lowered)
        }
        LocalTriggerEventNode::AttachedObjectEvent { role: _, action } => {
            // The runtime resolves the subject from the source's attachment;
            // Aura and Equipment attachments share the same matcher.
            Ok(match action {
                AttachedObjectEventAction::DealCombatDamageToPlayer => {
                    json!({"kind": "attachedPermanentCombatDamageToPlayer"})
                }
                AttachedObjectEventAction::Attack => {
                    json!({"kind": "attachedPermanentDeclaredAttacker"})
                }
                AttachedObjectEventAction::Die => json!({"kind": "attachedPermanentDied"}),
                AttachedObjectEventAction::BecomeTapped => {
                    json!({"kind": "attachedPermanentTapped", "attachment": self_ref()})
                }
                AttachedObjectEventAction::BecomeBlocked => {
                    json!({"kind": "attachedCreatureBecameBlocked", "attachment": self_ref()})
                }
                // The mana-ability observer of the enchanted permanent.
                AttachedObjectEventAction::TappedForMana => {
                    json!({"kind": "attachedPermanentManaAbilityActivated", "object": self_ref()})
                }
            })
        }
        LocalTriggerEventNode::DamageDealt {
            subject,
            complement,
        } => {
            let source = match subject {
                DamageDealer::Source(source) => {
                    cursor.enter("trigger.dealt-damage.source-support");
                    cursor.require(
                        *source == LocalObjectReference::SourceSelf,
                        "the current source as damage source",
                    )?;
                    json!({"kind": "self"})
                }
                // Aura and Equipment attachments share the attached matcher.
                DamageDealer::Attached(_) => {
                    json!({"kind": "attachedPermanent", "attachment": self_ref()})
                }
            };
            lower_damage_dealt_event(cursor, source, complement)
        }
        LocalTriggerEventNode::CardDrawn { .. }
        | LocalTriggerEventNode::OrdinalCardDrawn { .. } => {
            cursor.enter("trigger.program.local-event-support");
            Err(cursor.fail(
                "a local draw event with an inspected and accepted runtime player/ordinal contract",
            ))
        }
    }
}

/// Keyword actions whose canonical producer emits the completed named-action
/// signal (`emitAbilitySignal`) that the runtime observes. Another catalog
/// action is recognized but has no inspected producer and fails explicitly.
const SIGNALLED_NAMED_ACTIONS: [&str; 3] = ["scry", "surveil", "amass"];

fn event_actor_player(actor: EventActorScope) -> Value {
    match actor {
        EventActorScope::Controller => controller(),
        EventActorScope::Opponent => json!({"kind": "opponentsOf", "player": controller()}),
        EventActorScope::AnyPlayer => json!({"kind": "eachPlayer"}),
    }
}

/// Maps player-event verbs and one actor to the runtime events that already
/// implement them. A union lowers to `oneOf` in written order. Combinations
/// without an inspected runtime matcher fail explicitly instead of widening.
fn lower_player_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    actions: &[PlayerEventAction],
    one_or_more: bool,
    card: Option<&PlayerEventCardObject>,
    permanent: Option<&PlayerEventCardObject>,
) -> MachineResult<Value> {
    cursor.enter("trigger.program.player-event-support");
    let player = event_actor_player(actor);
    match actions {
        [] => Err(cursor.fail("at least one player-event verb")),
        [PlayerEventAction::TapForMana] => {
            lower_tap_for_mana_event(cursor, actor, one_or_more, permanent)
        }
        [PlayerEventAction::ActivateAbility(kind)] => {
            cursor.enter("trigger.player-event.activation-support");
            cursor.require(
                !one_or_more,
                "a per-player activation event (no one-or-more player batch)",
            )?;
            let mut lowered = match actor {
                EventActorScope::Controller => {
                    json!({"kind": "nonManaAbilityActivated", "player": player})
                }
                EventActorScope::Opponent => {
                    json!({"kind": "nonManaAbilityActivated", "opponentOfSourceController": true})
                }
                EventActorScope::AnyPlayer => {
                    return Err(engine_gap(
                        cursor,
                        "an activation matcher for any player (the runtime matches the controller or an opponent)",
                    ));
                }
            };
            if *kind == ActivatedAbilityKind::Loyalty {
                lowered["loyaltyOnly"] = json!(true);
            }
            Ok(lowered)
        }
        [PlayerEventAction::CommitCrime] => Err(engine_gap(
            cursor,
            "an engine matcher for a committed crime (CR 700.13)",
        )),
        [action]
            if !action.takes_card_object()
                && !matches!(action, PlayerEventAction::NamedAction(_)) =>
        {
            lower_fixed_player_event(cursor, actor, player, *action, one_or_more)
        }
        _ => lower_player_action_union(cursor, &player, actions, one_or_more, card),
    }
}

/// `ACTOR tap(s) PERMANENT for mana` (CR 106.11a, 605.1a): the mana-ability
/// observer of the tapped permanent, with the shared permanent predicate.
/// The observer records the permanent's controller as the acting player.
fn lower_tap_for_mana_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    one_or_more: bool,
    permanent: Option<&PlayerEventCardObject>,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.tap-for-mana-support");
    let permanent = permanent.ok_or_else(|| cursor.fail("the permanent tapped for mana"))?;
    cursor.require(
        !one_or_more && !permanent.exclude_source,
        "one tapped permanent without a source exclusion",
    )?;
    let predicate = &permanent.query.predicate;
    cursor.require(
        !predicate.contains_numeric_comparison() && domain_filter_is_representable(predicate),
        "a representable tapped-permanent predicate without numeric characteristics",
    )?;
    let filter = lower_permanent_object_predicate(predicate);
    cursor.require(!filter.is_null(), "a lowered tapped-permanent predicate")?;
    match actor {
        EventActorScope::Controller => Ok(json!({
            "kind": "controlledPermanentManaAbilityActivated",
            "player": controller(),
            "where": filter,
        })),
        EventActorScope::AnyPlayer => Ok(json!({
            "kind": "controlledPermanentManaAbilityActivated",
            "anyPlayer": true,
            "where": filter,
        })),
        EventActorScope::Opponent => Err(engine_gap(
            cursor,
            "a mana-ability observer for an opponent's permanent",
        )),
    }
}

/// Discard, cycling and named keyword actions: one event per action
/// occurrence and acting player (CR 701.9, 702.29, 701.22d).
fn lower_player_action_union(
    cursor: &mut OracleTokenCursor<'_>,
    player: &Value,
    actions: &[PlayerEventAction],
    one_or_more: bool,
    card: Option<&PlayerEventCardObject>,
) -> MachineResult<Value> {
    cursor.require(
        !one_or_more,
        "a per-player action event (no one-or-more player batch for this action)",
    )?;
    // CR 702.29a: cycling's cost discards the card, so every cycled card is
    // also discarded. `cycle or discard` is therefore the discard event alone;
    // keeping both branches would trigger twice for one cycled card.
    let normalized: Vec<PlayerEventAction> = if actions.contains(&PlayerEventAction::DiscardCard)
        && actions.contains(&PlayerEventAction::CycleCard)
    {
        actions
            .iter()
            .copied()
            .filter(|action| *action != PlayerEventAction::CycleCard)
            .collect()
    } else {
        actions.to_vec()
    };
    let card_filter = match card {
        Some(card) => {
            cursor.enter("trigger.player-event.card-filter-support");
            cursor.require(
                !card.query.predicate.contains_numeric_comparison()
                    && domain_filter_is_representable(&card.query.predicate),
                "a representable card predicate without numeric characteristics",
            )?;
            // `a card` is the unfiltered card domain: no filter at all.
            let filter = if card.query.predicate == ObjectPredicate::ObjectKind(ObjectKind::Card) {
                None
            } else {
                let filter = lower_card_object_predicate(&card.query.predicate);
                cursor.require(!filter.is_null(), "a lowered card predicate")?;
                Some(filter)
            };
            Some((filter, card.exclude_source))
        }
        None => None,
    };
    cursor.enter("trigger.player-event.action-support");
    let mut events = Vec::with_capacity(normalized.len());
    for action in &normalized {
        let mut lowered = match action {
            PlayerEventAction::DiscardCard => json!({"kind": "playerDiscardedCard"}),
            PlayerEventAction::CycleCard => json!({"kind": "cardCycled"}),
            PlayerEventAction::NamedAction(id) => {
                cursor.require(
                    SIGNALLED_NAMED_ACTIONS.contains(id),
                    "a named action whose canonical producer emits its completion signal",
                )?;
                json!({"kind": "abilityPerformed", "abilityId": id})
            }
            _ => return Err(cursor.fail("an action verb without a fixed complement")),
        };
        lowered["player"] = player.clone();
        if action.takes_card_object() {
            let (filter, exclude_source) = card_filter
                .clone()
                .ok_or_else(|| cursor.fail("the shared card object"))?;
            if let Some(filter) = filter {
                lowered["where"] = filter;
            }
            if exclude_source {
                lowered["excludeSource"] = json!(true);
            }
        }
        events.push(lowered);
    }
    Ok(if events.len() == 1 {
        events.into_iter().next().expect("one player action")
    } else {
        json!({"kind": "oneOf", "events": events})
    })
}

fn lower_fixed_player_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    player: Value,
    action: PlayerEventAction,
    one_or_more: bool,
) -> MachineResult<Value> {
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
        (
            PlayerEventAction::BeDealtDamage(_)
            | PlayerEventAction::DiscardCard
            | PlayerEventAction::CycleCard
            | PlayerEventAction::NamedAction(_)
            | PlayerEventAction::ActivateAbility(_)
            | PlayerEventAction::TapForMana
            | PlayerEventAction::CommitCrime,
            _,
        ) => unreachable!("lowered by the received-damage, action-union or action-frame branches"),
    }
}

#[cfg(test)]
#[path = "dealt_damage_event_tests.rs"]
mod dealt_damage_event_tests;

#[cfg(test)]
#[path = "trigger_condition_language_tests.rs"]
mod trigger_condition_language_tests;
