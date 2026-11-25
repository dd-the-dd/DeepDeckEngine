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
}

fn card_object(text: &str) -> PlayerEventCardObject {
    match read(text).unwrap_or_else(|failure| panic!("{text}: {failure}")) {
        LocalTriggerEventNode::PlayerEvent {
            card: Some(card), ..
        } => card,
        other => panic!("{text}: {other:?}"),
    }
}

#[test]
fn the_card_object_types_its_cardinality_and_cause() {
    // A plural card object is one batch of discarded cards (CR 603.2c),
    // distinct from one discarded card.
    assert_eq!(
        card_object("you discard one or more cards").selection,
        PlayerEventCardSelection::OneOrMore
    );
    assert_eq!(
        card_object("you discard a card").selection,
        PlayerEventCardSelection::One
    );
    let lands = card_object("an opponent discards one or more land cards");
    assert!(lands.is_batch());
    assert_ne!(
        lands.query.predicate,
        super::super::ObjectPredicate::ObjectKind(ObjectKind::Card)
    );
    // The source card itself (`When you discard this card`).
    assert!(card_object("you discard this card").is_source_card());
    // `you cycle this card` is the source-cycled event, not a card object.
    assert_eq!(
        read("you cycle this card")
            .expect_err("cycled source")
            .state,
        "event.player.cycled-source"
    );
}

#[test]
fn a_caused_discard_keeps_the_causing_spell_or_ability() {
    let tokens = recognize_oracle_entities(
        "a spell or ability an opponent controls causes you to discard this card",
    );
    let mut cursor = OracleTokenCursor::new("player-event-test", &tokens);
    let LocalTriggerEventNode::PlayerEvent {
        actor,
        actions,
        card: Some(card),
        ..
    } = recognize_caused_player_event(&mut cursor).expect("a caused discard")
    else {
        panic!("a player event")
    };
    assert_eq!(actor, EventActorScope::Controller);
    assert_eq!(actions, vec![PlayerEventAction::DiscardCard]);
    assert!(card.is_source_card());
    let cause = card.cause.expect("the cause");
    assert_eq!(
        cause.controller,
        Some(super::super::ObjectControllerScope::OpponentOfAbilityController)
    );
    assert_eq!(
        cause.kinds.len(),
        3,
        "a spell, an activated or triggered ability"
    );
}

#[test]
fn received_damage_reads_the_contracted_and_passive_forms() {
    for (text, actor, kind) in [
        (
            "you're dealt damage",
            EventActorScope::Controller,
            DamageKind::Any,
        ),
        (
            "combat damage is dealt to you",
            EventActorScope::Controller,
            DamageKind::Combat,
        ),
        (
            "noncombat damage is dealt to an opponent",
            EventActorScope::Opponent,
            DamageKind::Noncombat,
        ),
        (
            "a player is dealt damage",
            EventActorScope::AnyPlayer,
            DamageKind::Any,
        ),
    ] {
        assert_eq!(
            actions(text),
            (actor, vec![PlayerEventAction::BeDealtDamage(kind)], false),
            "{text}"
        );
    }
}

fn roll(minimum: Option<i64>, maximum: Option<i64>, batch: bool) -> PlayerEventAction {
    PlayerEventAction::RollDie(DieRollFilter {
        minimum,
        maximum,
        batch,
    })
}

#[test]
fn outcome_and_keyword_action_verbs_read_their_complement_in_the_actor_agreement() {
    use EventActorScope::{AnyPlayer, Controller, Opponent};
    use PlayerEventAction::{Clash, CommitCrime, CompleteDungeon, FlipCoin, Proliferate};
    for (text, actor, expected) in [
        ("you roll a die", Controller, roll(None, None, false)),
        // Brazen Dwarf: one occurrence per roll instruction.
        (
            "you roll one or more dice",
            Controller,
            roll(None, None, true),
        ),
        ("you roll a 6", Controller, roll(Some(6), Some(6), false)),
        // Steel Squirrel, Chittering Doom.
        (
            "you roll a 5 or higher on a die",
            Controller,
            roll(Some(5), None, false),
        ),
        (
            "you roll a 2 or lower",
            Controller,
            roll(None, Some(2), false),
        ),
        (
            "an opponent rolls a 4 or higher",
            Opponent,
            roll(Some(4), None, false),
        ),
        // Setzer, Zndrsplt, Karplusan Minotaur.
        ("you win a coin flip", Controller, FlipCoin { won: true }),
        (
            "a player wins a coin flip",
            AnyPlayer,
            FlipCoin { won: true },
        ),
        ("you lose a coin flip", Controller, FlipCoin { won: false }),
        // Sylvan Echoes, Marvo, Rebellion of the Flamekin.
        ("you clash", Controller, Clash { won: false }),
        ("you clash and win", Controller, Clash { won: true }),
        ("you win a clash", Controller, Clash { won: true }),
        // Ichor Aberration, Ezuri; an opponent's proliferation, no new code.
        ("you proliferate", Controller, Proliferate),
        ("an opponent proliferates", Opponent, Proliferate),
        // Varis, Silverymoon Ranger.
        ("you complete a dungeon", Controller, CompleteDungeon),
        ("an opponent commits a crime", Opponent, CommitCrime),
    ] {
        assert_eq!(actions(text), (actor, vec![expected], false), "{text}");
    }
    // Rohirrim Chargers: the exerted permanent is the event's permanent.
    let LocalTriggerEventNode::PlayerEvent {
        actions,
        permanent: Some(permanent),
        ..
    } = read("you exert a creature").unwrap()
    else {
        panic!("the exerted permanent")
    };
    assert_eq!(actions, vec![PlayerEventAction::Exert]);
    assert!(!permanent.exclude_source);
}

#[test]
fn outcome_verbs_refuse_complements_without_a_typed_filter() {
    for (text, state) in [
        // Only `or higher` / `or lower` compare a die result.
        ("you roll a 6 or more", "event.player.roll.comparator"),
        ("you roll a 1 or 2", "event.player.roll.comparator"),
        ("you roll the planar die", "event.player.roll.dice"),
        ("you roll a natural 20", "event.player.roll.result"),
        ("you lose a clash", "event.player.outcome.clash"),
        ("you complete a quest", "event.player.complete-dungeon"),
    ] {
        let failure = read(text).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
    // Agreement: the lemma's form must agree with the actor.
    for text in [
        "you rolls a die",
        "an opponent roll a die",
        "you proliferates",
    ] {
        assert!(read(text).is_err(), "{text}");
    }
}
