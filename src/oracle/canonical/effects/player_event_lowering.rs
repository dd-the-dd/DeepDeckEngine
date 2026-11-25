//! Lowering of player keyword-action events, state triggers and the
//! first-occurrence modifier to the runtime event kinds that observe them.
//!
//! Each player action is one runtime event of the acting player, matched
//! with the shared player relation (`event_actor_player`):
//!
//! ```text
//! commit a crime          crimeCommitted   CR 700.13
//! roll DIE-ROLL           dieRolled        CR 706.2 (minimum, maximum,
//!                                          aggregation `oneOrMore`)
//! win|lose a coin flip    coinFlipped      CR 705.2 (outcome)
//! clash [and win]         clashed          CR 701.30 (won)
//! proliferate             proliferated     CR 701.34
//! exert PERMANENT         permanentExerted CR 701.43 (where)
//! complete a dungeon      dungeonCompleted CR 309.7
//! When CONDITION          stateConditionMet CR 603.8
//! ```
//!
//! A combination without an inspected runtime matcher fails explicitly.
use super::super::*;

/// The runtime kind of a player action this module lowers; `None` for the
/// actions of the established player-event lowering.
fn outcome_kind(action: PlayerEventAction) -> Option<&'static str> {
    Some(match action {
        PlayerEventAction::CommitCrime => "crimeCommitted",
        PlayerEventAction::RollDie(_) => "dieRolled",
        PlayerEventAction::FlipCoin { .. } => "coinFlipped",
        PlayerEventAction::Clash { .. } => "clashed",
        PlayerEventAction::Proliferate => "proliferated",
        PlayerEventAction::Exert => "permanentExerted",
        PlayerEventAction::CompleteDungeon => "dungeonCompleted",
        _ => return None,
    })
}

fn outcome_engine_gap(cursor: &mut OracleTokenCursor<'_>, expected: &str) -> MachineFailure {
    cursor.enter("trigger.player-event.outcome-engine-gap");
    cursor.engine_gap(
        crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
        expected,
    )
}

/// Lowers one keyword-action or outcome event of a player. `None` when the
/// action belongs to the established player-event lowering.
pub(in crate::oracle::canonical) fn lower_player_outcome_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    action: PlayerEventAction,
    one_or_more: bool,
    permanent: Option<&PlayerEventCardObject>,
) -> Option<MachineResult<Value>> {
    let kind = outcome_kind(action)?;
    Some(lower_outcome(
        cursor,
        kind,
        actor,
        action,
        one_or_more,
        permanent,
    ))
}

fn lower_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    kind: &'static str,
    actor: EventActorScope,
    action: PlayerEventAction,
    one_or_more: bool,
    permanent: Option<&PlayerEventCardObject>,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.outcome-support");
    cursor.require(
        !one_or_more,
        "a per-player event (no one-or-more player batch for this action)",
    )?;
    let mut lowered = json!({"kind": kind, "player": event_actor_player(actor)});
    match action {
        PlayerEventAction::RollDie(filter) => {
            cursor.enter("trigger.player-event.die-result");
            cursor.require(
                !(filter.batch && (filter.minimum.is_some() || filter.maximum.is_some())),
                "a result filter on one die, not on a batch of dice",
            )?;
            cursor.require(
                filter
                    .minimum
                    .zip(filter.maximum)
                    .is_none_or(|(minimum, maximum)| minimum <= maximum),
                "a satisfiable die-result range",
            )?;
            if let Some(minimum) = filter.minimum {
                lowered["minimum"] = json!(minimum);
            }
            if let Some(maximum) = filter.maximum {
                lowered["maximum"] = json!(maximum);
            }
            if filter.batch {
                lowered["aggregation"] = json!("oneOrMore");
            }
        }
        PlayerEventAction::FlipCoin { won } => {
            lowered["outcome"] = json!(if won { "won" } else { "lost" });
        }
        PlayerEventAction::Clash { won } => {
            if won {
                lowered["won"] = json!(true);
            }
        }
        PlayerEventAction::Exert => {
            cursor.enter("trigger.player-event.exerted-permanent");
            let permanent =
                permanent.ok_or_else(|| cursor.fail("the exerted permanent of the event"))?;
            let predicate = &permanent.query.predicate;
            cursor.require(
                !permanent.exclude_source
                    && !predicate.contains_numeric_comparison()
                    && domain_filter_is_representable(predicate),
                "a representable exerted-permanent predicate without numeric characteristics",
            )?;
            let filter = lower_permanent_object_predicate(predicate);
            cursor.require(!filter.is_null(), "a lowered exerted-permanent predicate")?;
            lowered["where"] = filter;
        }
        PlayerEventAction::CompleteDungeon => {
            // The dungeon observer matches the source controller only.
            if actor != EventActorScope::Controller {
                return Err(outcome_engine_gap(
                    cursor,
                    "a dungeon-completion matcher for a player other than the ability controller",
                ));
            }
        }
        _ => {}
    }
    Ok(lowered)
}

/// `ACTOR activate(s) ABILITY [of SOURCE]` (CR 602.2): the non-mana
/// activation observer. The kind (loyalty) and the source object's card
/// predicate are attributes of the activation record; the actor is matched
/// with the shared player relation and bound as the triggering player.
pub(in crate::oracle::canonical) fn lower_ability_activation_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    kind: ActivatedAbilityKind,
    one_or_more: bool,
    source: Option<&PlayerEventCardObject>,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.activation-support");
    cursor.require(
        !one_or_more,
        "a per-player activation event (no one-or-more player batch)",
    )?;
    let mut lowered = match source {
        None => json!({"kind": "nonManaAbilityActivated"}),
        Some(source) => {
            cursor.enter("trigger.player-event.activation-source-support");
            let predicate = &source.query.predicate;
            // The activation record knows the source card's characteristics,
            // not its controller relation or its identity as the observer.
            cursor.require(
                source.query.controller.is_none()
                    && !source.exclude_source
                    && !predicate.contains_numeric_comparison()
                    && domain_filter_is_representable(predicate),
                "a representable ability-source predicate without controller, source exclusion or numeric characteristics",
            )?;
            let filter = lower_card_object_predicate(predicate);
            cursor.require(!filter.is_null(), "a lowered ability-source predicate")?;
            json!({"kind": "abilityActivated", "where": filter})
        }
    };
    match actor {
        EventActorScope::Controller => lowered["player"] = controller(),
        EventActorScope::Opponent => lowered["opponentOfSourceController"] = json!(true),
        EventActorScope::AnyPlayer => lowered["player"] = json!({"kind": "eachPlayer"}),
    }
    if kind == ActivatedAbilityKind::Loyalty {
        lowered["loyaltyOnly"] = json!(true);
    }
    Ok(lowered)
}

/// The causing spell or ability of a player action (CR 701.9): the resolving
/// stack object whose effect performed it, with its controller relation.
/// The runtime records the controller of the resolving spell or ability; it
/// does not distinguish a spell from an ability, so the selector must name
/// both.
fn lower_action_cause(
    cursor: &mut OracleTokenCursor<'_>,
    cause: &TargetingStackSelectorNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.cause-support");
    let every_kind = [
        TargetingStackKind::Spell,
        TargetingStackKind::ActivatedAbility,
        TargetingStackKind::TriggeredAbility,
    ]
    .iter()
    .all(|kind| cause.kinds.contains(kind));
    if !every_kind {
        return Err(outcome_engine_gap(
            cursor,
            "an action-cause matcher that tells a spell from an ability (the runtime records the resolving object's controller)",
        ));
    }
    let mut lowered = json!({"kind": "resolvingStackObject"});
    match cause.controller {
        None => {}
        Some(ObjectControllerScope::AbilityController) => lowered["controller"] = controller(),
        Some(ObjectControllerScope::OpponentOfAbilityController) => {
            lowered["controller"] = json!({"kind": "opponentsOf", "player": controller()});
        }
        Some(_) => return Err(cursor.fail("a controller relation of the causing spell or ability")),
    }
    Ok(lowered)
}

/// Player actions whose card object is not one card of a query: the source
/// card itself (`When you discard this card`, observed from the graveyard,
/// CR 113.6) and a `one or more` batch (one occurrence per action, CR
/// 603.2c), and any action with a written cause. `None` for the per-card
/// actions of the established lowering.
pub(in crate::oracle::canonical) fn lower_player_card_selection_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    actions: &[PlayerEventAction],
    one_or_more: bool,
    card: Option<&PlayerEventCardObject>,
) -> Option<MachineResult<Value>> {
    let card = card?;
    if card.selection == PlayerEventCardSelection::One && card.cause.is_none() {
        return None;
    }
    Some(lower_card_selection(
        cursor,
        actor,
        actions,
        one_or_more,
        card,
    ))
}

fn lower_card_selection(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    actions: &[PlayerEventAction],
    one_or_more: bool,
    card: &PlayerEventCardObject,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.card-selection-support");
    if actions == [PlayerEventAction::CounterSpell] {
        return lower_spell_countered_event(cursor, actor, card);
    }
    cursor.require(
        actions == [PlayerEventAction::DiscardCard] && !one_or_more,
        "one discard event of one player (no other action or player batch on this card object)",
    )?;
    let player = event_actor_player(actor);
    let mut lowered = match card.selection {
        // The card is its owner's: `you` discard it from your hand.
        PlayerEventCardSelection::Source => {
            cursor.require(
                actor == EventActorScope::Controller,
                "the card's owner as the discarding player (`you discard this card`)",
            )?;
            json!({"kind": "selfDiscarded", "object": self_ref()})
        }
        PlayerEventCardSelection::OneOrMore => {
            let predicate = &card.query.predicate;
            cursor.require(
                !predicate.contains_numeric_comparison()
                    && domain_filter_is_representable(predicate),
                "a representable card predicate without numeric characteristics",
            )?;
            let mut lowered = json!({"kind": "cardsDiscarded", "player": player});
            if *predicate != ObjectPredicate::ObjectKind(ObjectKind::Card) {
                let filter = lower_card_object_predicate(predicate);
                cursor.require(!filter.is_null(), "a lowered card predicate")?;
                lowered["where"] = filter;
            }
            if card.exclude_source {
                lowered["excludeSource"] = json!(true);
            }
            lowered
        }
        PlayerEventCardSelection::One => {
            return Err(outcome_engine_gap(
                cursor,
                "a per-card discard matcher that evaluates the action's cause",
            ));
        }
    };
    if let Some(cause) = &card.cause {
        if card.selection != PlayerEventCardSelection::Source {
            return Err(outcome_engine_gap(
                cursor,
                "a batch discard matcher that evaluates the action's cause",
            ));
        }
        lowered["cause"] = lower_action_cause(cursor, cause)?;
    }
    Ok(lowered)
}

/// `a spell or ability ACTOR controls counters a SPELL` (CR 701.6): the
/// countered-spell observer, the countering object's controller matched with
/// the shared player relation, the countered spell with the shared spell
/// predicate.
fn lower_spell_countered_event(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    card: &PlayerEventCardObject,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.counter-support");
    let cause = card
        .cause
        .as_ref()
        .ok_or_else(|| cursor.fail("the countering spell or ability"))?;
    // The countering object's kind is not recorded; its controller is.
    let mut lowered = lower_action_cause(cursor, cause)?;
    lowered["kind"] = json!("spellCounteredBy");
    lowered["controller"] = event_actor_player(actor);
    let predicate = &card.query.predicate;
    cursor.require(
        !predicate.contains_numeric_comparison(),
        "a countered spell predicate without numeric characteristics",
    )?;
    if *predicate != ObjectPredicate::ObjectKind(ObjectKind::Spell) {
        let filter = lower_spell_object_predicate(predicate)
            .ok_or_else(|| cursor.fail("a lowered countered-spell predicate"))?;
        lowered["where"] = filter;
    }
    Ok(lowered)
}

/// `PLAYER is dealt [KIND] damage` (CR 120.3): one received-damage record
/// per damaged player and source, or one event for a `one or more` batch.
/// The established opponent observer keeps its kind; any other player
/// relation is the received-damage observer matched with the shared player
/// relation (`playerDealtDamage`). The caller adds the damage kind.
pub(in crate::oracle::canonical) fn lower_player_received_damage(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    player: Value,
    one_or_more: bool,
) -> MachineResult<Value> {
    cursor.enter("trigger.player-event.received-damage");
    let kind = match actor {
        EventActorScope::Opponent => "opponentDealtDamage",
        EventActorScope::Controller | EventActorScope::AnyPlayer => {
            // One player cannot be a batch of players.
            cursor.require(
                !one_or_more || actor == EventActorScope::AnyPlayer,
                "a one-or-more batch of several players",
            )?;
            "playerDealtDamage"
        }
    };
    Ok(json!({
        "kind": kind,
        "player": player,
        "aggregation": if one_or_more { "oneOrMore" } else { "perRecipient" },
    }))
}

/// The runtime event kinds whose matcher evaluates `firstTimeEachTurn`
/// against the turn history of the acting player (`previous_turn_events.rs`).
const FIRST_OCCURRENCE_KINDS: [&str; 3] = ["lifeGained", "lifeLost", "abilityPerformed"];

/// `for the first time each turn` on a lowered player event: only the first
/// occurrence of that event this turn triggers. The runtime history keys
/// the occurrence by the acting player, so the actor must be one player.
pub(in crate::oracle::canonical) fn apply_first_time_each_turn(
    cursor: &mut OracleTokenCursor<'_>,
    mut lowered: Value,
    first_time_each_turn: bool,
) -> MachineResult<Value> {
    if !first_time_each_turn {
        return Ok(lowered);
    }
    let kind = lowered["kind"].as_str().unwrap_or_default();
    if !FIRST_OCCURRENCE_KINDS.contains(&kind) || lowered["player"] != controller() {
        return Err(outcome_engine_gap(
            cursor,
            "an engine event matcher for `for the first time each turn` on this event",
        ));
    }
    lowered["firstTimeEachTurn"] = json!(true);
    Ok(lowered)
}

/// `When CONDITION,` (CR 603.8): the engine's state trigger, checked
/// whenever a player would receive priority, with the condition lowered by
/// the shared condition lowering (the ability source is its only object).
pub(in crate::oracle::canonical) fn lower_state_trigger_event(
    cursor: &mut OracleTokenCursor<'_>,
    condition: &SentenceConditionNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.state-trigger.lowering");
    let condition = lower_sentence_condition(
        cursor,
        condition,
        &ConditionObjectBindings {
            permanent_targets: &[],
            card_targets: &[],
            card_focus: None,
            previous_action_result: None,
        },
    )?;
    Ok(json!({"kind": "stateConditionMet", "condition": condition}))
}

/// `one or more|N or more ATTACKERS attack PLAYER` (CR 508.1, 603.2c): one
/// event per attacked player of one declaration, counting the attackers
/// that attack that player directly (`attackersAttackedPlayer`). The
/// attackers keep their written controller relation (`you control`, none
/// for `creatures attack you`); the attacked player is one player relation.
pub(in crate::oracle::canonical) fn lower_attackers_attacked_player(
    cursor: &mut OracleTokenCursor<'_>,
    event: &ObjectEventNode,
) -> MachineResult<Value> {
    cursor.enter("trigger.attack.attacked-player-support");
    let [AttackedRecipientNode::Player(attacked)] = event.attacked.as_slice() else {
        return Err(outcome_engine_gap(
            cursor,
            "an attack-declaration matcher with a per-attacker attacked recipient",
        ));
    };
    let subject = &event.subject;
    let EventMultiplicity::PerEvent { minimum } = subject.multiplicity else {
        return Err(cursor.fail("`N or more` attackers of one declaration"));
    };
    cursor.require(
        !subject.exclude_source && !subject.query.exclude_source && !event.alone,
        "attackers without a source exclusion or `alone`",
    )?;
    let predicate = &subject.query.predicate;
    cursor.require(
        predicate.supports_domain(ObjectKind::Permanent)
            && domain_filter_is_representable(predicate),
        "a representable attacking-permanent predicate",
    )?;
    let mut attackers = json!({});
    match subject.query.controller {
        None => {}
        Some(ObjectControllerScope::AbilityController) => attackers["controller"] = controller(),
        Some(ObjectControllerScope::OpponentOfAbilityController) => {
            attackers["controller"] = json!({"kind": "opponentsOf", "player": controller()});
        }
        Some(_) => return Err(cursor.fail("a positive controller relation for an attacker")),
    }
    if *predicate != ObjectPredicate::ObjectKind(ObjectKind::Permanent) {
        let filter = lower_permanent_object_predicate(predicate);
        cursor.require(!filter.is_null(), "a lowered attacking-permanent predicate")?;
        attackers["where"] = filter;
    }
    if minimum > 1 {
        attackers["minimum"] = json!(minimum);
    }
    Ok(json!({
        "kind": "attackersAttackedPlayer",
        "attackers": attackers,
        "attacked": event_actor_player(*attacked),
    }))
}

/// The controller of the permanent the source is attached to, as a lowered
/// player reference (`its controller` after `enchanted land is tapped`).
fn attached_permanent_controller(player: &Value) -> bool {
    let object = &player["object"];
    match player["kind"].as_str() {
        Some("controllerOfAttachedPermanent") => player["attachment"] == self_ref(),
        Some("controllerOf") => match object["kind"].as_str() {
            Some("triggeringPermanent") => true,
            Some("attachedPermanent") => object["attachment"] == self_ref(),
            _ => false,
        },
        _ => false,
    }
}

/// `its controller adds ...` after `Whenever enchanted land is tapped for
/// mana`: the performer of the mana action is the enchanted permanent's
/// controller, who adds the mana to their own pool (CR 106.4). The action
/// names that player directly, the one shape a folded triggered mana
/// ability carries.
pub(in crate::oracle::canonical) fn normalize_attached_mana_performers(effects: &mut [Value]) {
    for effect in effects.iter_mut() {
        if effect["kind"] != "addMana" || !attached_permanent_controller(&effect["performer"]) {
            continue;
        }
        if let Some(fields) = effect.as_object_mut()
            && let Some(performer) = fields.remove("performer")
        {
            fields.insert("player".to_string(), performer);
        }
    }
}

/// A triggered mana ability of an attachment (CR 605.1b) that runs as part
/// of the attached permanent's mana ability (CR 605.4a): every action adds
/// mana to that permanent's controller and carries nothing else.
pub(in crate::oracle::canonical) fn attached_mana_trigger_is_folded(effects: &[Value]) -> bool {
    !effects.is_empty()
        && effects.iter().all(|effect| {
            effect["kind"] == "addMana"
                && attached_permanent_controller(&effect["player"])
                && effect.as_object().is_some_and(|fields| {
                    fields
                        .keys()
                        .all(|key| matches!(key.as_str(), "kind" | "player" | "mana"))
                })
        })
}

#[cfg(test)]
#[path = "player_event_lowering_tests.rs"]
mod tests;
