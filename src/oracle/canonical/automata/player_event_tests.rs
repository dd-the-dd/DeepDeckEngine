use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn read(text: &str) -> MachineResult<LocalTriggerEventNode> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("player-event-test", &tokens);
    recognize_player_event(&mut cursor)
}

fn actions(text: &str) -> (EventActorScope, Vec<PlayerEventAction>, bool) {
    match read(text).unwrap_or_else(|failure| panic!("{text}: {failure}")) {
        LocalTriggerEventNode::PlayerEvent {
            actor,
            actions,
            card,
            ..
        } => (actor, actions, card.is_some()),
        other => panic!("{text}: {other:?}"),
    }
}

#[test]
fn named_keyword_actions_come_from_the_catalog_and_agree_with_the_actor() {
    use PlayerEventAction::NamedAction;
    for (text, actor, expected) in [
        (
            "you scry",
            EventActorScope::Controller,
            vec![NamedAction("scry")],
        ),
        (
            "an opponent scries",
            EventActorScope::Opponent,
            vec![NamedAction("scry")],
        ),
        (
            "you scry or surveil",
            EventActorScope::Controller,
            vec![NamedAction("scry"), NamedAction("surveil")],
        ),
        (
            "a player surveils",
            EventActorScope::AnyPlayer,
            vec![NamedAction("surveil")],
        ),
    ] {
        assert_eq!(actions(text), (actor, expected, false), "{text}");
    }
    // Agreement is checked against the catalog's base form.
    for text in ["you scries", "an opponent scry", "you scry or surveils"] {
        assert!(read(text).is_err(), "{text}");
    }
}

#[test]
fn discard_and_cycling_share_the_actor_and_one_card_object() {
    use PlayerEventAction::{CycleCard, DiscardCard};
    assert_eq!(
        actions("you discard a card"),
        (EventActorScope::Controller, vec![DiscardCard], true)
    );
    assert_eq!(
        actions("an opponent discards a creature card"),
        (EventActorScope::Opponent, vec![DiscardCard], true)
    );
    assert_eq!(
        actions("you cycle or discard another card"),
        (
            EventActorScope::Controller,
            vec![CycleCard, DiscardCard],
            true
        )
    );
    let LocalTriggerEventNode::PlayerEvent {
        card: Some(card), ..
    } = read("you cycle or discard another card").unwrap()
    else {
        panic!("shared card object")
    };
    assert!(card.exclude_source);
}

#[test]
fn player_event_unions_reject_mixed_objects_and_repeated_verbs() {
    // A named action takes no card; a union cannot attach the card to one branch.
    let failure = read("you scry or discard a card").expect_err("mixed union");
    assert_eq!(failure.state, "event.player.union-distinct");
    let failure = read("you discard or discard a card").expect_err("repeated verb");
    assert_eq!(failure.state, "event.player.union-distinct");
    // The card object is a card query, not a player or permanent relation.
    let failure = read("you discard a creature you control").expect_err("controller relation");
    assert_eq!(failure.state, "event.player.card-domain");
    // A plural card object is a batch, not one discarded card.
    let failure = read("you discard one or more cards").expect_err("plural object");
    assert_eq!(failure.state, "event.player.card-determiner");
}
