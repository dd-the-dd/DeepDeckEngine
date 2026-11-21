use super::super::{
    CardDefinition, DecisionProvider, EngineDecisionRequest, EngineError, GameSetup, GameState,
    PlayerDeck, card_instance,
};
use super::*;
use serde_json::json;

fn engine() -> GameEngine {
    let definition: CardDefinition =
        serde_json::from_value(json!({"id":"land","name":"Land","typeLine":"Land","rules":[]}))
            .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 4],
                })
                .collect(),
        },
        7311,
    )
    .unwrap();
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &definition, 50));
    engine
}

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("unexpected untap-step decision"))
    }
}

#[test]
fn stun_replaces_one_real_transition_and_not_an_already_untapped_object() {
    for (tapped, count, outcome, remaining) in [
        (true, 1, UntapAttemptOutcome::StunCounterRemoved, 0),
        (true, 3, UntapAttemptOutcome::StunCounterRemoved, 2),
        (false, 3, UntapAttemptOutcome::Unchanged, 3),
        (true, 0, UntapAttemptOutcome::Untapped, 0),
    ] {
        let mut engine = engine();
        let card = &mut engine.state.players[0].battlefield[0];
        card.tapped = tapped;
        card.counters.insert("stun".into(), count);
        let id = card.instance_id.clone();
        let previous = engine.state.events.len();
        assert_eq!(engine.attempt_untap_permanent(&id), outcome);
        let card = &engine.state.players[0].battlefield[0];
        assert_eq!(card.tapped, tapped && count > 0);
        assert_eq!(card.counters.get("stun").copied().unwrap_or(0), remaining);
        assert_eq!(
            engine.state.events[previous..]
                .iter()
                .filter(|e| e.kind == "counterRemoved")
                .count(),
            usize::from(tapped && count > 0)
        );
        assert_eq!(
            engine.attempt_untap_permanent("missing"),
            UntapAttemptOutcome::Unchanged
        );
    }
}

#[test]
fn step_restrictions_prevent_the_attempt_and_do_not_consume_stun() {
    for skip in [false, true] {
        let mut engine = engine();
        let card = &mut engine.state.players[0].battlefield[0];
        card.tapped = true;
        card.counters.insert("stun".into(), 2);
        let id = card.instance_id.clone();
        if skip {
            engine
                .state
                .rule_modifiers
                .push(json!({"kind":"skipNextUntap","cardInstanceId":id}));
        } else {
            engine.state.players[0].battlefield[0]
                .flags
                .insert("doesNotUntap".into(), true);
        }
        let previous = engine.state.events.len();
        engine.start_turn(&mut NoChoices).unwrap();
        let card = &engine.state.players[0].battlefield[0];
        assert!(card.tapped);
        assert_eq!(card.counters.get("stun"), Some(&2));
        assert!(
            !engine.state.events[previous..]
                .iter()
                .any(|e| e.kind == "counterRemoved")
        );
    }
}

#[test]
fn step_and_effect_share_transition_but_effect_does_not_apply_step_restrictions() {
    let mut engine = engine();
    let card = &mut engine.state.players[0].battlefield[0];
    card.tapped = true;
    card.counters.insert("stun".into(), 2);
    let id = card.instance_id.clone();
    engine.start_turn(&mut NoChoices).unwrap();
    assert!(engine.state.players[0].battlefield[0].tapped);
    assert_eq!(
        engine.state.players[0].battlefield[0].counters.get("stun"),
        Some(&1)
    );
    assert_eq!(
        engine.attempt_untap_permanent(&id),
        UntapAttemptOutcome::StunCounterRemoved
    );
    engine.state.players[0].battlefield[0]
        .flags
        .insert("doesNotUntap".into(), true);
    assert_eq!(
        engine.attempt_untap_permanent(&id),
        UntapAttemptOutcome::Untapped
    );
}

#[test]
fn one_collection_action_does_not_attempt_the_same_object_twice() {
    use super::super::CanonicalRuleExecution;
    use std::collections::BTreeMap;
    let mut engine = engine();
    let card = &mut engine.state.players[0].battlefield[0];
    // The public executor selects one source definition, independently of the
    // repeated object identities supplied to the collection action below.
    card.definition.id = "untap-source".into();
    card.tapped = true;
    card.counters.insert("stun".into(), 3);
    let id = card.instance_id.clone();
    engine.execute_canonical_rule(CanonicalRuleExecution {
        controller_id:"p0".into(),source_definition_id:"untap-source".into(),
        rule:json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
            {"kind":"untapPermanents","objects":{"kind":"decisionResult","decisionId":"selected"}}
        ]}),
        decisions:BTreeMap::from([("selected".into(),json!([id,id]))]),targets:BTreeMap::new(),
    },&mut NoChoices).unwrap();
    assert!(engine.state.players[0].battlefield[0].tapped);
    assert_eq!(
        engine.state.players[0].battlefield[0].counters.get("stun"),
        Some(&2)
    );
}

#[test]
fn phased_objects_do_not_attempt_untap_but_phase_in_precedes_the_own_step_attempt() {
    let mut engine = engine();
    let card = &mut engine.state.players[0].battlefield[0];
    card.tapped = true;
    card.counters.insert("stun".into(), 2);
    card.flags.insert("phasedOut".into(), true);
    let id = card.instance_id.clone();
    let previous = engine.state.events.len();
    assert_eq!(
        engine.attempt_untap_permanent(&id),
        UntapAttemptOutcome::Unchanged
    );
    assert!(engine.state.players[0].battlefield[0].tapped);
    assert_eq!(
        engine.state.players[0].battlefield[0].counters.get("stun"),
        Some(&2)
    );
    assert_eq!(engine.state.events.len(), previous);
    engine.start_turn(&mut NoChoices).unwrap();
    let events = &engine.state.events[previous..];
    let phase = events
        .iter()
        .position(|e| e.kind == "permanentPhasedIn")
        .unwrap();
    let removed = events
        .iter()
        .position(|e| e.kind == "counterRemoved")
        .unwrap();
    assert!(phase < removed);
    assert!(engine.state.players[0].battlefield[0].tapped);
    assert_eq!(
        engine.state.players[0].battlefield[0].counters.get("stun"),
        Some(&1)
    );
}
