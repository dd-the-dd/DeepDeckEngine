//! The trigger condition language: lists of complete trigger conditions
//! (CR 603.2), event-time conditions, attack declarations and their attacked
//! recipient, source and attached-object verbs, player-action lemmas and
//! state triggers (CR 603.8). Each case lowers through the one envelope and
//! the one event lowering; bodies are composed elsewhere.
use super::*;
use crate::oracle::syntax::recognize_entities;

/// The lowered event and the ability's event-time condition, or the
/// failure of the envelope or of the lowering.
fn lowered(text: &str) -> Result<(Value, Option<Value>), MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("trigger-condition-language-test", &tokens);
    let envelope = match try_recognize_generic_trigger_envelope(&mut cursor, &[], None) {
        GenericTriggerEnvelopeAttempt::Recognized(envelope) => envelope,
        GenericTriggerEnvelopeAttempt::FailedCommitted(failure)
        | GenericTriggerEnvelopeAttempt::RejectedCandidate(failure) => return Err(failure),
        GenericTriggerEnvelopeAttempt::NotClaimed => panic!("{text}: not claimed"),
    };
    let event = lower_generic_trigger_event(&mut cursor, &envelope.event)?;
    let condition = envelope
        .event_condition
        .as_ref()
        .map(|condition| lower_event_time_condition(&mut cursor, condition))
        .transpose()?;
    Ok((event, condition))
}

fn event(text: &str) -> Value {
    lowered(text)
        .unwrap_or_else(|failure| panic!("{text}: {failure}"))
        .0
}

fn failure(text: &str) -> MachineFailure {
    lowered(text).expect_err(text)
}

#[test]
fn a_trigger_condition_list_lowers_every_member_into_one_union() {
    // Unshakable Tail / Wildfire Devils: entry and a step, each with its own
    // trigger word.
    let lowered =
        event("When this creature enters and at the beginning of your upkeep, surveil 1.");
    assert_eq!(lowered["kind"], "oneOf");
    assert_eq!(
        lowered["events"],
        json!([
            {"kind": "enterBattlefield", "object": self_ref()},
            {"kind": "stepBegan", "step": "upkeep", "player": controller()},
        ])
    );
    // Voice of Resurgence: a cast with its turn owner, then a death.
    let lowered = event(
        "Whenever an opponent casts a spell during your turn and when this creature dies, draw a card.",
    );
    let events = lowered["events"].as_array().expect("a union");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "spellCast");
    assert_eq!(events[0]["opponentOfSourceController"], true);
    assert_eq!(events[0]["duringControllerTurn"], true);
    assert_eq!(
        events[1],
        json!({"kind": "permanentDied", "object": self_ref()})
    );
    // Flaring Cinder: entry and a filtered cast.
    let lowered = event(
        "When this creature enters and whenever you cast a spell with mana value 4 or greater, draw a card.",
    );
    assert_eq!(lowered["events"][1]["kind"], "spellCast");
}

#[test]
fn a_member_union_is_flattened_and_the_source_union_needs_no_list() {
    // Sibling without new code: the source union `enters or dies`.
    assert_eq!(
        event("When this creature enters or dies, draw a card.")["events"],
        json!([
            {"kind": "enterBattlefield", "object": self_ref()},
            {"kind": "permanentDied", "object": self_ref()},
        ])
    );
    // A member that is itself a union is flattened in written order.
    let lowered = event(
        "Whenever this creature attacks or blocks and at the beginning of your end step, draw a card.",
    );
    let kinds = lowered["events"]
        .as_array()
        .expect("a union")
        .iter()
        .map(|event| event["kind"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            json!("declaredAttacker"),
            json!("declaredBlocker"),
            json!("stepBegan")
        ]
    );
}

#[test]
fn and_without_a_trigger_word_coordinates_nothing() {
    let failure = failure("When this creature enters and attacks, draw a card.");
    assert!(!failure.is_engine_gap(), "{failure}");
}

#[test]
fn a_while_condition_is_the_event_time_condition_of_the_ability() {
    // Pugnacious Hammerskull.
    let (event, condition) = lowered(
        "Whenever this creature attacks while you don't control another Dinosaur, put a stun counter on it.",
    )
    .expect("a conditioned attack");
    assert_eq!(
        event,
        json!({"kind": "declaredAttacker", "object": self_ref()})
    );
    assert!(condition.is_some(), "the condition is never dropped");
    // An unmatched turn owner composes with the written condition.
    let (event, condition) = lowered(
        "Whenever you gain life during your turn while you control an artifact, draw a card.",
    )
    .expect("a conditioned life gain");
    assert_eq!(event["kind"], "lifeGained");
    let condition = condition.expect("an event-time condition");
    assert_eq!(condition["kind"], "and");
    assert_eq!(
        condition["operands"][0],
        json!({"kind": "duringControllerTurn", "player": controller()})
    );
}

#[test]
fn a_member_condition_lowers_only_where_its_matcher_evaluates_it() {
    // A previously unused combination of supported concepts.
    let (event, condition) = lowered(
        "Whenever you attack with two or more creatures while you control an artifact, and whenever a creature you control is turned face up, draw a card.",
    )
    .expect("a list with a conditioned member");
    assert!(
        condition.is_none(),
        "a member condition is not the ability's"
    );
    let events = event["events"].as_array().expect("a union");
    assert_eq!(events[0]["kind"], "controlledCreaturesAttacked");
    assert_eq!(events[0]["minimum"], 2);
    assert!(events[0].get("eventCondition").is_some());
    assert_eq!(events[1]["kind"], "turnedFaceUp");
    assert_eq!(events[1]["object"]["controller"], controller());
    assert!(events[1].get("eventCondition").is_none());
    // Near miss: the life-gain observer does not evaluate member conditions.
    let failure = failure(
        "Whenever you gain life while you control an artifact, and whenever you draw a card, draw a card.",
    );
    assert_eq!(failure.state, "engine.gap.event-matcher");
}

#[test]
fn attack_declarations_count_their_attackers() {
    // Legion's Landing.
    assert_eq!(
        event("When you attack with three or more creatures, draw a card.")["minimum"],
        3
    );
    // A one-or-more set needs no count.
    let lowered = event("Whenever you attack with one or more Wraiths, draw a card.");
    assert_eq!(lowered["kind"], "controlledCreaturesAttacked");
    assert!(lowered.get("minimum").is_none());
    assert!(lowered.get("where").is_some());
    // The same declaration with the attackers as subject.
    assert_eq!(
        event("Whenever two or more creatures you control attack, draw a card.")["minimum"],
        2
    );
    // Battalion / Odric: the source and at least N other creatures.
    let lowered =
        event("Whenever this creature and at least two other creatures attack, draw a card.");
    assert_eq!(lowered["kind"], "controlledCreaturesAttacked");
    assert_eq!(lowered["sourceMustAttack"], true);
    assert_eq!(lowered["minimum"], 3);
}

#[test]
fn attack_declarations_refuse_what_their_matcher_cannot_count() {
    // The matcher filters every attacker, the source included.
    assert_eq!(
        failure("Whenever this creature and at least two other Knights attack, draw a card.").state,
        "engine.gap.event-matcher"
    );
    // Only the controller's declaration is counted.
    assert_eq!(
        failure("Whenever an opponent attacks with two or more creatures, draw a card.").state,
        "trigger.attack-declaration.attacker-support"
    );
}

#[test]
fn the_attacked_object_is_an_attribute_of_the_attack_pattern() {
    let lowered =
        event("Whenever a creature attacks you or a planeswalker you control, draw a card.");
    assert_eq!(lowered["kind"], "eventPattern");
    assert_eq!(lowered["lemma"], "attack");
    let alternatives = lowered["attacked"]["anyOf"]
        .as_array()
        .expect("alternatives");
    assert_eq!(alternatives.len(), 2);
    assert_eq!(
        alternatives[0],
        json!({"kind": "player", "player": controller()})
    );
    assert_eq!(alternatives[1]["kind"], "permanents");
    assert_eq!(alternatives[1]["controller"], controller());
    // A batch of attackers has no per-attacker recipient matcher.
    assert_eq!(
        failure("Whenever one or more creatures you control attack an opponent, draw a card.")
            .state,
        "engine.gap.event-matcher"
    );
}

#[test]
fn source_state_changes_lower_to_their_self_observers() {
    // Monastery Loremaster.
    assert_eq!(
        event("When this creature is turned face up, draw a card."),
        json!({"kind": "turnedFaceUp", "object": self_ref()})
    );
    // Arbiter of the Ideal, Oreskos Sun Guide (inspired).
    assert_eq!(
        event("Whenever this creature becomes untapped, you gain 2 life."),
        json!({"kind": "permanentUntapped", "object": self_ref()})
    );
    let lowered = event("Whenever another creature you control is turned face up, draw a card.");
    assert_eq!(lowered["object"]["excludeSource"], true);
}

#[test]
fn the_attached_object_takes_the_shared_event_verbs() {
    // Sleeping Potion.
    let lowered =
        event("When enchanted creature becomes the target of a spell or ability, draw a card.");
    assert_eq!(lowered["kind"], "becameTarget");
    assert_eq!(
        lowered["object"],
        json!({"kind": "attachedPermanent", "attachment": self_ref()})
    );
    assert!(lowered.get("targetingStackObject").is_some());
    // Fertile Ground, Wild Growth.
    assert_eq!(
        event("Whenever enchanted land is tapped for mana, draw a card.")["kind"],
        "attachedPermanentManaAbilityActivated"
    );
    // Sibling with no new code: the shared passive damage clause.
    assert_eq!(
        event("Whenever enchanted creature is dealt damage, draw a card."),
        json!({
            "kind": "eventPattern",
            "lemma": "dealDamage",
            "recipient": {"kind": "attachedPermanent", "attachment": self_ref()},
        })
    );
}

#[test]
fn player_action_lemmas_lower_to_their_activation_observers() {
    // Manabarbs, Heartbeat of Spring.
    let lowered = event("Whenever a player taps a land for mana, draw a card.");
    assert_eq!(lowered["kind"], "controlledPermanentManaAbilityActivated");
    assert_eq!(lowered["anyPlayer"], true);
    // Mirari's Wake.
    assert_eq!(
        event("Whenever you tap a land for mana, draw a card.")["player"],
        controller()
    );
    assert_eq!(
        event("Whenever an opponent activates an ability that isn't a mana ability, draw a card."),
        json!({"kind": "nonManaAbilityActivated", "opponentOfSourceController": true})
    );
    assert_eq!(
        event("Whenever you activate a loyalty ability, draw a card.")["loyaltyOnly"],
        true
    );
    // Recognized, without a runtime matcher: an explicit engine gap.
    assert_eq!(
        failure("Whenever you commit a crime, draw a card.").state,
        "engine.gap.event-matcher"
    );
    // A bare ability names no kind the activation record distinguishes.
    assert_eq!(
        failure("Whenever you activate an ability, draw a card.").state,
        "event.player.ability-query.relative"
    );
}

#[test]
fn a_counter_threshold_on_the_source_is_a_state_trigger() {
    // Dark Depths.
    assert_eq!(
        event("When this land has no ice counters on it, draw a card."),
        json!({
            "kind": "stateConditionMet",
            "condition": {
                "kind": "compare",
                "left": {"kind": "countCounters", "object": self_ref(), "counter": "ice"},
                "operator": "==",
                "right": {"kind": "integer", "value": 0},
            },
        })
    );
    assert_eq!(
        event("When this creature has five or more +1/+1 counters on it, draw a card.")["condition"]
            ["operator"],
        ">="
    );
}

#[test]
fn an_entry_origin_is_an_attribute_of_the_entry_pattern() {
    assert_eq!(
        event("When this creature enters from your graveyard, draw a card."),
        json!({"kind": "enterBattlefield", "object": self_ref(), "fromZone": "graveyard"})
    );
    let lowered = event("Whenever a creature enters from a graveyard, draw a card.");
    assert_eq!(lowered["kind"], "permanentEntered");
    assert_eq!(lowered["fromZone"], "graveyard");
    assert_eq!(
        event("Whenever a creature you control enters from exile, draw a card.")["fromZone"],
        "exile"
    );
    // Another permanent's owner relation is not recorded with the origin.
    assert_eq!(
        failure("Whenever a creature enters from your graveyard, draw a card.").state,
        "trigger.envelope.entry-origin-owner"
    );
}
