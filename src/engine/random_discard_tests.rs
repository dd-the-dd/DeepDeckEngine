use serde_json::json;

use super::super::{GameEvent, GameStep};
use super::{
    RANDOM_DISCARD_EVENT_KIND, RandomDiscardContractError, RandomDiscardProvenance,
    correlate_random_discard, random_discard_event_detail, random_selection_event_detail,
    validate_random_discard_contract,
};

fn event(player: &str, detail: serde_json::Value) -> GameEvent {
    GameEvent {
        sequence: 1,
        turn_number: 2,
        step: GameStep::PrecombatMain,
        kind: "cardsRandomlySelected".to_string(),
        player_id: Some(player.to_string()),
        card_instance_id: Some("source".to_string()),
        detail,
    }
}

#[test]
fn linked_random_discard_contract_is_typed_by_one_decision_id() {
    let effect = json!({
        "kind": "discardCards",
        "player": { "kind": "abilityController" },
        "cards": { "kind": "decisionResult", "decisionId": "randomDiscard" },
        "selectionProvenance": { "kind": "random", "decisionId": "randomDiscard" },
    });

    assert_eq!(
        validate_random_discard_contract(&effect),
        Ok(Some(RandomDiscardProvenance {
            decision_id: "randomDiscard".to_string(),
        }))
    );
}

#[test]
fn ordinary_discard_has_no_random_provenance() {
    let effect = json!({
        "kind": "discardCards",
        "player": { "kind": "abilityController" },
        "count": { "kind": "integer", "value": 1 },
    });

    assert_eq!(validate_random_discard_contract(&effect), Ok(None));
}

#[test]
fn mismatched_producer_and_consumer_ids_are_rejected() {
    let effect = json!({
        "kind": "discardCards",
        "cards": { "kind": "decisionResult", "decisionId": "selectedA" },
        "selectionProvenance": { "kind": "random", "decisionId": "selectedB" },
    });

    assert_eq!(
        validate_random_discard_contract(&effect),
        Err(RandomDiscardContractError::DecisionIdMismatch)
    );
}

#[test]
fn correlation_requires_actual_discards_from_the_matching_selection() {
    let selected = vec!["card-a".to_string(), "card-b".to_string()];
    let events = vec![event(
        "player-0",
        random_selection_event_detail("randomDiscard", "stack-1", &selected, 2, 2),
    )];
    let provenance = RandomDiscardProvenance {
        decision_id: "randomDiscard".to_string(),
    };

    let correlation =
        correlate_random_discard(&events, "stack-1", "player-0", &provenance, &selected)
            .expect("the consumer correlates with its random producer");
    assert_eq!(correlation.card_ids, selected);
    assert_eq!(
        random_discard_event_detail(&correlation),
        json!({
            "decisionId": "randomDiscard",
            "cardIds": ["card-a", "card-b"],
            "count": 2,
        })
    );
    assert_eq!(RANDOM_DISCARD_EVENT_KIND, "cardsDiscardedAtRandom");
}

#[test]
fn random_selection_without_a_discard_never_correlates() {
    let selected = vec!["card-a".to_string()];
    let events = vec![event(
        "player-0",
        random_selection_event_detail("randomDiscard", "stack-1", &selected, 1, 1),
    )];
    let provenance = RandomDiscardProvenance {
        decision_id: "randomDiscard".to_string(),
    };

    assert_eq!(
        correlate_random_discard(&events, "stack-1", "player-0", &provenance, &[]),
        None
    );
}

#[test]
fn unrelated_random_selection_cannot_prove_the_discard() {
    let selected = vec!["card-a".to_string()];
    let events = vec![event(
        "player-0",
        random_selection_event_detail("otherSelection", "stack-1", &selected, 1, 1),
    )];
    let provenance = RandomDiscardProvenance {
        decision_id: "randomDiscard".to_string(),
    };

    assert_eq!(
        correlate_random_discard(&events, "stack-1", "player-0", &provenance, &selected,),
        None
    );
}
