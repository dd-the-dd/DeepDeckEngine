use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

/// Outcomes preserve ownership from the typed event envelope. Cursor
/// advancement and diagnostic labels never decide semantic commitment.
pub(in crate::oracle::canonical) enum GenericTriggerProgramAttempt {
    NotApplicable,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(CanonicalRuleDraft),
}

/// The common parent never reparses source text. Once the event is a complete
/// typed envelope, this parent owns its guard/program and any failure there.
pub(in crate::oracle::canonical) fn try_parse_generic_trigger_program(
    input: &AbilityInput<'_>,
) -> GenericTriggerProgramAttempt {
    let mut cursor = OracleTokenCursor::new("generic-trigger-program", input.tokens);
    // The ability's resolved source names are readable everywhere in the
    // ability, its event and intervening condition included (`if it has
    // greater power than NAME`), not only inside the effect sequence.
    cursor.replace_source_names(Some(input.name_links.into()));
    let envelope = match try_recognize_generic_trigger_envelope(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        GenericTriggerEnvelopeAttempt::NotClaimed => {
            return GenericTriggerProgramAttempt::NotApplicable;
        }
        GenericTriggerEnvelopeAttempt::RejectedCandidate(failure) => {
            return GenericTriggerProgramAttempt::RejectedCandidate(failure);
        }
        GenericTriggerEnvelopeAttempt::FailedCommitted(failure) => {
            return GenericTriggerProgramAttempt::FailedCommitted(failure);
        }
        GenericTriggerEnvelopeAttempt::Recognized(envelope) => envelope,
    };
    match compile_recognized_trigger_program(input, &mut cursor, envelope) {
        Ok(draft) => GenericTriggerProgramAttempt::Parsed(draft),
        Err(failure) => GenericTriggerProgramAttempt::FailedCommitted(failure),
    }
}

fn compile_recognized_trigger_program(
    input: &AbilityInput<'_>,
    cursor: &mut OracleTokenCursor<'_>,
    envelope: GenericTriggerEnvelopeNode,
) -> MachineResult<CanonicalRuleDraft> {
    // The zone the ability functions from (CR 113.6) follows from the typed
    // event: the spell's own cast functions on the stack for any spell, a
    // battlefield observer needs a permanent face. A card zone the guard or
    // the program declares replaces the battlefield; the face is checked
    // once that zone is known.
    cursor.enter("trigger.program.permanent-face");
    let face_cursor = cursor.clone();
    let battlefield_face = !envelope.event.requires_permanent_face()
        || envelope.event.functions_on_the_stack()
        || type_line_has_permanent_card_type(input.face_type_line);
    // CR 701.28: `transforms into FACE` is observed by the abilities of the
    // face the permanent turned to, so FACE must print this ability.
    cursor.enter("trigger.program.transformed-face");
    cursor.require(
        envelope
            .event
            .transformed_into()
            .is_none_or(|face| face.eq_ignore_ascii_case(input.face_name)),
        "`transforms into` naming the face that prints this ability",
    )?;
    let event = lower_generic_trigger_event(cursor, &envelope.event)?;
    // `while CONDITION` / an unmatched turn owner: checked as the event
    // occurs (CR 603.2), never on resolution like the intervening `if`.
    let event_condition = envelope
        .event_condition
        .as_ref()
        .map(|condition| lower_event_time_condition(cursor, condition))
        .transpose()?;
    let mut construction = vec![(
        "Resolve the event envelope; leave the guard and body pending".into(),
        json!({"syntax":"triggeredProgram", "event":event,
            "eventTokens":[0,envelope.body_start], "pendingFromToken":envelope.body_start}),
    )];
    let node = recognize_trigger_program_tail(
        cursor,
        SyntaxNode {
            tokens: 0..envelope.body_start,
            value: envelope.event,
        },
        input.name_links,
        input.name_resolution_failure,
        source_attachment_role(input.face_type_line),
    )?;
    // `if you control another Human` / `two or more other creatures` (CR
    // 603.4): `other` excludes the object the ability is about. When that
    // object is the ability's source (its own entry, its own attack, or an
    // event that binds no object), the count excludes the source's current
    // battlefield incarnation, which the runtime authenticates by instance
    // and zone identity both when the trigger is put on the stack and when
    // it resolves; once the source left the battlefield nothing is excluded
    // (CR 400.7). An event that binds another object (`Whenever a Mountain
    // you control enters, if you control at least five other Mountains`)
    // makes `other` exclude that object, which this count cannot express.
    if node
        .guard
        .as_ref()
        .is_some_and(|guard| guard_excludes_source(&guard.value))
        && guard_other_names_event_object(&node.event.value)
    {
        cursor.enter("trigger.program.guard-other-event-object");
        return Err(cursor.fail(
            "an exclusion of the triggering event's object for `other`; the count excludes only the ability's source",
        ));
    }
    let condition = node
        .guard
        .as_ref()
        .map(|guard| {
            lower_sentence_condition(
                cursor,
                &guard.value,
                &ConditionObjectBindings {
                    permanent_targets: &[],
                    card_targets: &[],
                    card_focus: None,
                    previous_action_result: None,
                },
            )
        })
        .transpose()?;
    construction.push((
        "Compose the typed condition and effect sequence within their token ranges".into(),
        json!({"syntax":"triggeredProgram", "event":event,
            "eventTokens":[node.event.tokens.start,node.event.tokens.end],
            "condition":condition,
            "conditionTokens":node.guard.as_ref().map(|n| vec![n.tokens.start,n.tokens.end]),
            "programTokens":[node.program.tokens.start,node.program.tokens.end],
            "actionCount":node.program.value.action_count()}),
    ));
    // The lowering reads the frame's event card through the same environment.
    let enclosing =
        cursor.replace_binding_environment(Some(std::sync::Arc::new(BindingEnvironment {
            frame: FrameBindings {
                event_card: event_card_zone(&node.event.value),
                ..FrameBindings::default()
            },
            ..BindingEnvironment::default()
        })));
    let lowered = match &node.program.value {
        // X is lowered provisionally; the card-composition validator accepts
        // it only with a typed producer of this rule (CR 107.3m).
        TriggerBodyNode::Program(program) => lower_effect_program_with_context(
            cursor,
            program,
            EffectSequenceLoweringContext::ValidateXAfterCardComposition,
        ),
        TriggerBodyNode::Modal(modal) => lower_effect_sequence_modal(cursor, modal),
    };
    cursor.replace_binding_environment(enclosing);
    let mut lowered = lowered?;
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable action",
    )?;
    if event["kind"] == "attachedPermanentManaAbilityActivated" {
        normalize_attached_mana_performers(&mut lowered.effects);
    }
    // CR 605.1b: a trigger on an enchanted permanent's mana ability whose
    // program adds mana is a triggered mana ability. It resolves at once as
    // part of that mana ability (CR 605.4a) when every action adds mana to
    // the enchanted permanent's controller and nothing is declared; any
    // other mana-adding program has no typed runtime context. A program
    // that adds no mana is an ordinary triggered ability.
    if event["kind"] == "attachedPermanentManaAbilityActivated"
        && lowered.effects.iter().any(adds_mana)
        && !(condition.is_none()
            && event_condition.is_none()
            && !node.once_each_turn
            && lowered.decisions.is_empty()
            && attached_mana_trigger_is_folded(&lowered.effects))
    {
        cursor.enter("trigger.program.attached-mana-ability");
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
            "a checked mana discovery for a triggered mana ability of the enchanted permanent",
        ));
    }
    let trigger_zone = declared_trigger_zone(
        cursor,
        node.guard.as_ref().map(|guard| &guard.value),
        &lowered.effects,
        &node.event.value,
    )?;
    if trigger_zone == TriggerZone::Battlefield && !battlefield_face {
        return Err(
            face_cursor.fail("a permanent source face for this battlefield triggered ability")
        );
    }
    cursor.enter("trigger.program.zone-of-function.event");
    if !trigger_zone.observes(&event) {
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
            "a runtime observer of this event for a source in its zone of function (CR 113.6)",
        ));
    }
    let mut rule = json!({"kind":"triggeredAbility", "source":self_ref(), "event":event, "effects":lowered.effects});
    if let Some(zone) = trigger_zone.card_zone_name() {
        rule["triggerZone"] = json!(zone);
    }
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    if let Some(event_condition) = event_condition {
        rule["eventCondition"] = event_condition;
    }
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({"kind":"castingDeclaration", "decisions":lowered.decisions});
    }
    if node.once_each_turn {
        // Unique per ability on this face: the runtime counts per source and id.
        rule["triggerLimit"] = json!({
            "kind": "onceEachTurn",
            "id": format!("onceEachTurn:{}", input.source.line_start),
        });
    }
    construction.push((
        "Lower the composed syntax into canonical primitive IR".into(),
        rule.clone(),
    ));
    let mut result = draft(
        rule,
        &["Compose the shared event, Boolean guard and action program"],
    );
    result.construction = construction;
    Ok(result)
}

/// The zone a triggered ability functions from (CR 113.6), one typed
/// attribute of the trigger frame. The battlefield is the default; the
/// stack is the zone of a spell's own cast observer (CR 601.2i); a card zone
/// is declared by the ability's own evidence (an intervening `if this card
/// is in your graveyard`, or a program that moves `this card` out of that
/// zone), never by the card's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TriggerZone {
    Battlefield,
    Stack,
    Graveyard,
    Hand,
    Exile,
}

/// Lowered event kinds a card in a graveyard observes through the shared
/// publication of their family (`engine::zone_of_function`): an observed
/// entry, a step, a spell cast by a player, a draw, a life gain, an attack
/// declaration batch and the player outcome events. Other zone changes are
/// admitted by their move shape in [`graveyard_observes`].
const GRAVEYARD_OBSERVED_EVENT_KINDS: [&str; 12] = [
    "permanentEntered",
    "stepBegan",
    "spellCast",
    "cardDrawn",
    "lifeGained",
    "controlledCreaturesAttacked",
    "crimeCommitted",
    "proliferated",
    "dieRolled",
    "coinFlipped",
    "clashed",
    "permanentExerted",
];

/// Whether a card in a graveyard observes one lowered event (every
/// alternative of a list): a kind of [`GRAVEYARD_OBSERVED_EVENT_KINDS`]
/// other than a spell's own cast, a death of another permanent, an entry,
/// or cards arriving in a graveyard (a generic move of cards to a
/// graveyard). A departure read only by permanents captured on the
/// battlefield is not observed from a graveyard.
fn graveyard_observes(event: &Value) -> bool {
    match event["kind"].as_str() {
        Some("oneOf") => event["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty() && events.iter().all(graveyard_observes)),
        Some("spellCast") => event["sourceIsSelf"].as_bool() != Some(true),
        Some("permanentDied") => event.get("object").is_none(),
        Some("eventPattern") => {
            event["lemma"] == "move"
                && ((event["to"] == "graveyard" && event["subject"]["object"]["kind"] == "cards")
                    || (event["to"] == "battlefield"
                        && event["subject"]["object"]["kind"] == "permanents"))
        }
        Some(kind) => GRAVEYARD_OBSERVED_EVENT_KINDS.contains(&kind),
        None => false,
    }
}

impl TriggerZone {
    /// The engine name of a card zone outside the battlefield and the
    /// stack, written as the rule's `triggerZone`; `None` for the default
    /// battlefield and for a spell's own cast, typed by its event.
    fn card_zone_name(self) -> Option<&'static str> {
        match self {
            Self::Battlefield | Self::Stack => None,
            Self::Graveyard => Some("graveyard"),
            Self::Hand => Some("hand"),
            Self::Exile => Some("exile"),
        }
    }

    /// Whether the runtime observes this lowered event for a source in this
    /// zone. The battlefield and the stack keep their event lowering; a
    /// graveyard reads [`graveyard_observes`]; the hand and exile have no
    /// observer of a printed triggered ability (an explicit engine gap).
    fn observes(self, event: &Value) -> bool {
        match self {
            Self::Battlefield | Self::Stack => true,
            Self::Graveyard => graveyard_observes(event),
            Self::Hand | Self::Exile => false,
        }
    }
}

/// The card zone a guard's zone-of-function atom names, when it is a card
/// zone outside the battlefield.
fn card_zone_of_function(zone: ConditionZone) -> Option<TriggerZone> {
    match zone {
        ConditionZone::Graveyard => Some(TriggerZone::Graveyard),
        ConditionZone::Exile => Some(TriggerZone::Exile),
        ConditionZone::Hand => Some(TriggerZone::Hand),
        _ => None,
    }
}

/// The card zone a source atom of the guard names (`if this card is in your
/// graveyard`), or `None`.
fn guard_zone_atom(atom: &ConditionAtom) -> Option<TriggerZone> {
    match atom {
        ConditionAtom::ObjectZone(ObjectZoneConditionNode {
            object,
            zones,
            negated: false,
        }) if *object == ConditionObjectReference::SOURCE => match zones.as_slice() {
            [zone] => card_zone_of_function(*zone),
            _ => None,
        },
        _ => None,
    }
}

/// Whether the event's trigger condition names the ability's own source,
/// which then says where it is (CR 113.6k: `When this creature dies, return
/// this card from your graveyard` functions on the battlefield).
fn event_names_source(event: &GenericTriggerEventNode) -> bool {
    match event {
        GenericTriggerEventNode::SourceTransition(_) | GenericTriggerEventNode::SourceEvent(_) => {
            true
        }
        GenericTriggerEventNode::Local(local) => matches!(
            local,
            LocalTriggerEventNode::SourceEntered { .. }
                | LocalTriggerEventNode::SourceAttacked { .. }
                | LocalTriggerEventNode::SourceCycled { .. }
                | LocalTriggerEventNode::LastCounterRemoved { .. }
                | LocalTriggerEventNode::SourceDealtDamage { .. }
        ),
        GenericTriggerEventNode::Conditioned { event, .. } => event_names_source(event),
        GenericTriggerEventNode::Compound(members) => members.iter().any(event_names_source),
        _ => false,
    }
}

/// Whether the event reads the object the ability's source is attached to
/// (`enchanted creature`, `equipped creature`, CR 301.5, 303.4).
fn event_reads_source_attachment(event: &GenericTriggerEventNode) -> bool {
    match event {
        GenericTriggerEventNode::Local(local) => matches!(
            local,
            LocalTriggerEventNode::AttachedObjectEvent { .. }
                | LocalTriggerEventNode::AttachedObjectBlock { .. }
                | LocalTriggerEventNode::AttachedObjectDeparture { .. }
        ),
        GenericTriggerEventNode::Conditioned { event, .. } => event_reads_source_attachment(event),
        GenericTriggerEventNode::Compound(members) => {
            members.iter().any(event_reads_source_attachment)
        }
        _ => false,
    }
}

/// The card zone a lowered program moves the ability's own source out of
/// (`return this card from your graveyard to your hand`): the `this card`
/// operand of the move is the card in that zone. Moving another card out of
/// a graveyard (`return target creature card from your graveyard`) declares
/// nothing.
fn program_source_origin(value: &Value) -> Option<TriggerZone> {
    match value {
        Value::Object(map) => {
            let moves_source = matches!(
                map.get("kind").and_then(Value::as_str),
                Some("moveCards" | "returnToOwnersHand")
            ) && [map.get("cards"), map.get("object")]
                .into_iter()
                .flatten()
                .any(|moved| *moved == self_ref());
            let origin = if moves_source {
                match map
                    .get("from")
                    .and_then(|from| from.get("kind"))
                    .and_then(Value::as_str)
                {
                    Some("graveyard") => Some(TriggerZone::Graveyard),
                    Some("hand") => Some(TriggerZone::Hand),
                    _ => None,
                }
            } else {
                None
            };
            origin.or_else(|| map.values().find_map(program_source_origin))
        }
        Value::Array(items) => items.iter().find_map(program_source_origin),
        _ => None,
    }
}

/// The zone of function a triggered ability declares (CR 113.6): a guard
/// conjunct `if this card is in your graveyard` (kept as the intervening
/// `if`, rechecked on resolution, CR 603.4), else a program that moves the
/// ability's own source out of a card zone (CR 113.6k) when the event does
/// not name the source. A source zone atom anywhere else in the guard, or two
/// different zones, fail explicitly.
fn declared_trigger_zone(
    cursor: &mut OracleTokenCursor<'_>,
    guard: Option<&SentenceConditionNode>,
    effects: &[Value],
    event: &GenericTriggerEventNode,
) -> MachineResult<TriggerZone> {
    cursor.enter("trigger.program.zone-of-function");
    let conjuncts: Vec<&SentenceConditionNode> = match guard {
        None => Vec::new(),
        Some(BooleanConditionNode::And(operands)) => operands.iter().collect(),
        Some(other) => vec![other],
    };
    let mut zones = conjuncts
        .iter()
        .filter_map(|conjunct| match conjunct {
            BooleanConditionNode::Atom(atom) => guard_zone_atom(atom),
            _ => None,
        })
        .collect::<Vec<_>>();
    zones.dedup();
    let total = guard.map_or(0, count_guard_zone_atoms);
    cursor.require(
        zones.len() <= 1 && total == conjuncts_with_zone(&conjuncts),
        "one zone of function stated by a conjunct of the intervening `if`",
    )?;
    if let Some(zone) = zones.first() {
        return Ok(*zone);
    }
    if event.functions_on_the_stack() {
        return Ok(TriggerZone::Stack);
    }
    // `When enchanted creature dies, return this card from your graveyard`
    // (CR 603.10a): the event reads what the source is attached to, which
    // only a permanent source has (CR 303.4); the ability looks back to the
    // source on the battlefield, so moving it from the graveyard declares
    // no other zone of function.
    if event_names_source(event) || event_reads_source_attachment(event) {
        return Ok(TriggerZone::Battlefield);
    }
    Ok(effects
        .iter()
        .find_map(program_source_origin)
        .unwrap_or(TriggerZone::Battlefield))
}

/// Every source zone atom of the guard, wherever it is.
fn count_guard_zone_atoms(condition: &SentenceConditionNode) -> usize {
    match condition {
        BooleanConditionNode::Atom(atom) => usize::from(guard_zone_atom(atom).is_some()),
        BooleanConditionNode::And(operands) | BooleanConditionNode::Or(operands) => {
            operands.iter().map(count_guard_zone_atoms).sum()
        }
        BooleanConditionNode::Not(operand) => count_guard_zone_atoms(operand),
    }
}

/// The source zone atoms that are direct conjuncts of the guard.
fn conjuncts_with_zone(conjuncts: &[&SentenceConditionNode]) -> usize {
    conjuncts
        .iter()
        .filter(|conjunct| {
            matches!(conjunct, BooleanConditionNode::Atom(atom) if guard_zone_atom(atom).is_some())
        })
        .count()
}

/// Whether a lowered effect adds mana anywhere in its nested programs.
fn adds_mana(effect: &Value) -> bool {
    match effect {
        Value::Object(map) => {
            map.get("kind").and_then(Value::as_str) == Some("addMana")
                || map.values().any(adds_mana)
        }
        Value::Array(items) => items.iter().any(adds_mana),
        _ => false,
    }
}

fn guard_excludes_source(guard: &SentenceConditionNode) -> bool {
    condition_any(guard, &ConditionAtom::excludes_source)
}

/// Whether the triggering event binds an object other than the ability's
/// source, which an `other` of the guard would then exclude instead of the
/// source (`a Forest you control enters, if you control five other Forests`).
fn guard_other_names_event_object(event: &GenericTriggerEventNode) -> bool {
    if event.observed_object_is_source() {
        return false;
    }
    event.observed_object().is_some()
        || event_frame(event)
            .event_object
            .is_some_and(|slot| !matches!(slot.reference, PermanentEffectReference::Source(_)))
}

#[cfg(test)]
#[path = "generic_trigger_attempt_tests.rs"]
mod generic_trigger_attempt_tests;

#[cfg(test)]
#[path = "trigger_guard_condition_tests.rs"]
mod trigger_guard_condition_tests;

#[cfg(test)]
#[path = "trigger_guard_other_tests.rs"]
mod trigger_guard_other_tests;

#[cfg(test)]
#[path = "trigger_frame_event_tests.rs"]
mod trigger_frame_event_tests;

#[cfg(test)]
#[path = "player_action_trigger_tests.rs"]
mod player_action_trigger_tests;

#[cfg(test)]
#[path = "counter_event_trigger_tests.rs"]
mod counter_event_trigger_tests;

#[cfg(test)]
#[path = "damage_event_trigger_tests.rs"]
mod damage_event_trigger_tests;
